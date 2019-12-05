#[macro_use]
extern crate serde_derive;

use std::fmt::Debug;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::thread;

use uuid::Uuid;

use crate::ssdp::SsdpListener;
use crate::upnp::*;
use crate::virtual_device::wrappers::*;
use crate::virtual_device::*;

mod ssdp;
mod upnp;
pub mod virtual_device;

#[derive(Clone)]
pub(crate) struct RustmoDevice {
    pub(crate) name: String,
    pub(crate) ip_address: IpAddr,
    pub(crate) port: u16,
    pub(crate) uuid: Uuid,
    pub(crate) virtual_device: WrappedVirtualDevice,
}

impl RustmoDevice {
    pub fn new<T: VirtualDevice>(
        name: &str,
        ip_address: IpAddr,
        port: u16,
        virtual_device: T,
    ) -> Self {
        let mut bytes = Vec::from(name.as_bytes());
        while bytes.len() < 16 {
            bytes.push(bytes.len() as u8);
        }
        while bytes.len() > 16 {
            bytes.pop();
        }

        let device = RustmoDevice {
            name: name.to_string(),
            ip_address,
            port,
            uuid: Uuid::from_slice(bytes.as_slice())
                .ok()
                .expect("failed to generate UUID"),
            virtual_device: Arc::new(Mutex::new(Box::new(virtual_device))),
        };

        let cloned = device.clone();
        thread::spawn(move || {
            let server = hyper::Server::http(SocketAddr::new(ip_address, port)).unwrap();
            server.handle(DeviceHttpServerHandler::new(cloned)).unwrap();
        });

        device
    }
}

///
/// Create a `RustmoServer` and add devices to make them discoverable, and controllable via Alexa.
///
/// Each `VirtualDevice` you wish to expose requires a backing HTTP server (Rustmo takes care of
/// this for you).  This is the reason why the various `::add_xxx_device()` methods require a `port`
/// number.
///
/// Note that each device must be assigned a unique `port` number.
///
/// `RustmoServer` also creates a multicast UDP socket listener to implement the SSDP-based device
/// discovery protocol required by Alexa -- this listens on port 1900.
///
pub struct RustmoServer {
    devices: Arc<Mutex<Vec<RustmoDevice>>>,
    ip_address: Ipv4Addr,
    ssdp_listener: SsdpListener,
}

#[derive(Debug)]
pub enum RustmoError {
    DeviceAlreadyExistsOnPort(u16),
    DeviceAlreadyExistsByName(String),
}

impl RustmoServer {
    ///
    /// Create a new `RustmoServer` and listen for SSDP requests on the specified network interface
    ///
    pub fn new(interface: Ipv4Addr) -> Self {
        let devices = Arc::new(Mutex::new(Vec::new()));

        RustmoServer {
            devices: devices.clone(),
            ip_address: interface,
            ssdp_listener: SsdpListener::listen(interface, devices),
        }
    }

    ///
    /// Add a `VirtualDevice` to make it discoverable and controllable.
    ///
    /// `@name`:  The word or phrase you'll use when talking to Alexa to control this device
    /// `@port`:  The port on which the backing HTTP server will listen for UPNP requests
    /// `@virtual_device`:  A `VirtualDevice` implementation
    ///
    pub fn add_device<T: VirtualDevice>(
        &mut self,
        name: &str,
        port: u16,
        virtual_device: T,
    ) -> Result<WrappedVirtualDevice, RustmoError> {
        self.internal_add_device(name, IpAddr::V4(self.ip_address), port, virtual_device)
    }

    ///
    /// Add a `VirtualDevice` to make it discoverable and controllable.
    ///
    /// This version wraps the provided `VirtualDevice` such that it will poll (via
    /// `::check_is_on()`), up to 4 seconds, whenever `::turn_on()` or `::turn_off()` is called.
    ///
    /// This form is useful when controlling a physical device that takes a few seconds (but
    /// less than 5) for its state to register as changed.
    ///
    /// `@name`:  The word or phrase you'll use when talking to Alexa to control this device
    /// `@port`:  The port on which the backing HTTP server will listen for UPNP requests
    /// `@virtual_device`:  A `VirtualDevice` implementation
    ///
    pub fn add_polling_device<T: VirtualDevice>(
        &mut self,
        name: &str,
        port: u16,
        virtual_device: T,
    ) -> Result<WrappedVirtualDevice, RustmoError> {
        self.internal_add_device(
            name,
            IpAddr::V4(self.ip_address),
            port,
            PollingDevice {
                device: Box::new(virtual_device),
            },
        )
    }

    ///
    /// Add a `VirtualDevice` to make it discoverable and controllable.
    ///
    /// This version wraps the provided `VirtualDevice` and pretends that the `::turn_on()` and
    /// `::turn_off()` calls happen immediately.
    ///
    /// This form is useful when controlling a physical device that takes more than 5 seconds for
    /// its state to register as changed but is otherwise "guaranteed" to eventually happen.
    ///
    /// The implementation detail here is that calls to `::check_is_on()` will lie and return "ON"
    /// after a call to `::turn_on()` until your underlying implementation for `::check_is_on()`
    /// actually does return "ON".
    ///
    /// `@name`:  The word or phrase you'll use when talking to Alexa to control this device
    /// `@port`:  The port on which the backing HTTP server will listen for UPNP requests
    /// `@virtual_device`:  A `VirtualDevice` implementation
    ///
    pub fn add_instant_on_device<T: VirtualDevice>(
        &mut self,
        name: &str,
        port: u16,
        virtual_device: T,
    ) -> Result<WrappedVirtualDevice, RustmoError> {
        self.internal_add_device(
            name,
            IpAddr::V4(self.ip_address),
            port,
            InstantOnDevice {
                device: Box::new(virtual_device),
                instant: false,
            },
        )
    }

    ///
    /// Add an anonymous device to make it discoverable and controllable.
    ///
    /// This version allows for the anonymous implementation of a device.
    ///
    /// `@name`:  The word or phrase you'll use when talking to Alexa to control this device
    /// `@port`:  The port on which the backing HTTP server will listen for UPNP requests
    /// `@turn_on:` A closure that knows how to turn the device on
    /// `@turn_off:` A closure that knows how to turn the device off
    /// `@check_is_on:` A closure that knows how to determine if the device is on or off
    ///
    pub fn add_functional_device<TurnOn, TurnOff, CheckIsOn>(
        &mut self,
        name: &str,
        port: u16,
        turn_on: TurnOn,
        turn_off: TurnOff,
        check_is_on: CheckIsOn,
    ) -> Result<WrappedVirtualDevice, RustmoError>
    where
        TurnOn: FnMut() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
        TurnOff: FnMut() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
        CheckIsOn:
            FnMut() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
    {
        self.internal_add_device(
            name,
            IpAddr::V4(self.ip_address),
            port,
            FunctionalDevice {
                turn_on,
                turn_off,
                check_is_on,
            },
        )
    }

    ///
    /// Add a device that is a composite of multiple other devices.
    ///
    /// This is useful if you wish to create a "Living Room Lights" device ("Alexa, turn on
    /// Living Roomt Lights"), that necessitates controlling multiple other devices that you've
    /// already added because they can also be controlled independently.
    ///
    /// Note that communication with the list of devices in a device group happens in parallel, so
    /// make sure you don't have any kind of state dependencies between devices.
    ///
    /// An example of this might be turning a receiver on (one device) and changing its input to
    /// "DVD".  The receiver would need to be guaranteed "on" before its input source can be changed
    /// and this function does not guarantee that.
    ///
    /// `@name`:  The word or phrase you'll use when talking to Alexa to control this device
    /// `@port`:  The port on which the backing HTTP server will listen for UPNP requests
    /// `@devices`:  A vector of `WrappedVirtualDevice` instances that have previously been added
    /// to this `RustmoServer`
    ///
    pub fn add_device_group(
        &mut self,
        name: &str,
        port: u16,
        devices: Vec<WrappedVirtualDevice>,
    ) -> Result<WrappedVirtualDevice, RustmoError> {
        self.internal_add_device(
            name,
            IpAddr::V4(self.ip_address),
            port,
            CompositeDevice { devices },
        )
    }

    fn internal_add_device<T: VirtualDevice>(
        &mut self,
        name: &str,
        ip_address: IpAddr,
        port: u16,
        virtual_device: T,
    ) -> Result<WrappedVirtualDevice, RustmoError> {
        let mut device_list = self.devices.lock().unwrap();
        for existing_device in device_list.iter() {
            if existing_device.port == port {
                return Err(RustmoError::DeviceAlreadyExistsOnPort(port));
            } else if existing_device.name.to_lowercase().eq(&name.to_lowercase()) {
                return Err(RustmoError::DeviceAlreadyExistsByName(name.to_string()));
            }
        }

        let device = RustmoDevice::new(name, ip_address, port, virtual_device);
        let wrapped_device = device.virtual_device.clone();
        device_list.push(device);

        Ok(wrapped_device)
    }
}

impl Drop for RustmoServer {
    fn drop(&mut self) {
        self.ssdp_listener.stop()
    }
}
