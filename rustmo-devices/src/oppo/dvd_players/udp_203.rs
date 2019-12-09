use std::ffi::CStr;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::thread;
use std::time::Duration;

use rustmo_server::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};

const TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Clone, Copy)]
pub struct Device {
    ip_address: IpAddr,
}

/// http://download.oppodigital.com/UDP203/OPPO_UDP-20X_RS-232_and_IP_Control_Protocol.pdf
/// https://www.oppodigital.com/blu-ray-udp-203/
impl Device {
    pub fn new(ip_address: IpAddr) -> Self {
        Device { ip_address }
    }

    pub fn enter(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#SEL")
    }

    pub fn up(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#NUP")
    }

    pub fn down(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#NDN")
    }

    pub fn left(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#NLT")
    }

    pub fn right(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#NRT")
    }

    pub fn home(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#HOM")
    }

    pub fn osd(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#OSD")
    }

    pub fn play(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#PLA")
    }

    pub fn pause(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#PAU")
    }

    pub fn stop(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#STP")
    }

    pub fn rewind(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#REV")
    }

    pub fn fast_forward(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#FWD")
    }

    fn send_command(&self, command: &'static str) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let mut stream =
            TcpStream::connect_timeout(&SocketAddr::new(self.ip_address, 23), TIMEOUT)?;
        stream.write_all(format!("{}\r\n", command).as_ref())?;

        let res = &mut [0 as u8; 32];
        let len = stream.read(res)?;
        let str = CStr::from_bytes_with_nul(&res[..=len])?.to_string_lossy();

        if str.to_string().starts_with("@OK ") {
            Ok(VirtualDeviceState::On)
        } else {
            Err(VirtualDeviceError(str.to_string()))
        }
    }
}

impl VirtualDevice for Device {
    fn turn_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#PON")?;
        thread::sleep(Duration::from_secs(2));
        Ok(VirtualDeviceState::On)
    }

    fn turn_off(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#POF")?;
        thread::sleep(Duration::from_secs(2));
        Ok(VirtualDeviceState::Off)
    }

    fn check_is_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let mut stream =
            TcpStream::connect_timeout(&SocketAddr::new(self.ip_address, 23), TIMEOUT)?;
        stream.write_all("#QPW\r\n".as_ref())?;
        let res = &mut [0 as u8; 32];
        let len = stream.read(res)?;
        let str = CStr::from_bytes_with_nul(&res[..=len])?.to_string_lossy();

        Ok(match str.to_string().as_str() {
            "@OK ON\r" => VirtualDeviceState::On,
            _ => VirtualDeviceState::Off,
        })
    }
}
