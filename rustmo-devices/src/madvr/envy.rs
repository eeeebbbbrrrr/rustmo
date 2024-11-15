use std::fmt::Debug;
use std::io::{BufRead, BufReader, LineWriter, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;

use rustmo_server::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};

#[derive(Clone, Debug)]
pub struct Device {
    ip: IpAddr,
    mac: [u8; 6],
}

impl Device {
    pub fn new(ip: IpAddr, mac: [u8; 6]) -> Self {
        Self { ip, mac }
    }

    pub fn power_on(&self) -> Result<(), VirtualDeviceError> {
        let packet = wake_on_lan::MagicPacket::new(&self.mac);
        Ok(packet.send()?)
    }

    pub fn power_off(&self) -> Result<(), VirtualDeviceError> {
        self.send_command("PowerOff", false).map(|_| ())
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

    pub fn custom_zoom_off(&mut self, aspect_ratio: usize) -> Result<(), VirtualDeviceError> {
        self.send_command(
            format!("ChangeOption temporary\\customZoomConfig\\active.{aspect_ratio} NO",),
            false,
        )?;
        Ok(())
    }

    pub fn custom_zoom_on(&mut self, aspect_ratio: usize) -> Result<(), VirtualDeviceError> {
        self.send_command(
            format!("ChangeOption temporary\\customZoomConfig\\active.{aspect_ratio} YES",),
            false,
        )?;
        Ok(())
    }

    pub fn heart_beat(&self) -> Result<(), VirtualDeviceError> {
        self.send_command("HeartBeat", false).map(|_| ())
    }

    fn nearest_aspect_ratio_int(ar: String) -> Result<usize, VirtualDeviceError> {
        static KNOWN_ARS: &[usize] = &[
            119, 133, 137, 143, 166, 177, 185, 200, 220, 235, 240, 255, 266, 276,
        ];
        tracing::debug!("MADVR NEAREST AR LINE: {ar}");
        let mut parts = ar.split(' ');
        let _ar = parts.next().unwrap();
        let _resolution = parts.next().unwrap();
        let _float = parts.next().unwrap();
        let ar_int = parts.next().unwrap().parse::<usize>()?;

        match KNOWN_ARS.binary_search(&ar_int) {
            Ok(_) => Ok(ar_int),
            Err(idx) => Ok(KNOWN_ARS[idx - 1]),
        }
    }

    fn send_command<B: AsRef<[u8]> + Debug>(
        &self,
        command: B,
        expect_response: bool,
    ) -> Result<Vec<String>, VirtualDeviceError> {
        tracing::info!(
            "envy command: {}",
            String::from_utf8_lossy(command.as_ref())
        );
        let socket = TcpStream::connect(&SocketAddr::new(self.ip, 44077))?;
        socket.set_read_timeout(Some(Duration::from_millis(1000)))?;

        let mut reader = BufReader::new(socket.try_clone()?);
        let mut writer = LineWriter::new(socket);

        // consume WELCOME message
        let mut welcome = String::new();
        reader.read_line(&mut welcome)?;
        tracing::debug!("ENVY:  got welcome={}", welcome);

        std::thread::sleep(Duration::from_millis(300));

        // can't write until we do
        writer.write_all(command.as_ref())?;
        writer.write_all(b"\r\n")?;
        writer.flush()?;

        tracing::debug!(
            "ENVY:  send command={}",
            String::from_utf8_lossy(command.as_ref())
        );
        let mut responses = Vec::new();
        let mut got_ok = false;
        tracing::debug!("ENVY:  starting to read");
        for line in reader.lines() {
            tracing::debug!("   ENVY line={:?}", line);
            let line = match line {
                Ok(line) => line,
                Err(e) => {
                    tracing::debug!("ENVY error={:?}", e.kind());
                    return Err(VirtualDeviceError::from(format!("{:?}", e)));
                }
            };
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
    fn turn_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.power_on().map(|_| VirtualDeviceState::On)
    }

    fn turn_off(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.power_off().map(|_| VirtualDeviceState::Off)
    }

    fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        // if something worked then it's on
        self.heart_beat().map(|_| VirtualDeviceState::On)
    }
}
