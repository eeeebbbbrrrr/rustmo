use std::fmt::Debug;
use std::io::Write;
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;

use byteorder::ReadBytesExt;

use rustmo_server::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};

#[derive(Debug)]
pub struct Device {
    ip: IpAddr,
}

#[derive(Debug)]
struct MySocket(TcpStream);

impl Drop for MySocket {
    fn drop(&mut self) {
        std::thread::sleep(Duration::from_millis(30));
    }
}

impl Device {
    pub fn new(ip: IpAddr) -> Self {
        Self { ip }
    }

    pub fn power_status(&self) -> Result<bool, VirtualDeviceError> {
        let status: usize = self.send_command("Z1POW?;", Some("Z1POW"))?.parse()?;
        Ok(status == 1)
    }

    pub fn power_on(&mut self) -> Result<(), VirtualDeviceError> {
        self.send_command("Z1POW1;", None).map(|_| ())
    }

    pub fn power_off(&mut self) -> Result<(), VirtualDeviceError> {
        self.send_command("Z1POW0;", Some("Z1POW")).map(|_| ())
    }

    pub fn inputs(&mut self) -> Result<impl Iterator<Item = (usize, String)>, VirtualDeviceError> {
        let mut socket = self.connect()?;
        let many = self
            .send_command_with_socket(&mut socket, "ICN?;", Some("ICN"))?
            .parse::<usize>()?;
        let mut inputs = Vec::with_capacity(many);
        for i in 1..=many {
            let name = Self::validate_response(&mut socket, Some(format!("IS{}IN", i).as_str()))?;
            inputs.push((i, name));
        }

        Ok(inputs.into_iter())
    }

    pub fn change_input(&mut self, num: usize) -> Result<(), VirtualDeviceError> {
        self.send_command(&format!("Z1INP{};", num), None)
            .map(|_| ())
    }

    pub fn current_input(&mut self) -> Result<usize, VirtualDeviceError> {
        Ok(self.send_command("Z1INP?;", Some("Z1INP"))?.parse()?)
    }

    pub fn get_volume(&mut self) -> Result<(f32, usize), VirtualDeviceError> {
        let dcbl = self.send_command("Z1VOL?;", Some("Z1VOL"))?.parse()?;
        let pct = self.send_command("Z1PVOL?;", Some("Z1PVOL"))?.parse()?;
        Ok((dcbl, pct))
    }

    pub fn set_volume_percent(&mut self, vol: usize) -> Result<(f32, usize), VirtualDeviceError> {
        let mut socket = self.connect()?;
        let pct = self
            .send_command_with_socket(&mut socket, &format!("Z1PVOL{};", vol), Some("Z1PVOL"))?
            .parse()?;
        let dcbl = Self::validate_response(&mut socket, Some("Z1VOL"))?.parse()?;
        Ok((dcbl, pct))
    }

    pub fn set_volume_decibel(&mut self, vol: isize) -> Result<(f32, usize), VirtualDeviceError> {
        let mut socket = self.connect()?;
        let dcbl = self
            .send_command_with_socket(&mut socket, &format!("Z1VOL{};", vol), Some("Z1VOL"))?
            .parse()?;
        let pct = Self::validate_response(&mut socket, Some("Z1PVOL"))?.parse()?;
        Ok((dcbl, pct))
    }

    pub fn volume_up(&mut self) -> Result<(), VirtualDeviceError> {
        self.send_command("Z1VUP;", Some("Z1VOL")).map(|_| ())
    }

    pub fn volume_down(&mut self) -> Result<(), VirtualDeviceError> {
        self.send_command("Z1VDN;", Some("Z1VOL")).map(|_| ())
    }

    pub fn mute(&mut self) -> Result<(), VirtualDeviceError> {
        self.send_command("Z1MUTt;", Some("Z1MUT")).map(|_| ())
    }

    fn connect(&self) -> Result<MySocket, VirtualDeviceError> {
        let socket =
            TcpStream::connect_timeout(&SocketAddr::new(self.ip, 14999), Duration::from_secs(1))?;
        socket.set_read_timeout(Some(Duration::from_millis(5000)))?;
        Ok(MySocket(socket))
    }

    fn send_command<B: AsRef<[u8]> + Debug>(
        &self,
        command: B,
        expected: Option<&str>,
    ) -> Result<String, VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.send_command_with_socket(&mut socket, command, expected)
    }

    fn send_command_with_socket<B: AsRef<[u8]> + Debug>(
        &self,
        socket: &mut MySocket,
        command: B,
        expected: Option<&str>,
    ) -> Result<String, VirtualDeviceError> {
        tracing::info!("avm70: {}", String::from_utf8_lossy(command.as_ref()));
        let bytes = command.as_ref();
        if bytes[bytes.len() - 1] != b';' {
            return Err(VirtualDeviceError::from(format!(
                "malformed AVM command: {}",
                String::from_utf8_lossy(bytes)
            )));
        }
        socket.0.write_all(bytes)?;
        socket.0.flush()?;
        Self::validate_response(socket, expected)
    }

    fn validate_response(
        socket: &mut MySocket,
        expected: Option<&str>,
    ) -> Result<String, VirtualDeviceError> {
        if expected.is_none() {
            return Ok(String::new());
        }
        let mut retries = 0;
        loop {
            let buf = Self::read_response(socket)?;
            let response = String::from_utf8_lossy(&buf).to_string();
            tracing::debug!("AVM RESPONSE: /{}/", response);
            return match expected {
                Some(expected) if response.starts_with(expected) => {
                    Ok(response.trim_start_matches(expected).to_string())
                }
                Some(_) if response.starts_with("!") => Err(VirtualDeviceError::from(response)),
                Some(_) if retries == 10 => Err(VirtualDeviceError::from("Too many retries")),
                Some(_) => {
                    // we got some other, likely async, response
                    tracing::debug!("AVM ASYNC RESPONSE: /{}/", response);

                    // so try again
                    retries += 1;
                    continue;
                }
                None => Ok(String::new()),
            };
        }
    }

    fn read_response(socket: &mut MySocket) -> Result<Vec<u8>, VirtualDeviceError> {
        let mut buf = Vec::new();
        loop {
            let b = socket.0.read_u8()?;
            if b == b';' {
                break;
            }
            buf.push(b);
        }
        Ok(buf)
    }
}

impl VirtualDevice for Device {
    fn turn_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.power_on()?;
        Ok(VirtualDeviceState::On)
    }

    fn turn_off(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.power_off()?;
        Ok(VirtualDeviceState::Off)
    }

    fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        if self.power_status()? {
            Ok(VirtualDeviceState::On)
        } else {
            Ok(VirtualDeviceState::Off)
        }
    }
}
