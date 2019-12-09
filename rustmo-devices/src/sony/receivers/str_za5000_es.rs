use std::net::IpAddr;

use rustmo_server::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);

#[derive(Clone, Copy)]
pub struct Device {
    ip_address: IpAddr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VideoInput {
    Sat,
    Stb,
    Game,
    Bd,
    Sacd,
    Unknown,
}

#[derive(Deserialize, Debug)]
struct VideoInputResponsePacket {
    id: i32,
    feature: String,
    value: String,
}

impl VideoInputResponsePacket {
    fn unknown() -> Self {
        VideoInputResponsePacket {
            id: 0,
            feature: "unknown".to_string(),
            value: "unknown".to_string()
        }
    }
}

#[derive(Deserialize, Debug)]
struct VideoInputResponse {
    #[serde(rename = "type")]
    ttype: String,
    packet: Vec<VideoInputResponsePacket>,
    event_available: Vec<u8>,
}

/// https://www.sony.com/electronics/av-receivers/str-za5000es
impl Device {
    pub fn new(ip_address: IpAddr) -> Self {
        Device { ip_address }
    }

    pub fn get_video_input(&mut self) -> Result<VideoInput, VirtualDeviceError> {
        if self.is_off() {
            return Ok(VideoInput::Unknown);
        }

        let client = reqwest::ClientBuilder::new().timeout(TIMEOUT).build()?;

        let mut response = client
            .post(format!("http://{}/request.cgi", self.ip_address.to_string()).as_str())
            .body("{\"type\":\"http_get\",\"packet\":[{\"id\":1,\"feature\":\"main.input\"}]}")
            .send()?;

        let response = response.text()?;
        let response: VideoInputResponse = serde_json::from_str(response.as_str())?;

        Ok(match response.packet.get(0).unwrap_or(&VideoInputResponsePacket::unknown()).value.as_str() {
            "sat" => VideoInput::Sat,
            "stb" => VideoInput::Stb,
            "game" => VideoInput::Game,
            "bd" => VideoInput::Bd,
            "sacd" => VideoInput::Sacd,
            _ => VideoInput::Unknown,
        })
    }

    pub fn set_video_input(&mut self, input: &VideoInput) -> Result<VirtualDeviceState, VirtualDeviceError> {
        if self.is_off() {
            return Err(VirtualDeviceError::new("Receiver is turned off"));
        }

        let str = match input {
            VideoInput::Sat => "GUI.sat",
            VideoInput::Stb => "GUI.stb",
            VideoInput::Game => "GUI.game",
            VideoInput::Bd => "GUI.bddvd",
            VideoInput::Sacd => "GUI.sacd",
            VideoInput::Unknown => "unknown",
        };

        let client = reqwest::ClientBuilder::new().timeout(TIMEOUT).build()?;
        let body = format!("{{\"type\":\"http_set\",\"packet\":[{{\"id\":274,\"feature\":\"{str}\",\"value\":\"main\"}}]}}", str = str).clone();
        client
            .post(format!("http://{}/request.cgi", self.ip_address.to_string()).as_str())
            .body(body)
            .send()?;

        if self.get_video_input()?.eq(input) {
            Ok(VirtualDeviceState::On)
        } else {
            Err(VirtualDeviceError("Couldn't change state".to_string()))
        }
    }

    pub fn volume_up(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let client = reqwest::ClientBuilder::new().timeout(TIMEOUT).build()?;

        client.post(format!("http://{}/request.cgi", self.ip_address.to_string()).as_str())
            .body("{\"type\":\"http_set\",\"packet\":[{\"id\":267,\"feature\":\"GUI.volumeup\",\"value\":\"main\"}]}")
            .send()?;

        Ok(VirtualDeviceState::On)
    }

    pub fn volume_down(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let client = reqwest::ClientBuilder::new().timeout(TIMEOUT).build()?;

        client.post(format!("http://{}/request.cgi", self.ip_address.to_string()).as_str())
            .body("{\"type\":\"http_set\",\"packet\":[{\"id\":267,\"feature\":\"GUI.volumedown\",\"value\":\"main\"}]}")
            .send()?;

        Ok(VirtualDeviceState::On)
    }

    pub fn toggle_mute(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let client = reqwest::ClientBuilder::new().timeout(TIMEOUT).build()?;

        client.post(format!("http://{}/request.cgi", self.ip_address.to_string()).as_str())
            .body("{\"type\":\"http_set\",\"packet\":[{\"id\":267,\"feature\":\"GUI.muting\",\"value\":\"main\"}]}")
            .send()?;

        self.check_is_muted()
    }

    fn is_off(&mut self) -> bool {
        self.check_is_on()
            .unwrap_or(VirtualDeviceState::Off)
            .eq(&VirtualDeviceState::Off)
    }

    pub fn check_is_muted(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        if self.is_off() {
            return Ok(VirtualDeviceState::Off);
        }

        let client = reqwest::ClientBuilder::new().timeout(TIMEOUT).build()?;

        let mut response = client.post(format!("http://{}/request.cgi", self.ip_address.to_string()).as_str())
            .body("{\"type\":\"http_get\",\"packet\":[{\"id\":1,\"feature\":\"main.mute\"}]}")
            .send()?;

        let response = response.text()?;
        let response: VideoInputResponse = serde_json::from_str(response.as_str())?;

        Ok(match response.packet.get(0).unwrap_or(&VideoInputResponsePacket::unknown()).value.as_str() {
            "off" => VirtualDeviceState::Off,
            "on" => VirtualDeviceState::On,
            _ => VirtualDeviceState::Off,
        })
    }
}

impl VirtualDevice for Device {
    fn turn_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let client = reqwest::ClientBuilder::new().timeout(TIMEOUT).build()?;

        client.post(format!("http://{}/request.cgi", self.ip_address.to_string()).as_str())
            .body("{\"type\":\"http_set\",\"packet\":[{\"id\":267,\"feature\":\"main.power\",\"value\":\"on\"}]}")
            .send()?;

        Ok(VirtualDeviceState::On)
    }

    fn turn_off(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let client = reqwest::ClientBuilder::new().timeout(TIMEOUT).build()?;

        client.post(format!("http://{}/request.cgi", self.ip_address.to_string()).as_str())
            .body("{\"type\":\"http_set\",\"packet\":[{\"id\":267,\"feature\":\"main.power\",\"value\":\"off\"}]}")
            .send()?;

        Ok(VirtualDeviceState::Off)
    }

    fn check_is_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let client = reqwest::ClientBuilder::new().timeout(TIMEOUT).build()?;
        let mut res = client
            .post("http://192.168.0.237/request.cgi")
            .body("{\"type\":\"http_get\",\"packet\":[{\"id\":1,\"feature\":\"main.input\"}]}")
            .send()?;

        if res.text()?.is_empty() {
            Ok(VirtualDeviceState::Off)
        } else {
            Ok(VirtualDeviceState::On)
        }
    }
}
