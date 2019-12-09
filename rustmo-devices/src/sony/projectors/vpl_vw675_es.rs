use std::io::{Cursor, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};

use byteorder::{BigEndian, ReadBytesExt};

use rustmo_server::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);

#[derive(Clone, Copy)]
pub struct Device {
    ip_address: IpAddr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PowerStatus {
    // 0x0000
    Standby,

    // 0x0001,  0x0002
    Warming,

    // 0x0003
    PowerOn,

    // 0x0004, 0x0005
    Cooling,
}

/// http://www.sonypremiumhome.com/projectors/VPL-VW675ES.php
impl Device {
    pub fn new(ip_address: IpAddr) -> Self {
        Device { ip_address }
    }

    pub fn get_power_status(&self) -> Result<PowerStatus, VirtualDeviceError> {
        let mut stream =
            TcpStream::connect_timeout(&SocketAddr::new(self.ip_address, 53484), TIMEOUT)?;
        stream.write_all(
            Device::make_command_bytes(0x01, 0x01, 0x02, Vec::new().as_slice()).as_slice(),
        )?;

        let _version = stream.read_u8()?;
        let _category = stream.read_u8()?;
        let _community: i32 = stream.read_i32::<BigEndian>()?;
        let success = stream.read_u8()?;
        let _command = stream.read_i16::<BigEndian>()?;
        let expected_len = stream.read_u8()? as usize;
        let data = &mut [0 as u8; 32];
        let len = stream.read(data)?;
        let data = data.to_vec();

        if success == 1 && expected_len == len && len == 2 {
            let mut cursor = Cursor::new(data);
            let status = cursor.read_i16::<BigEndian>()?;

            match status {
                0x0000 => Ok(PowerStatus::Standby),
                0x0001 | 0x0002 => Ok(PowerStatus::Warming),
                0x0003 => Ok(PowerStatus::PowerOn),
                0x0004 | 0x0005 => Ok(PowerStatus::Cooling),
                _ => Err(VirtualDeviceError::from(format!(
                    "Invalid status code({:X}) received from  Vw675Es",
                    status
                ))),
            }
        } else {
            Err(VirtualDeviceError::new(
                "Coudln't determine power status for Vw675Es",
            ))
        }
    }

    fn make_command_bytes(action: u8, command_hi: u8, command_lo: u8, data: &[u8]) -> Vec<u8> {
        let mut bytes = vec![
            0x02 as u8, // version
            0x0a,       // category
            b'S',
            b'O',
            b'N',
            b'Y', // community
            action,
            command_hi,
            command_lo,
            data.len() as u8,
        ];
        bytes.extend_from_slice(data);

        bytes
    }
}

impl VirtualDevice for Device {
    fn turn_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let mut stream =
            TcpStream::connect_timeout(&SocketAddr::new(self.ip_address, 53484), TIMEOUT)?;
        stream.write_all(
            Device::make_command_bytes(0x00, 0x01, 0x30, vec![0x00, 0x01].as_slice()).as_slice(),
        )?;

        Ok(VirtualDeviceState::On)
    }

    fn turn_off(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let mut stream =
            TcpStream::connect_timeout(&SocketAddr::new(self.ip_address, 53484), TIMEOUT)?;
        stream.write_all(
            Device::make_command_bytes(0x00, 0x01, 0x30, vec![0x00, 0x00].as_slice()).as_slice(),
        )?;

        Ok(VirtualDeviceState::Off)
    }

    fn check_is_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        Ok(match self.get_power_status()? {
            PowerStatus::Standby => VirtualDeviceState::Off,
            PowerStatus::Warming => VirtualDeviceState::On,
            PowerStatus::PowerOn => VirtualDeviceState::On,
            PowerStatus::Cooling => VirtualDeviceState::Off,
        })
    }
}
