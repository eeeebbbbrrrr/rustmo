#[macro_use]
extern crate serde_derive;

use std::fmt::{Debug, Display, Formatter};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::thread;

use parking_lot::RwLock;
use tracing::warn;
use uuid::Uuid;

use crate::ssdp::SsdpListener;
use crate::upnp::*;
use crate::virtual_device::wrappers::*;
use crate::virtual_device::*;

mod ssdp;
mod upnp;
pub mod virtual_device;

#[derive(Clone)]
pub struct RustmoDeviceInfo {
    pub(crate) name: String,
    pub(crate) ip_address: IpAddr,
    pub(crate) port: u16,
    pub(crate) uuid: Uuid,
}

pub struct RustmoDevice {
    pub(crate) info: RustmoDeviceInfo,
    pub(crate) device: Box<dyn VirtualDevice>,
}

// unsafe impl Send for RustmoDevice {}
// unsafe impl Sync for RustmoDevice {}

impl RustmoDevice {
    pub fn new<T: VirtualDevice, S: Into<String>>(
        name: S,
        ip_address: IpAddr,
        port: u16,
        virtual_device: &SynchronizedDevice<T>,
    ) -> Self {
        let name = name.into();
        let mut bytes = Vec::from(name.as_bytes());
        while bytes.len() < 16 {
            bytes.push(bytes.len() as u8);
        }
        while bytes.len() > 16 {
            bytes.pop();
        }

        let device_info = RustmoDeviceInfo {
            name: name.to_string(),
            ip_address,
            port,
            uuid: Uuid::from_slice(bytes.as_slice()).expect("failed to generate UUID"),
        };

        let device: Box<dyn VirtualDevice> = Box::new(virtual_device.clone());
        let info = device_info.clone();
        thread::spawn(move || {
            let device = RustmoDevice { info, device };

            let server = match hyper::Server::http(SocketAddr::new(ip_address, port)) {
                Ok(server) => server,
                Err(e) => panic!(
                    "ERROR STARTING DEVICE SERVER:  ip={}, port={}, e={}",
                    ip_address, port, e
                ),
            };
            server.handle(DeviceHttpServerHandler::new(device)).unwrap();
        });

        let device: Box<dyn VirtualDevice> = Box::new(virtual_device.clone());
        RustmoDevice {
            info: device_info,
            device,
        }
    }

    fn turn_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.device.turn_on()
    }

    fn turn_off(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.device.turn_off()
    }

    fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.device.check_is_on()
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
    devices: VirtualDevicesList,
    next_port: u16,
    ip_address: IpAddr,
    ssdp_listener: SsdpListener,
}

pub(crate) type VirtualDevicesList = Arc<RwLock<Vec<RustmoDevice>>>;

#[derive(Debug)]
pub enum RustmoError {
    DeviceAlreadyExistsByName(String),
}

impl Display for RustmoError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!("{:?}", self))
    }
}

impl std::error::Error for RustmoError {}

impl RustmoServer {
    ///
    /// Create a new `RustmoServer` and listen for SSDP requests on the specified network interface
    ///
    pub fn new(interface: IpAddr, starting_port: u16) -> Self {
        let devices: VirtualDevicesList = Arc::new(RwLock::new(Vec::new()));
        RustmoServer {
            devices: devices.clone(),
            ip_address: interface,
            next_port: starting_port,
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
    pub fn add_device<T: VirtualDevice, S: Into<String>>(
        &mut self,
        name: S,
        virtual_device: T,
    ) -> Result<SynchronizedDevice<T>, RustmoError> {
        self.internal_add_device(name, self.ip_address, virtual_device)
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
    pub fn add_polling_device<T: VirtualDevice, S: Into<String>>(
        &mut self,
        name: S,
        virtual_device: T,
    ) -> Result<SynchronizedDevice<PollingDevice<T>>, RustmoError> {
        let virtual_device = PollingDevice {
            device: virtual_device,
        };
        self.internal_add_device(name, self.ip_address, virtual_device)
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
    pub fn add_instant_on_device<T: VirtualDevice, S: Into<String>>(
        &mut self,
        name: S,
        virtual_device: T,
    ) -> Result<SynchronizedDevice<InstantOnDevice<T>>, RustmoError> {
        let virtual_device = InstantOnDevice {
            device: virtual_device,
            believed_on: Default::default(),
        };
        self.internal_add_device(name, self.ip_address, virtual_device)
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
        turn_on: TurnOn,
        turn_off: TurnOff,
        check_is_on: CheckIsOn,
    ) -> Result<SynchronizedDevice<FunctionalDevice<TurnOn, TurnOff, CheckIsOn>>, RustmoError>
    where
        TurnOn: Fn() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
        TurnOff: Fn() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
        CheckIsOn: Fn() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
    {
        let virtual_device = FunctionalDevice {
            turn_on,
            turn_off,
            check_is_on,
        };
        self.internal_add_device(name, self.ip_address, virtual_device)
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
    /// `@devices`:  A vector of `SynchronizedDevice` instances that have previously been added
    /// to this `RustmoServer`
    ///
    pub fn add_device_group(
        &mut self,
        name: &str,
        devices: Vec<Box<dyn VirtualDevice>>,
    ) -> Result<SynchronizedDevice<CompositeDevice>, RustmoError> {
        let virtual_device = CompositeDevice { devices };
        self.internal_add_device(name, self.ip_address, virtual_device)
    }

    fn internal_add_device<T: VirtualDevice, S: Into<String>>(
        &mut self,
        name: S,
        ip_address: IpAddr,
        virtual_device: T,
    ) -> Result<SynchronizedDevice<T>, RustmoError> {
        let name = name.into();
        warn!("registered Rustmo Device `{}` of type `{}`", name, std::any::type_name::<T>());
        let mut device_list = self.devices.write();
        for existing_device in device_list.iter() {
            if existing_device
                .info
                .name
                .to_lowercase()
                .eq(&name.to_lowercase())
            {
                return Err(RustmoError::DeviceAlreadyExistsByName(name.to_string()));
            }
        }

        let synced = SynchronizedDevice::new(virtual_device);
        let device = RustmoDevice::new(name, ip_address, self.next_port, &synced);
        self.next_port += 1;

        device_list.push(device);

        Ok(synced)
    }
}

impl Drop for RustmoServer {
    fn drop(&mut self) {
        self.ssdp_listener.stop()
    }
}
