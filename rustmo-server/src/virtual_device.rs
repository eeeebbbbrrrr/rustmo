use std::fmt::{Debug, Display, Formatter};
use std::net::AddrParseError;
use std::num::{ParseFloatError, ParseIntError};
use std::ops::Deref;
use std::str::Utf8Error;
use std::sync::Arc;

use parking_lot::{Mutex, MutexGuard};
use postgres::Error;

use crate::RustmoError;

#[derive(Debug, Eq, PartialEq)]
pub struct VirtualDeviceError(pub String);

impl VirtualDeviceError {
    pub fn new(message: &'static str) -> Self {
        VirtualDeviceError(message.to_string())
    }

    pub fn from<S: Into<String>>(message: S) -> Self {
        VirtualDeviceError(message.into())
    }
}

impl Display for VirtualDeviceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<RustmoError> for VirtualDeviceError {
    fn from(e: RustmoError) -> Self {
        VirtualDeviceError::from(e.to_string())
    }
}

impl From<std::io::Error> for VirtualDeviceError {
    fn from(e: std::io::Error) -> Self {
        VirtualDeviceError::from(e.to_string())
    }
}

impl From<std::ffi::FromBytesWithNulError> for VirtualDeviceError {
    fn from(e: std::ffi::FromBytesWithNulError) -> Self {
        VirtualDeviceError::from(e.to_string())
    }
}

impl From<ureq::Error> for VirtualDeviceError {
    fn from(e: ureq::Error) -> Self {
        VirtualDeviceError::from(e.to_string())
    }
}

impl From<serde_json::Error> for VirtualDeviceError {
    fn from(e: serde_json::Error) -> Self {
        VirtualDeviceError::from(e.to_string())
    }
}

impl From<serde_xml_rs::Error> for VirtualDeviceError {
    fn from(e: serde_xml_rs::Error) -> Self {
        VirtualDeviceError::from(e.to_string())
    }
}

impl From<Utf8Error> for VirtualDeviceError {
    fn from(e: Utf8Error) -> Self {
        VirtualDeviceError::from(e.to_string())
    }
}

impl From<ParseFloatError> for VirtualDeviceError {
    fn from(e: ParseFloatError) -> Self {
        VirtualDeviceError::from(e.to_string())
    }
}

impl From<ParseIntError> for VirtualDeviceError {
    fn from(e: ParseIntError) -> Self {
        VirtualDeviceError::from(e.to_string())
    }
}

impl From<AddrParseError> for VirtualDeviceError {
    fn from(e: AddrParseError) -> Self {
        VirtualDeviceError::from(e.to_string())
    }
}

impl From<postgres::Error> for VirtualDeviceError {
    fn from(e: Error) -> Self {
        VirtualDeviceError::from(e.to_string())
    }
}

impl std::error::Error for VirtualDeviceError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum VirtualDeviceState {
    /// the device is on
    On,

    /// the device is off
    Off,
}

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
    fn turn_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError>;

    /// turn the device off
    fn turn_off(&self) -> Result<VirtualDeviceState, VirtualDeviceError>;

    /// is the device on?
    fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError>;
}

pub(crate) mod wrappers {
    use std::ops::{Deref, DerefMut};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::Duration;

    use crate::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};

    ///
    /// Wrapper for `VirtualDevice` that pretends the device is instantly turned on when
    /// Alexa calls `::turn_on()`.
    ///
    pub struct InstantOnDevice<T> {
        pub(crate) device: T,
        pub(crate) believed_on: AtomicBool,
    }

    impl<T> InstantOnDevice<T> {
        pub fn new(device: T) -> Self {
            Self {
                device,
                believed_on: AtomicBool::new(false),
            }
        }
    }

    impl<T> Deref for InstantOnDevice<T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &self.device
        }
    }

    impl<T> DerefMut for InstantOnDevice<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.device
        }
    }

    impl<T: VirtualDevice> VirtualDevice for InstantOnDevice<T> {
        fn turn_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            let result = self.device.turn_on();
            self.believed_on.store(true, Ordering::SeqCst);

            result
        }

        fn turn_off(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            let result = self.device.turn_off();
            self.believed_on.store(false, Ordering::SeqCst);

            result
        }

        fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            if self.believed_on.load(Ordering::SeqCst) {
                return Ok(VirtualDeviceState::On);
            }

            let result = self.device.check_is_on();
            result
        }
    }

    ///
    /// Wrapper for `VirtualDevice` that polls the device for its status, up to ~4 seconds, to
    /// ensure the state has changed to what Alexa requested
    ///
    pub struct PollingDevice<T> {
        pub(crate) device: T,
    }

    impl<T> Deref for PollingDevice<T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &self.device
        }
    }

    impl<T> DerefMut for PollingDevice<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.device
        }
    }

    impl<T: VirtualDevice> VirtualDevice for PollingDevice<T> {
        fn turn_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            self.device.turn_on()?;

            let mut state = self.device.check_is_on().unwrap_or(VirtualDeviceState::Off);
            match state {
                VirtualDeviceState::Off => {
                    let mut cnt = 0;
                    while state.eq(&VirtualDeviceState::Off) {
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

        fn turn_off(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            self.device.turn_off()?;

            let mut state = self.device.check_is_on().unwrap_or(VirtualDeviceState::On);
            match state {
                VirtualDeviceState::On => {
                    let mut cnt = 0;
                    while state.eq(&VirtualDeviceState::On) {
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

        fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            self.device.check_is_on()
        }
    }

    ///
    /// Wrapper for `VirtualDevice` that allows a list of devices to work together as a single
    /// device.
    ///
    /// All state changes and inqueries to the underlying devices happen in parallel
    ///
    pub struct CompositeDevice {
        pub(crate) devices: Vec<Box<dyn VirtualDevice>>,
    }

    impl VirtualDevice for CompositeDevice {
        fn turn_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            self.devices.iter().for_each(|device| {
                if device.check_is_on().unwrap_or(VirtualDeviceState::Off)
                    == VirtualDeviceState::Off
                {
                    device.turn_on().ok().unwrap();
                }
            });

            self.check_is_on()
        }

        fn turn_off(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            self.devices.iter().for_each(|device| {
                if device.check_is_on().unwrap_or(VirtualDeviceState::Off) == VirtualDeviceState::On
                {
                    device.turn_off().ok().unwrap();
                }
            });

            self.check_is_on()
        }

        fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            let on = AtomicBool::new(true);
            self.devices.iter().for_each(|device| {
                match device.check_is_on().unwrap_or(VirtualDeviceState::Off) {
                    VirtualDeviceState::On => {
                        on.compare_exchange(true, true, Ordering::SeqCst, Ordering::SeqCst)
                            .ok();
                    }
                    VirtualDeviceState::Off => {
                        on.store(false, Ordering::SeqCst);
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
    pub struct FunctionalDevice<TurnOn, TurnOff, CheckIsOn>
    where
        TurnOn: Fn() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
        TurnOff: Fn() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
        CheckIsOn: Fn() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
    {
        pub(crate) turn_on: TurnOn,
        pub(crate) turn_off: TurnOff,
        pub(crate) check_is_on: CheckIsOn,
    }

    impl<TurnOn, TurnOff, CheckIsOn> VirtualDevice for FunctionalDevice<TurnOn, TurnOff, CheckIsOn>
    where
        TurnOn: Fn() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
        TurnOff: Fn() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
        CheckIsOn: Fn() -> Result<VirtualDeviceState, VirtualDeviceError> + Sync + Send + 'static,
    {
        fn turn_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            (self.turn_on)()
        }

        fn turn_off(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            (self.turn_off)()
        }

        fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            (self.check_is_on)()
        }
    }
}

///
/// [`SynchronizedDevice`] represents a `VirtualDevice` implementation
/// that is reference counted and guarded by a mutex, so that it can
/// be shared across threads
///
pub struct SynchronizedDevice<T: ?Sized> {
    device: Arc<Mutex<T>>,
}

impl<T> Clone for SynchronizedDevice<T> {
    #[inline]
    fn clone(&self) -> Self {
        SynchronizedDevice {
            device: Arc::clone(&self.device),
        }
    }
}

impl<T> SynchronizedDevice<T>
where
    T: VirtualDevice,
{
    #[inline]
    pub fn new(device: T) -> Self {
        SynchronizedDevice {
            device: Arc::new(Mutex::new(device)),
        }
    }

    #[inline]
    pub fn lock(&self) -> MutexGuard<T> {
        self.device.lock()
    }
}

impl<T> VirtualDevice for SynchronizedDevice<T>
where
    T: VirtualDevice + Send + Sync + 'static,
{
    fn turn_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.lock().turn_on()
    }

    fn turn_off(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.lock().turn_off()
    }

    fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.lock().check_is_on()
    }
}

impl VirtualDevice for Box<dyn VirtualDevice> {
    fn turn_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.deref().turn_on()
    }

    fn turn_off(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.deref().turn_off()
    }

    fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.deref().check_is_on()
    }
}
