use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io::{BufRead, BufReader, Lines, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use rustmo_server::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};
use tracing::warn;

/// wrapper to control the `atvremote` CLI from here:  https://github.com/eeeebbbbrrrr/pyatv/tree/endless-loop
#[derive(Debug)]
struct AtvRemoteProcess {
    stdin: Option<ChildStdin>,
    stdout: Option<Lines<BufReader<ChildStdout>>>,
}

impl AtvRemoteProcess {
    fn new() -> Result<Self, VirtualDeviceError> {
        let (mut child, lines) = Self::start_process()?;
        Ok(Self {
            stdin: child.stdin.take(),
            stdout: Some(lines),
        })
    }

    fn start_process() -> Result<(Child, Lines<BufReader<ChildStdout>>), VirtualDeviceError> {
        let mut child = Command::new("atvremote")
            .arg("loop")
            .env("PYTHONUNBUFFERED", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().unwrap();
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        for line in lines.next() {
            if line? == "awaiting input..." {
                break;
            }
        }
        Ok((child, lines))
    }

    fn send_command<S: AsRef<str>>(&mut self, args: S) -> Result<String, VirtualDeviceError> {
        let mut retries = 0;
        loop {
            if retries > 10 {
                return Err(VirtualDeviceError::new(
                    "tried to restart atvremote too many times",
                ));
            }
            let result: Result<String, VirtualDeviceError> = {
                self.stdin
                    .as_mut()
                    .ok_or(VirtualDeviceError::new("atvremote process died"))?
                    .write(args.as_ref().as_bytes())?;

                let mut response = String::new();
                for line in self
                    .stdout
                    .as_mut()
                    .ok_or(VirtualDeviceError::new("atvremote process died"))?
                {
                    let line = line?;
                    if line == "awaiting input..." {
                        break;
                    } else {
                        response.push_str(&line);
                        response.push('\n');
                    }
                }
                Ok(response)
            };

            let response = match result {
                Ok(response) => response,
                Err(e) => {
                    let (mut child, lines) = AtvRemoteProcess::start_process()?;
                    self.stdin = child.stdin.take();
                    self.stdout = Some(lines);
                    retries += 1;
                    warn!("restarting atvremote process: {e}");
                    continue;
                }
            };

            return Ok(response.trim().to_string());
        }
    }
}

impl Drop for AtvRemoteProcess {
    fn drop(&mut self) {
        tracing::info!("terminating atvremote process");
        if let Some(stdin) = self.stdin.as_mut() {
            stdin.write(b"quit\n").ok();
        }
    }
}

#[derive(Debug)]
pub struct Device {
    id: String,
    raop_creds: String,
    airplay_creds: String,
    companion_creds: String,
    process: AtvRemoteProcess,
}

impl Device {
    pub fn new<S: Into<String>>(
        id: S,
        raop_creds: S,
        airplay_creds: S,
        companion_creds: S,
    ) -> Result<Self, VirtualDeviceError> {
        Ok(Self {
            id: id.into(),
            raop_creds: raop_creds.into(),
            airplay_creds: airplay_creds.into(),
            companion_creds: companion_creds.into(),
            process: AtvRemoteProcess::new()?,
        })
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

    pub fn open_url(&mut self, url: &str) -> Result<(), VirtualDeviceError> {
        self.exec(vec![format!("open_url={url}")])?;
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

    fn exec<S: Into<String> + Debug>(
        &mut self,
        args: Vec<S>,
    ) -> Result<String, VirtualDeviceError> {
        tracing::info!("appletv: {:?}", args);
        let mut command = Vec::<String>::new();

        command.push("--id".to_string());
        command.push(self.id.clone());
        command.push("--raop-credentials".to_string());
        command.push(self.raop_creds.clone());
        command.push("--airplay-credentials".to_string());
        command.push(self.airplay_creds.clone());
        command.push("--companion-credentials".to_string());
        command.push(self.companion_creds.clone());
        command.extend(args.into_iter().map(|a| a.into()));

        tracing::debug!("APPLETV COMMAND: {:?}", command);

        let command_string = command.join(" ") + "\n";
        self.process.send_command(command_string)
    }
}

impl VirtualDevice for Device {
    fn turn_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let mut d = Device::new(
            self.id.clone(),
            self.raop_creds.clone(),
            self.airplay_creds.clone(),
            self.companion_creds.clone(),
        )?;
        d.power_on()?;
        Ok(VirtualDeviceState::On)
    }

    fn turn_off(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let mut d = Device::new(
            self.id.clone(),
            self.raop_creds.clone(),
            self.airplay_creds.clone(),
            self.companion_creds.clone(),
        )?;
        d.power_off()?;
        Ok(VirtualDeviceState::Off)
    }

    fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let mut d = Device::new(
            self.id.clone(),
            self.raop_creds.clone(),
            self.airplay_creds.clone(),
            self.companion_creds.clone(),
        )?;
        let status = d.power_status()?;
        if status {
            Ok(VirtualDeviceState::On)
        } else {
            Ok(VirtualDeviceState::Off)
        }
    }
}
