use std::io::{Cursor, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;

use byteorder::{BigEndian, ReadBytesExt};

use rustmo_server::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};

pub struct Device {
    ip: IpAddr,
}

#[derive(Debug, Copy, Clone, PartialEq)]
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

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum BlankingPosition {
    Left = 0xB0,
    Right = 0xB1,
    Top = 0xB2,
    Bottom = 0xB3,
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum PicturePosition {
    Aspect185_1 = 0x00,
    Aspect235_1 = 0x01,
    Custom1 = 0x02,
    Custom2 = 0x03,
    Custom3 = 0x04,
}

// https://digis.ru/upload/iblock/53c/VPL-VW320,%20VW520_ProtocolManual.pdf
// http://www.sonypremiumhome.com/projectors/VPL-VW675ES.php
impl Device {
    pub fn new(ip: IpAddr) -> Self {
        Device { ip: ip }
    }

    fn open(&self) -> Result<TcpStream, VirtualDeviceError> {
        let stream = TcpStream::connect_timeout(
            &SocketAddr::new(self.ip, 53484),
            Duration::from_millis(30000),
        )?;
        stream.set_read_timeout(Some(Duration::from_millis(1000)))?;
        Ok(stream)
    }

    pub fn get(&self, hi: u8, lo: u8) -> Result<Vec<u8>, VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x01, hi, lo, &[]))?;
        stream.flush()?;
        match Device::read_response(&mut stream) {
            Ok((_len, data)) => Ok(data.into_inner()),
            Err(e) => Err(e),
        }
    }

    pub fn set(&mut self, hi: u8, lo: u8, data: &[u8]) -> Result<Vec<u8>, VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, hi, lo, data))?;
        stream.flush()?;
        match Device::read_response(&mut stream) {
            Ok((_len, data)) => Ok(data.into_inner()),
            Err(e) => Err(e),
        }
    }

    pub fn cursor_up(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x17, 0x35, &[0x00, 0x00]))?;
        stream.flush()?;
        Ok(())
    }

    pub fn lens_control(&mut self, on: bool) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(
            0x00,
            0xAE,
            0x62,
            &[0x00, on as u8],
        ))?;
        Ok(stream.flush()?)
    }

    pub fn lens_zoom(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x19, 0x62, &[0x00, 0x00]))?;
        Ok(stream.flush()?)
    }

    pub fn lens_focus(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x19, 0x64, &[0x00, 0x00]))?;
        Ok(stream.flush()?)
    }

    pub fn lens_shift(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x19, 0x63, &[0x00, 0x00]))?;
        Ok(stream.flush()?)
    }

    #[track_caller]
    pub fn lens_shift_up(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x17, 0x72, &[0x00, 00]))?;
        Ok(stream.flush()?)
    }

    pub fn lens_shift_down(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x17, 0x73, &[0x00, 00]))?;
        Ok(stream.flush()?)
    }

    pub fn lens_shift_left(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x19, 0x02, &[0x00, 0x00]))?;
        Ok(stream.flush()?)
    }

    pub fn lens_shift_right(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x19, 0x03, &[0x00, 0x00]))?;
        Ok(stream.flush()?)
    }

    pub fn lens_focus_far(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x17, 0x74, &[0x00, 0x00]))?;
        Ok(stream.flush()?)
    }

    pub fn lens_focus_near(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x17, 0x75, &[0x00, 0x00]))?;
        Ok(stream.flush()?)
    }

    pub fn lens_zoom_large(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x17, 0x77, &[0x00, 0x00]))?;
        stream.flush()?;
        Ok(())
    }

    pub fn lens_zoom_small(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x17, 0x78, &[0x00, 0x00]))?;
        Ok(stream.flush()?)
    }

    pub fn zoom_menu(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x17, 0x62, &[0x00, 0x00]))?;
        Ok(stream.flush()?)
    }

    pub fn reset(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x17, 0x7B, &[0x00, 0x00]))?;
        stream.flush()?;
        Ok(())
    }

    pub fn enter(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x17, 0x5a, &[0x00, 0x00]))?;
        Ok(stream.flush()?)
    }

    pub fn picture_position(&self) -> Result<PicturePosition, VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x01, 0x00, 0x66, &[]))?;
        stream.flush()?;
        let (_len, mut data) = Device::read_response(&mut stream)?;
        let code = data.read_u16::<BigEndian>()?;
        tracing::debug!("{:#04X?}", data.into_inner());
        Ok(match code {
            0x0000 => PicturePosition::Aspect185_1,
            0x0001 => PicturePosition::Aspect235_1,
            0x0002 => PicturePosition::Custom1,
            0x0003 => PicturePosition::Custom2,
            0x0004 => PicturePosition::Custom3,
            _ => {
                return Err(VirtualDeviceError::from(format!(
                    "unexpected PicturePosition value: {}",
                    code
                )))
            }
        })
    }

    pub fn picture_position_185_1(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x00, 0x66, &[0x00, 0x00]))?;
        Ok(stream.flush()?)
    }

    pub fn picture_position_235_1(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x00, 0x66, &[0x00, 0x01]))?;
        Ok(stream.flush()?)
    }

    pub fn picture_position_custom_1(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x00, 0x66, &[0x00, 0x02]))?;
        Ok(stream.flush()?)
    }

    pub fn picture_position_custom_2(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x00, 0x66, &[0x00, 0x03]))?;
        Ok(stream.flush()?)
    }

    pub fn picture_position_custom_3(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x00, 0x66, &[0x00, 0x04]))?;
        Ok(stream.flush()?)
    }

    pub fn aspect_normal(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x00, 0x20, &[0x00, 0x01]))?;
        Ok(stream.flush()?)
    }

    pub fn aspect_vstretch(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x00, 0x20, &[0x00, 0x0B]))?;
        Ok(stream.flush()?)
    }

    pub fn aspect_1851_zoom(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x00, 0x20, &[0x00, 0x0C]))?;
        Ok(stream.flush()?)
    }

    pub fn aspect_2351_zoom(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x00, 0x20, &[0x00, 0x0D]))?;
        Ok(stream.flush()?)
    }

    pub fn aspect_stretch(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x00, 0x20, &[0x00, 0x0E]))?;
        Ok(stream.flush()?)
    }

    pub fn aspect_squeeze(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x00, 0x20, &[0x00, 0x0F]))?;
        Ok(stream.flush()?)
    }

    pub fn lens_toggle(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x1B, 0x78, &[0x00, 0x00]))?;
        Ok(stream.flush()?)
    }

    pub fn test_pattern_off(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x00, 0xAB, &[0x00, 0x00]))?;
        Ok(stream.flush()?)
    }

    pub fn test_pattern_on(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x00, 0xAB, &[0x00, 0x01]))?;
        Ok(stream.flush()?)
    }

    pub fn settings_reset(&mut self) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x01, 0x6A, &[0x00, 0x00]))?;
        Ok(stream.flush()?)
    }

    pub fn blanking(
        &mut self,
        which: BlankingPosition,
        value: u8,
    ) -> Result<(), VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(
            0x00,
            0x00,
            which as u8,
            &[0x00, value.max(50) as u8],
        ))?;
        Ok(stream.flush()?)
    }

    fn read_response(
        stream: &mut TcpStream,
    ) -> Result<(usize, Cursor<Vec<u8>>), VirtualDeviceError> {
        let _version = stream.read_u8()?;
        let _category = stream.read_u8()?;
        let _community: i32 = stream.read_i32::<BigEndian>()?;
        let success = stream.read_u8()?;
        let _command = stream.read_i16::<BigEndian>()?;
        let _expected_len = stream.read_u8()? as usize;
        let mut buf = [0u8; 32];
        let len = stream.read(&mut buf)?;

        let data = (&buf[..len]).to_vec();
        if success == 0 {
            Err(VirtualDeviceError::from(format!("error: {:?}", data)))
        } else {
            Ok((len, Cursor::new(data)))
        }
    }

    pub fn get_power_status(&self) -> Result<PowerStatus, VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x01, 0x01, 0x02, &[]))?;
        stream.flush()?;

        let (len, mut data) = Device::read_response(&mut stream)?;
        if len == 2 {
            let status = data.read_i16::<BigEndian>()?;

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

        tracing::info!("pj_talk command: {:?}", bytes);
        bytes
    }
}

impl VirtualDevice for Device {
    fn turn_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x01, 0x30, &[0x00, 0x01]))?;
        stream.flush()?;

        Ok(VirtualDeviceState::On)
    }

    fn turn_off(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let mut stream = self.open()?;
        stream.write_all(&Device::make_command_bytes(0x00, 0x01, 0x30, &[0x00, 0x00]))?;
        stream.flush()?;

        Ok(VirtualDeviceState::Off)
    }

    fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        Ok(match self.get_power_status()? {
            PowerStatus::Standby => VirtualDeviceState::Off,
            PowerStatus::Warming => VirtualDeviceState::On,
            PowerStatus::PowerOn => VirtualDeviceState::On,
            PowerStatus::Cooling => VirtualDeviceState::Off,
        })
    }
}
