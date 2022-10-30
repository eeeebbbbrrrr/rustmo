use rustmo_server::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};
use std::collections::{BTreeMap, HashMap};
use std::process::Command;

#[derive(Clone)]
pub struct Device {
    id: String,
    raop_creds: String,
    airplay_creds: String,
    companion_creds: String,
}

impl Device {
    pub fn new<S: Into<String>>(
        id: S,
        raop_creds: S,
        airplay_creds: S,
        companion_creds: S,
    ) -> Self {
        Self {
            id: id.into(),
            raop_creds: raop_creds.into(),
            airplay_creds: airplay_creds.into(),
            companion_creds: companion_creds.into(),
        }
    }

    pub fn power_status(&mut self) -> Result<bool, VirtualDeviceError> {
        Ok(self.exec(vec!["power_state"])? == "PowerState.On")
    }

    pub fn power_on(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["turn_on"])?;
        Ok(())
    }

    pub fn power_off(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["turn_off"])?;
        Ok(())
    }

    pub fn launch_app(&mut self, bundle_id: &str) -> Result<(), VirtualDeviceError> {
        self.exec(vec![format!("launch_app={bundle_id}")])?;
        Ok(())
    }

    pub fn current_app(&mut self) -> Result<Option<(String, String)>, VirtualDeviceError> {
        let map = Self::parse_map(&self.exec(vec!["app"])?, "\n");
        if let Some(app) = map.get("App") {
            Ok(Self::parse_app_tuple(app))
        } else {
            Ok(None)
        }
    }

    pub fn app_list(
        &mut self,
    ) -> Result<impl Iterator<Item = (String, String)>, VirtualDeviceError> {
        let mut apps = Vec::new();
        for line in self.exec(vec!["app_list"])?.split(", ") {
            let map = Self::parse_map(line, "\n");
            if let Some(app) = map.get("App") {
                if let Some(a) = Self::parse_app_tuple(app) {
                    apps.push(a);
                }
            }
        }
        Ok(apps.into_iter())
    }

    pub fn up(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["up"]).map(|_| ())
    }

    pub fn down(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["down"]).map(|_| ())
    }

    pub fn left(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["left"]).map(|_| ())
    }

    pub fn right(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["right"]).map(|_| ())
    }

    pub fn channel_down(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["channel_down"]).map(|_| ())
    }

    pub fn channel_up(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["channel_up"]).map(|_| ())
    }

    pub fn home(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["home"]).map(|_| ())
    }

    pub fn home_hold(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["home_hold"]).map(|_| ())
    }

    pub fn menu(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["menu"]).map(|_| ())
    }

    pub fn top_menu(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["top_menu"]).map(|_| ())
    }

    pub fn next(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["next"]).map(|_| ())
    }

    pub fn previous(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["previous"]).map(|_| ())
    }

    pub fn play(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["play"]).map(|_| ())
    }

    pub fn pause(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["pause"]).map(|_| ())
    }

    pub fn stop(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["stop"]).map(|_| ())
    }

    pub fn select(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["select"]).map(|_| ())
    }

    pub fn skip_backward(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["skip_backward"]).map(|_| ())
    }

    pub fn skip_forward(&mut self) -> Result<(), VirtualDeviceError> {
        self.exec(vec!["skip_forward"]).map(|_| ())
    }

    pub fn playing(&mut self) -> Result<BTreeMap<String, String>, VirtualDeviceError> {
        Ok(Self::parse_map(&self.exec(vec!["playing"])?, "\n"))
    }

    pub fn paused(&mut self) -> Result<bool, VirtualDeviceError> {
        Ok(self.exec(vec!["device_state"])? == "DeviceState.Paused")
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

    fn exec<S: Into<String>>(&mut self, args: Vec<S>) -> Result<String, VirtualDeviceError> {
        let mut command = Command::new("atvremote");

        command
            .arg("--id")
            .arg(&self.id)
            .arg("--raop-credentials")
            .arg(&self.raop_creds)
            .arg("--airplay-credentials")
            .arg(&self.airplay_creds)
            .arg("--companion-credentials")
            .arg(&self.companion_creds)
            .args(args.into_iter().map(|a| a.into()));

        let output = command.output()?;
        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(result)
        } else {
            Err(VirtualDeviceError::from(format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )))
        }
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

    fn check_is_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        if self.power_status()? {
            Ok(VirtualDeviceState::On)
        } else {
            Ok(VirtualDeviceState::Off)
        }
    }
}
