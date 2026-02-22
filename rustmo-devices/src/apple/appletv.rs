use std::collections::BTreeMap;
use std::process::Command;

use rustmo_server::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};

#[derive(Debug, Clone)]
pub struct Device {
    id: String,
    ip: std::net::IpAddr,
    raop_creds: String,
    airplay_creds: String,
    companion_creds: String,
}

impl Device {
    pub fn new<S: Into<String>>(
        id: S,
        ip: std::net::IpAddr,
        raop_creds: S,
        airplay_creds: S,
        companion_creds: S,
    ) -> Self {
        Self {
            id: id.into(),
            ip,
            raop_creds: raop_creds.into(),
            airplay_creds: airplay_creds.into(),
            companion_creds: companion_creds.into(),
        }
    }

    pub fn power_status(&self) -> Result<bool, VirtualDeviceError> {
        Ok(self.exec("power_state")? == "PowerState.On")
    }

    pub fn power_on(&self) -> Result<(), VirtualDeviceError> {
        self.exec("turn_on")?;
        Ok(())
    }

    pub fn power_off(&self) -> Result<(), VirtualDeviceError> {
        self.exec("turn_off")?;
        Ok(())
    }

    pub fn launch_app(&self, bundle_id: &str) -> Result<(), VirtualDeviceError> {
        self.exec(&format!("launch_app={bundle_id}"))?;
        Ok(())
    }

    pub fn open_url(&self, url: &str) -> Result<(), VirtualDeviceError> {
        self.exec(&format!("open_url={url}"))?;
        Ok(())
    }

    pub fn current_app(&self) -> Result<Option<(String, String)>, VirtualDeviceError> {
        let map = Self::parse_map(&self.exec("app")?, "\n");
        if let Some(app) = map.get("App") {
            Ok(Self::parse_app_tuple(app))
        } else {
            Ok(None)
        }
    }

    pub fn app_list(&self) -> Result<impl Iterator<Item = (String, String)>, VirtualDeviceError> {
        let mut apps = Vec::new();
        for line in self.exec("app_list")?.split(", ") {
            let map = Self::parse_map(line, "\n");
            if let Some(app) = map.get("App") {
                if let Some(a) = Self::parse_app_tuple(app) {
                    apps.push(a);
                }
            }
        }
        Ok(apps.into_iter())
    }

    pub fn up(&self) -> Result<(), VirtualDeviceError> {
        self.exec("up").map(|_| ())
    }

    pub fn down(&self) -> Result<(), VirtualDeviceError> {
        self.exec("down").map(|_| ())
    }

    pub fn left(&self) -> Result<(), VirtualDeviceError> {
        self.exec("left").map(|_| ())
    }

    pub fn right(&self) -> Result<(), VirtualDeviceError> {
        self.exec("right").map(|_| ())
    }

    pub fn channel_down(&self) -> Result<(), VirtualDeviceError> {
        self.exec("channel_down").map(|_| ())
    }

    pub fn channel_up(&self) -> Result<(), VirtualDeviceError> {
        self.exec("channel_up").map(|_| ())
    }

    pub fn home(&self) -> Result<(), VirtualDeviceError> {
        self.exec("home").map(|_| ())
    }

    pub fn home_hold(&self) -> Result<(), VirtualDeviceError> {
        self.exec("home_hold").map(|_| ())
    }

    pub fn menu(&self) -> Result<(), VirtualDeviceError> {
        self.exec("menu").map(|_| ())
    }

    pub fn top_menu(&self) -> Result<(), VirtualDeviceError> {
        self.exec("top_menu").map(|_| ())
    }

    pub fn next(&self) -> Result<(), VirtualDeviceError> {
        self.exec("next").map(|_| ())
    }

    pub fn previous(&self) -> Result<(), VirtualDeviceError> {
        self.exec("previous").map(|_| ())
    }

    pub fn play(&self) -> Result<(), VirtualDeviceError> {
        self.exec("play").map(|_| ())
    }

    pub fn pause(&self) -> Result<(), VirtualDeviceError> {
        self.exec("pause").map(|_| ())
    }

    pub fn stop(&self) -> Result<(), VirtualDeviceError> {
        self.exec("stop").map(|_| ())
    }

    pub fn select(&self) -> Result<(), VirtualDeviceError> {
        self.exec("select").map(|_| ())
    }

    pub fn skip_backward(&self) -> Result<(), VirtualDeviceError> {
        self.exec("skip_backward").map(|_| ())
    }

    pub fn skip_forward(&self) -> Result<(), VirtualDeviceError> {
        self.exec("skip_forward").map(|_| ())
    }

    pub fn playing(&self) -> Result<BTreeMap<String, String>, VirtualDeviceError> {
        Ok(Self::parse_map(&self.exec("playing")?, "\n"))
    }

    pub fn paused(&self) -> Result<bool, VirtualDeviceError> {
        Ok(self.exec("device_state")? == "DeviceState.Paused")
    }

    fn parse_app_tuple(app: &String) -> Option<(String, String)> {
        if let Some((name, bundle_id)) = app.split_once(" (") {
            Some((
                bundle_id.trim_matches(')').to_string(),
                name.trim().to_string(),
            ))
        } else {
            None
        }
    }

    fn parse_map(input: &str, line_sep: &str) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        for line in input.split(line_sep) {
            match line.split_once(": ") {
                Some((k, v)) => {
                    map.insert(k.trim().to_string(), v.trim().to_string());
                }
                None => {}
            }
        }
        map
    }

    fn exec(&self, atvremote_command: &str) -> Result<String, VirtualDeviceError> {
        tracing::info!("appletv: {}", atvremote_command);

        let output = Command::new("atvremote")
            .arg("--id")
            .arg(&self.id)
            .arg("--scan-hosts")
            .arg(self.ip.to_string())
            .arg("--raop-credentials")
            .arg(&self.raop_creds)
            .arg("--airplay-credentials")
            .arg(&self.airplay_creds)
            .arg("--companion-credentials")
            .arg(&self.companion_creds)
            .arg(atvremote_command)
            .output()
            .map_err(|e| VirtualDeviceError::from(format!("failed to run atvremote: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        if !stderr.is_empty() {
            tracing::warn!("atvremote stderr: {stderr}");
        }

        tracing::info!("atvremote response: {stdout}");

        if !output.status.success() {
            return Err(VirtualDeviceError::from(format!(
                "atvremote '{}' failed (exit {}): {}",
                atvremote_command,
                output.status.code().unwrap_or(-1),
                if stdout.is_empty() { &stderr } else { &stdout }
            )));
        }

        Ok(stdout)
    }
}

impl VirtualDevice for Device {
    fn turn_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.power_on()?;
        Ok(VirtualDeviceState::On)
    }

    fn turn_off(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
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
