use byteorder::WriteBytesExt;
use rustmo_server::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

#[derive(Copy, Clone)]
pub struct Device {
    ip: IpAddr,
}

impl Device {
    pub fn new(ip: IpAddr) -> Self {
        Self { ip }
    }

    pub fn standby(&mut self) -> Result<(), VirtualDeviceError> {
        self.send_command("Standby", true).map(|_| ())
    }

    pub fn reset(&mut self) -> Result<(), VirtualDeviceError> {
        self.send_command("ReloadSoftware", true).map(|_| ())
    }

    pub fn aspect_ratio(&self) -> Result<String, VirtualDeviceError> {
        Ok(self.send_command("GetAspectRatio", true)?.pop().unwrap())
    }

    pub fn get_nearest_aspect_ratio(&self) -> Result<usize, VirtualDeviceError> {
        Self::nearest_aspect_ratio_int(self.aspect_ratio()?)
    }

    pub fn custom_zoom_off(&mut self) -> Result<(), VirtualDeviceError> {
        let ar = Self::nearest_aspect_ratio_int(self.aspect_ratio()?)?;

        self.send_command(
            format!("ChangeOption temporary\\customZoomConfig\\active.{ar} NO",),
            false,
        )?;
        Ok(())
    }

    pub fn custom_zoom_on(&mut self) -> Result<(), VirtualDeviceError> {
        let ar = Self::nearest_aspect_ratio_int(self.aspect_ratio()?)?;

        self.send_command(
            format!("ChangeOption temporary\\customZoomConfig\\active.{ar} YES",),
            false,
        )?;
        Ok(())
    }

    fn nearest_aspect_ratio_int(ar: String) -> Result<usize, VirtualDeviceError> {
        static KNOWN_ARS: &[usize] = &[
            119, 133, 137, 143, 166, 177, 185, 200, 220, 235, 240, 255, 266, 276,
        ];
        eprintln!("MADVR NEAREST AR LINE: {ar}");
        let mut parts = ar.split(' ');
        let _ar = parts.next().unwrap();
        let _resolution = parts.next().unwrap();
        let _float = parts.next().unwrap();
        let mut ar_int = parts.next().unwrap().parse::<usize>()?;

        match KNOWN_ARS.binary_search(&ar_int) {
            Ok(_) => Ok(ar_int),
            Err(idx) => Ok(KNOWN_ARS[idx - 1]),
        }
    }

    fn send_command<B: AsRef<[u8]>>(
        &self,
        command: B,
        expect_response: bool,
    ) -> Result<Vec<String>, VirtualDeviceError> {
        let mut socket = TcpStream::connect(&SocketAddr::new(self.ip, 44077))?;
        socket.set_read_timeout(Some(Duration::from_millis(1000)))?;

        let mut reader = BufReader::new(socket.try_clone()?);

        // consume WELCOME message
        reader.read_line(&mut String::new())?;

        // can't write until we do
        socket.write_all(command.as_ref())?;
        socket.write_u8(b'\r')?;
        socket.write_u8(b'\n')?;
        socket.flush()?;

        let mut responses = Vec::new();
        let mut got_ok = false;
        for line in reader.lines() {
            eprintln!("   ENVY line={:?}", line);
            if let Err(e) = line {
                if let ErrorKind::WouldBlock = e.kind() {
                    eprintln!("   ENVY would block... retrying in 250ms");
                    std::thread::sleep(Duration::from_millis(250));
                    continue;
                }
            }
            let line = line?;
            let line = line.trim();

            if line == "OK" {
                if expect_response {
                    got_ok = true;
                    continue;
                } else {
                    break;
                }
            } else if line.starts_with("ERROR") {
                return Err(VirtualDeviceError::from(format!(
                    "{}: {}",
                    String::from_utf8_lossy(command.as_ref()),
                    line
                )));
            }
            responses.push(line.to_string());
            if got_ok {
                break;
            }
        }
        Ok(responses)
    }
}

impl VirtualDevice for Device {
    fn turn_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        todo!("How to turn madvr on?")
        // Ok(VirtualDeviceState::On)
    }

    fn turn_off(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        todo!("How to turn madvr off?")
        // Ok(VirtualDeviceState::Off)
    }

    fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        // if something worked then it's on
        self.aspect_ratio().map(|_| VirtualDeviceState::On)
    }
}
