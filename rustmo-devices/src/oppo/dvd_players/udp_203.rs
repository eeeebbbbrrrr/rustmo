use std::ffi::CStr;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::thread;
use std::time::Duration;

use rustmo_server::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};

const TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Clone, Debug)]
pub struct Device {
    ip: IpAddr,
}

/// http://download.oppodigital.com/UDP203/OPPO_UDP-20X_RS-232_and_IP_Control_Protocol.pdf
/// https://www.oppodigital.com/blu-ray-udp-203/
impl Device {
    pub fn new(ip: IpAddr) -> Self {
        Device { ip }
    }

    pub fn enter(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#SEL")
    }

    pub fn up(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#NUP")
    }

    pub fn down(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#NDN")
    }

    pub fn left(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#NLT")
    }

    pub fn right(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#NRT")
    }

    pub fn home(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#HOM")
    }

    pub fn osd(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#OSD")
    }

    pub fn play(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#PLA")
    }

    pub fn pause(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#PAU")
    }

    pub fn stop(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#STP")
    }

    pub fn rewind(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#REV")
    }

    pub fn fast_forward(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#FWD")
    }

    fn send_command(
        &self,
        command: &'static str,
    ) -> Result<VirtualDeviceState, VirtualDeviceError> {
        tracing::info!("udp_203 command: {}", command);
        let mut stream = TcpStream::connect_timeout(&SocketAddr::new(self.ip, 23), TIMEOUT)?;
        stream.set_read_timeout(Some(Duration::from_millis(1000)))?;
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
    fn turn_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#PON")?;
        thread::sleep(Duration::from_secs(2));
        Ok(VirtualDeviceState::On)
    }

    fn turn_off(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#POF")?;
        thread::sleep(Duration::from_secs(2));
        Ok(VirtualDeviceState::Off)
    }

    fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let mut stream = TcpStream::connect_timeout(&SocketAddr::new(self.ip, 23), TIMEOUT)?;
        stream.set_read_timeout(Some(Duration::from_millis(1000)))?;
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
