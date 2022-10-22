use serde_json::Error;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct VirtualDeviceError(pub String);

impl VirtualDeviceError {
    pub fn new(message: &'static str) -> Self {
        VirtualDeviceError(message.to_string())
    }

    pub fn from(message: String) -> Self {
        VirtualDeviceError(message)
    }
}

impl std::convert::From<std::io::Error> for VirtualDeviceError {
    fn from(e: std::io::Error) -> Self {
        VirtualDeviceError::from(e.to_string())
    }
}

impl std::convert::From<std::ffi::FromBytesWithNulError> for VirtualDeviceError {
    fn from(e: std::ffi::FromBytesWithNulError) -> Self {
        VirtualDeviceError::from(e.to_string())
    }
}

impl std::convert::From<reqwest::Error> for VirtualDeviceError {
    fn from(e: reqwest::Error) -> Self {
        VirtualDeviceError::from(e.to_string())
    }
}

impl std::convert::From<serde_json::error::Error> for VirtualDeviceError {
    fn from(e: Error) -> Self {
        VirtualDeviceError::from(e.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VirtualDeviceState {
    /// the device is on
    On,

    /// the device is off
    Off,
}

///
/// `WrappedVirtualDevice` represents a `VirtualDevice` implementaiton
/// that is reference counted and guarded by a mutex, so that it can
/// be shared across threads
///
pub type WrappedVirtualDevice = Arc<Mutex<Box<dyn VirtualDevice>>>;

///
/// The `VirtualDevice` trait allows implementors to create devices that
/// can be exposed to Alexa via `RustmoServer`
///
/// Rustmo pretends that devices are a "plug", so they only have two states:
/// On and Off.
///
/// Some implementation notes:
///
///   1) Alexa will consider a device to be unresponsive if a request takes longer than 5 seconds.
///
///   2) When Alexa changes the state ("Alexa, turn $device ON/OFF") via `::turn_on()` or `::turn_off`,
/// it will then immediately check the state via `::check_is_on()`.  If that request doesn't match
/// what you just told Alexa to do, it will consider the device to be malfunctioning.
///
///   3) `RustmoServer` provides helper methods for wrapped devices so they can automatically poll
/// to make sure the desired state matches reality, or to just blindly pretend that the
/// state change worked.
///
///   4) It's best to implement `::turn_on()` and `::turn_off()` to execute as quickly as possible
/// and use one of the helper methods in `RustmoServer` to provide (slightly) more sophisticated
/// status verification.
///
pub trait VirtualDevice: Sync + Send + 'static {
    /// turn the device on
    fn turn_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError>;

    /// turn the device off
    fn turn_off(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError>;

    /// is the device on?
    fn check_is_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError>;
}

pub(crate) mod wrappers {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::Duration;

    use rayon::prelude::*;

    use crate::virtual_device::{
        VirtualDevice, VirtualDeviceError, VirtualDeviceState, WrappedVirtualDevice,
    };

    ///
    /// Wrapper for `VirtualDevice` that pretends the device is instantly turned on when
    /// Alexa calls `::turn_on()`.
    ///
    pub(crate) struct InstantOnDevice {
        pub(crate) device: Box<dyn VirtualDevice>,
        pub(crate) instant: bool,
    }

    impl VirtualDevice for InstantOnDevice {
        fn turn_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            let result = self.device.turn_on();
            self.instant = true;

            result
        }

        fn turn_off(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            let result = self.device.turn_off();
            self.instant = false;

            result
        }

        fn check_is_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            let result = self.device.check_is_on();

            if self.instant {
                if let VirtualDeviceState::On = result.unwrap_or(VirtualDeviceState::Off) {
                    self.instant = false;
                }
                return Ok(VirtualDeviceState::On);
            }

            result
        }
    }

    ///
    /// Wrapper for `VirtualDevice` that polls the device for its status, up to ~4 seconds, to
    /// ensure the state has changed to what Alexa requested
    ///
    pub(crate) struct PollingDevice {
        pub(crate) device: Box<dyn VirtualDevice>,
    }

    impl VirtualDevice for PollingDevice {
        fn turn_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            self.device.turn_on()?;

            let mut state = self.device.check_is_on().unwrap_or(VirtualDeviceState::Off);
            match state {
                VirtualDeviceState::Off => {
                    let mut cnt = 0;
                    while state.eq(&VirtualDeviceState::Off) {
                        println!("POLLING for 'on': cnt={}", cnt);

                        thread::sleep(Duration::from_millis(400));
                        state = self.device.check_is_on().unwrap_or(VirtualDeviceState::Off);
                        cnt += 1;
                        if cnt == 10 {
                            break;
                        }
                    }
                    Ok(state)
                }
                _ => Ok(state),
            }
        }

        fn turn_off(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            self.device.turn_off()?;

            let mut state = self.device.check_is_on().unwrap_or(VirtualDeviceState::On);
            match state {
                VirtualDeviceState::On => {
                    let mut cnt = 0;
                    while state.eq(&VirtualDeviceState::On) {
                        println!("POLLING for 'off': cnt={}", cnt);
                        thread::sleep(Duration::from_millis(400));

                        state = self.device.check_is_on().unwrap_or(VirtualDeviceState::On);
                        cnt += 1;
                        if cnt == 10 {
                            break;
                        }
                    }
                    Ok(state)
                }
                _ => Ok(state),
            }
        }

        fn check_is_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            self.device.check_is_on()
        }
    }

    ///
    /// Wrapper for `VirtualDevice` that allows a list of devices to work together as a single
    /// device.
    ///
    /// All state changes and inqueries to the underlying devices happen in parallel
    ///
    pub(crate) struct CompositeDevice {
        pub(crate) devices: Vec<WrappedVirtualDevice>,
    }

    impl VirtualDevice for CompositeDevice {
        fn turn_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            self.devices.par_iter_mut().for_each(|device| {
                if let Ok(mut device) = device.lock() {
                    if device.check_is_on().unwrap_or(VirtualDeviceState::Off)
                        == VirtualDeviceState::Off
                    {
                        device.turn_on().ok().unwrap();
                    }
                }
            });

            self.check_is_on()
        }

        fn turn_off(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            self.devices.par_iter_mut().for_each(|device| {
                if let Ok(mut device) = device.lock() {
                    if device.check_is_on().unwrap_or(VirtualDeviceState::Off)
                        == VirtualDeviceState::On
                    {
                        device.turn_off().ok().unwrap();
                    }
                }
            });

            self.check_is_on()
        }

        fn check_is_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            let on = AtomicBool::new(true);
            self.devices.par_iter_mut().for_each(|device| {
                if let Ok(mut device) = device.lock() {
                    match device.check_is_on().unwrap_or(VirtualDeviceState::Off) {
                        VirtualDeviceState::On => {
                            on.compare_exchange(true, true, Ordering::SeqCst, Ordering::SeqCst)
                                .ok();
                        }
                        VirtualDeviceState::Off => {
                            on.store(false, Ordering::SeqCst);
                        }
                    }
                }
            });

            if on.load(Ordering::SeqCst) {
                Ok(VirtualDeviceState::On)
            } else {
                Ok(VirtualDeviceState::Off)
            }
        }
    }

    ///
    /// Wrapper for `VirtualDevice` that allows a device to be implemented using closures
    pub(crate) struct FunctionalDevice<TurnOn, TurnOff, CheckIsOn>
    where
        TurnOn: FnMut() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
        TurnOff: FnMut() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
        CheckIsOn:
            FnMut() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
    {
        pub(crate) turn_on: TurnOn,
        pub(crate) turn_off: TurnOff,
        pub(crate) check_is_on: CheckIsOn,
    }

    impl<TurnOn, TurnOff, CheckIsOn> VirtualDevice for FunctionalDevice<TurnOn, TurnOff, CheckIsOn>
    where
        TurnOn: FnMut() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
        TurnOff: FnMut() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
        CheckIsOn:
            FnMut() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
    {
        fn turn_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            (self.turn_on)()
        }

        fn turn_off(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            (self.turn_off)()
        }

        fn check_is_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            (self.check_is_on)()
        }
    }
}
