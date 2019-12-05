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

    pub fn enter(&self) -> Result<(), VirtualDeviceError> {
        self.send_command("#SEL")?;
        Ok(())
    }

    pub fn up(&self) -> Result<(), VirtualDeviceError> {
        self.send_command("#NUP")?;
        Ok(())
    }

    pub fn down(&self) -> Result<(), VirtualDeviceError> {
        self.send_command("#NDN")?;
        Ok(())
    }

    pub fn left(&self) -> Result<(), VirtualDeviceError> {
        self.send_command("#NLT")?;
        Ok(())
    }

    pub fn right(&self) -> Result<(), VirtualDeviceError> {
        self.send_command("#NRT")?;
        Ok(())
    }

    pub fn home(&self) -> Result<(), VirtualDeviceError> {
        self.send_command("#HOM")?;
        Ok(())
    }

    pub fn osd(&self) -> Result<(), VirtualDeviceError> {
        self.send_command("#OSD")?;
        Ok(())
    }

    pub fn play(&self) -> Result<(), VirtualDeviceError> {
        self.send_command("#PLA")?;
        Ok(())
    }

    pub fn pause(&self) -> Result<(), VirtualDeviceError> {
        self.send_command("#PAU")?;
        Ok(())
    }

    pub fn stop(&self) -> Result<(), VirtualDeviceError> {
        self.send_command("#STP")?;
        Ok(())
    }

    pub fn rewind(&self) -> Result<(), VirtualDeviceError> {
        self.send_command("#REV")?;
        Ok(())
    }

    pub fn fast_forward(&self) -> Result<(), VirtualDeviceError> {
        self.send_command("#FWD")?;
        Ok(())
    }

    fn send_command(&self, command: &'static str) -> Result<TcpStream, VirtualDeviceError> {
        let mut stream =
            TcpStream::connect_timeout(&SocketAddr::new(self.ip_address, 23), TIMEOUT)?;
        stream.write_all(format!("{}\r\n", command).as_ref())?;
        Ok(stream)
    }
}

impl VirtualDevice for Device {
    fn turn_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#PON")?;
        Ok(VirtualDeviceState::On)
    }

    fn turn_off(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.send_command("#POF")?;
        thread::sleep(Duration::from_secs(2));
        Ok(VirtualDeviceState::Off)
    }

    fn check_is_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let mut stream = self.send_command("#QPW")?;
        let res = &mut [0 as u8; 32];
        let len = stream.read(res)?;
        let str = CStr::from_bytes_with_nul(&res[..len + 1])?.to_string_lossy();

        Ok(match str.to_string().as_str() {
            "@OK ON\r" => VirtualDeviceState::On,
            _ => VirtualDeviceState::Off,
        })
    }
}
