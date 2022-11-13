#![allow(dead_code)]

use std::fmt::{Debug, Formatter};
use std::net::{IpAddr, SocketAddr};
use std::ops::{Deref, DerefMut};
use std::panic::catch_unwind;
use std::path::Path;
use std::time::Duration;

use serde::de::{Error, Unexpected, Visitor};
use serde::Deserializer;
use telnet::Event;

use rustmo_server::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};

struct MyTelnet {
    inner: telnet::Telnet,
}

impl Deref for MyTelnet {
    type Target = telnet::Telnet;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for MyTelnet {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Debug for MyTelnet {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Telnet()")
    }
}

#[derive(Debug, Clone)]
pub struct Ra2MainRepeater {
    ip: IpAddr,
    uid: String,
    upw: String,
}

#[derive(Clone, Debug)]
pub struct Device {
    ip: IpAddr,
    uid: String,
    upw: String,
    name: String,
    id: usize,
}

#[derive(Debug, Deserialize)]
pub struct ProjectName {
    #[serde(rename = "ProjectName")]
    project_name: String,
    #[serde(rename = "UUID")]
    uuid: usize,
}

#[derive(Debug, Deserialize)]
pub struct Dealer {
    #[serde(rename = "AccountNumber")]
    account_number: String,
    #[serde(rename = "UserID")]
    user_id: String,
}

#[derive(Debug, Deserialize)]
pub struct DealerInformation {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Email")]
    email: String,
    #[serde(rename = "Phone")]
    phone: String,
}

#[derive(Debug, Deserialize)]
pub struct Project {
    #[serde(skip)]
    #[serde(default)]
    ra2: Option<Ra2MainRepeater>,
    #[serde(rename = "ProjectName")]
    project_name: ProjectName,
    #[serde(rename = "Dealer")]
    dealer: Dealer,
    #[serde(rename = "DealerInformation")]
    dealer_information: DealerInformation,
    #[serde(rename = "Latitude")]
    latitude: f32,
    #[serde(rename = "Longitude")]
    longitude: f32,
    #[serde(rename = "Copyright")]
    copyright: String,
    #[serde(rename = "GUID")]
    guid: String,
    #[serde(rename = "ProductType")]
    product_type: usize,
    #[serde(rename = "AppVer")]
    app_version: String,
    #[serde(rename = "XMLVer")]
    xml_ver: String,
    #[serde(rename = "DbExportDate")]
    db_export_date: String,
    #[serde(rename = "DbExportTime")]
    db_export_time: String,
    #[serde(rename = "IsConnectEnabled")]
    #[serde(deserialize_with = "deser_bool")]
    is_connect_enabled: bool,
    #[serde(rename = "Areas")]
    areas: Areas,
}

#[derive(Debug, Deserialize)]
pub struct Areas {
    #[serde(rename = "$value")]
    #[serde(default)]
    children: Vec<Area>,
}

#[derive(Debug, Deserialize)]
pub struct Area {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "UUID")]
    uuid: usize,
    #[serde(rename = "IntegrationID")]
    integration_id: usize,
    #[serde(rename = "OccupancyGroupAssignedToID")]
    occupancy_group_assigned_to_id: usize,
    #[serde(rename = "SortOrder")]
    sort_order: usize,
    #[serde(rename = "DeviceGroups")]
    device_groups: DeviceGroups,
    #[serde(rename = "Scenes")]
    scenes: Scenes,
    #[serde(rename = "ShadeGroups")]
    shade_groups: ShadeGroups,
    #[serde(rename = "Outputs")]
    outputs: Outputs,
    #[serde(rename = "Areas")]
    areas: Areas,
}

#[derive(Debug, Deserialize)]
pub struct DeviceGroups {
    #[serde(rename = "$value")]
    #[serde(default)]
    children: Vec<DeviceGroup>,
}

#[derive(Debug, Deserialize)]
pub struct DeviceGroup {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "SortOrder")]
    sort_order: usize,
    #[serde(rename = "Devices")]
    devices: Option<Vec<Devices>>,
}

#[derive(Debug, Deserialize)]
pub struct Scenes {
    #[serde(rename = "$value")]
    #[serde(default)]
    children: Vec<Scene>,
}

#[derive(Debug, Deserialize)]
pub struct Scene {}

#[derive(Debug, Deserialize)]
pub struct ShadeGroups {
    #[serde(rename = "$value")]
    #[serde(default)]
    children: Vec<ShadeGroup>,
}
#[derive(Debug, Deserialize)]
pub struct ShadeGroup {}

#[derive(Debug, Deserialize)]
pub struct Outputs {
    #[serde(rename = "$value")]
    #[serde(default)]
    children: Vec<Output>,
}
#[derive(Debug, Deserialize)]
pub struct Output {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "UUID")]
    uuid: String,
    #[serde(rename = "IntegrationID")]
    integration_id: usize,
    #[serde(rename = "OutputType")]
    output_type: String,
    #[serde(rename = "Wattage")]
    wattage: usize,
    #[serde(rename = "SortOrder")]
    sort_order: usize,
}

#[derive(Debug, Deserialize)]
pub struct Devices {
    #[serde(rename = "$value")]
    #[serde(default)]
    children: Vec<LutronDevice>,
}

#[derive(Debug, Deserialize)]
pub struct LutronDevice {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "UUID")]
    uuid: String,
    #[serde(rename = "IntegrationID")]
    integration_id: usize,
    #[serde(rename = "DeviceType")]
    device_type: String,
    #[serde(rename = "GangPosition")]
    gang_position: usize,
    #[serde(rename = "SortOrder")]
    sort_order: usize,
    #[serde(rename = "Components")]
    components: Components,
}

#[derive(Debug, Deserialize)]
pub struct Components {
    #[serde(rename = "$value")]
    #[serde(default)]
    children: Vec<Component>,
}

#[derive(Debug, Deserialize)]
pub struct Component {
    #[serde(rename = "ComponentNumber")]
    component_number: usize,
    #[serde(rename = "ComponentType")]
    component_type: String,
}

#[derive(Debug, Copy, Clone)]
pub enum OutputEvent {
    On { id: usize },
    Off { id: usize },
}

impl Ra2MainRepeater {
    pub fn new(ip: IpAddr, username: &str, password: &str) -> Self {
        Ra2MainRepeater {
            ip,
            uid: username.to_string(),
            upw: password.to_string(),
        }
    }

    pub fn turn_on_light(
        &self,
        id: usize,
        percent: f32,
        ttl: Duration,
    ) -> Result<(), VirtualDeviceError> {
        output_set(self.ip, &self.uid, &self.upw, id, percent, ttl)
    }

    pub fn turn_off_light(&self, id: usize) -> Result<(), VirtualDeviceError> {
        output_set(
            self.ip,
            &self.uid,
            &self.upw,
            id,
            0.0,
            Duration::from_secs(0),
        )
    }

    pub fn light_state(&self, id: usize) -> Result<VirtualDeviceState, VirtualDeviceError> {
        output_get(self.ip, &self.uid, &self.upw, id).map(|v| {
            if v > 0.0 {
                VirtualDeviceState::On
            } else {
                VirtualDeviceState::Off
            }
        })
    }

    pub fn monitor_output(
        &self,
        timeout: Duration,
    ) -> Result<crossbeam::channel::Receiver<OutputEvent>, VirtualDeviceError> {
        let ip = self.ip;
        let username = self.uid.clone();
        let password = self.upw.clone();
        let (sender, receiver) = crossbeam::channel::bounded(100);

        std::thread::spawn(move || loop {
            tracing::info!("starting lutron monitor");
            let result = catch_unwind(|| {
                let mut telnet = login(ip, &username, &password)?;
                while let Event::Data(data) = telnet.read()? {
                    let response = String::from_utf8_lossy(&data).to_string();
                    if response.starts_with("~OUTPUT") {
                        let response = response.trim();
                        tracing::debug!("LUTRON MONITOR LINE: {}", response);
                        let mut parts = response.split(',');
                        let _ = parts.next().unwrap();
                        let id: usize = parts.next().unwrap().parse()?;
                        let action: usize = parts.next().unwrap().parse()?;
                        if action == 1 {
                            tracing::info!("lutron light {id} changed");
                            let percent: f64 = parts.next().unwrap().parse()?;
                            sender
                                .send(if percent > 0.0 {
                                    OutputEvent::On { id }
                                } else {
                                    OutputEvent::Off { id }
                                })
                                .expect("failed to send OutputEvent");
                        }
                    }
                }
                Ok::<(), VirtualDeviceError>(())
            });
            std::thread::sleep(timeout.clone());
            tracing::info!("LUTRON MONITOR RESULT: {:?}", result);
        });

        Ok(receiver)
    }

    pub fn describe(&self) -> Result<Project, VirtualDeviceError> {
        let mut telnet = login(self.ip, &self.uid, &self.upw)?;
        let xml = send_command(&mut telnet, "?SYSTEM,12")?.join("");
        let mut project = serde_xml_rs::from_str::<Project>(&xml)?;
        project.ra2 = Some(self.clone());
        Ok(project)
    }

    pub fn describe_from_file<P: AsRef<Path> + Debug>(
        &self,
        path: P,
    ) -> Result<Project, VirtualDeviceError> {
        let xml = std::fs::read_to_string(path.as_ref())?;
        self.describe_from_xml(&xml)
    }

    pub fn describe_from_xml(&self, xml: &str) -> Result<Project, VirtualDeviceError> {
        let mut project = serde_xml_rs::from_str::<Project>(xml)?;
        project.ra2 = Some(self.clone());
        Ok(project)
    }
}

impl Project {
    pub fn into_iter(self) -> impl Iterator<Item = Device> {
        let project = self;
        let mut devices = Vec::new();

        fn find_output(
            ra2: &Ra2MainRepeater,
            areas: &Areas,
            devices: &mut Vec<Device>,
            name: String,
        ) {
            for area in &areas.children {
                for output in &area.outputs.children {
                    devices.push(Device::new(
                        ra2.ip,
                        &ra2.uid,
                        &ra2.upw,
                        format!("{} {} {}", name, area.name, output.name)
                            .trim()
                            .to_string(),
                        output.integration_id,
                    ));
                }

                find_output(ra2, &area.areas, devices, format!("{} {}", name, area.name));
            }
        }

        let ra2 = project.ra2.unwrap();
        find_output(
            &ra2,
            &project.areas.children.first().unwrap().areas,
            &mut devices,
            Default::default(),
        );
        devices.into_iter()
    }
}

impl Device {
    pub fn new(
        ip: IpAddr,
        username: &str,
        password: &str,
        name: String,
        integration_id: usize,
    ) -> Self {
        Device {
            ip,
            uid: username.to_string(),
            upw: password.to_string(),
            name,
            id: integration_id,
        }
    }

    pub fn turn_off(&self) -> Result<(), VirtualDeviceError> {
        output_set(
            self.ip,
            &self.uid,
            &self.upw,
            self.id,
            0.0,
            Duration::from_secs(0),
        )
    }

    pub fn turn_on(&self, percent: f32, ttl: Duration) -> Result<(), VirtualDeviceError> {
        output_set(self.ip, &self.uid, &self.upw, self.id, percent, ttl)
    }

    pub fn state(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        output_get(self.ip, &self.uid, &self.upw, self.id).map(|v| {
            if v > 0.0 {
                VirtualDeviceState::On
            } else {
                VirtualDeviceState::Off
            }
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn id(&self) -> usize {
        self.id
    }
}

pub fn output_set(
    ip: IpAddr,
    uid: &str,
    upw: &str,
    id: usize,
    percent: f32,
    ttl: Duration,
) -> Result<(), VirtualDeviceError> {
    let mut telnet = login(ip, &uid, &upw)?;
    let response = send_command(
        &mut telnet,
        &format!("#OUTPUT,{},1,{},{}", id, percent, ttl.as_secs()),
    )?;
    tracing::debug!("{:#?}", response);
    Ok(())
}

pub fn output_get(ip: IpAddr, uid: &str, upw: &str, id: usize) -> Result<f32, VirtualDeviceError> {
    let mut telnet = login(ip, &uid, &upw)?;
    let response = send_command(&mut telnet, &format!("?OUTPUT,{},1", id))?
        .into_iter()
        .filter(|line| line.starts_with(&format!("~OUTPUT,{}", id)))
        .map(|line| line.trim().to_string())
        .collect::<String>();
    let response = response.trim();

    tracing::debug!("LUTRON OUTPUT RESPONSE for {}: /{}/", id, response);
    if response.is_empty() {
        return Err(VirtualDeviceError::new("empty response from lutron"));
    }

    match catch_unwind(|| {
        tracing::debug!("LUTRON RESPONSE: /{}/", response);
        let mut parts = response.split(',');
        let _command = parts.next().unwrap();
        let _id = parts.next().unwrap();
        let _action = parts.next().unwrap();
        let percent = parts.next().unwrap();
        percent.parse()
    }) {
        Ok(percent) => Ok(percent?),
        Err(e) => {
            tracing::debug!("OUTPUT_GET ERROR: {:?}", e);
            Err(VirtualDeviceError::from(format!("{:?}", e)))
        }
    }
}

fn login(ip: IpAddr, uid: &str, upw: &str) -> Result<MyTelnet, VirtualDeviceError> {
    let mut telnet = MyTelnet {
        inner: telnet::Telnet::connect_timeout(
            &SocketAddr::new(ip, 23),
            1024,
            Duration::from_millis(1000),
        )?,
    };

    loop {
        let event = telnet.read_timeout(Duration::from_millis(1000))?;
        if let Event::Data(bytes) = event {
            tracing::debug!("LUTRON EVENT DATA: {:?}", String::from_utf8_lossy(&bytes));
            match bytes.as_ref() {
                b"login: " => send_line(&mut telnet, uid)?,
                b"password: " => send_line(&mut telnet, upw)?,
                b"\r\nGNET> \0" => break,
                _ => {
                    let prompt = String::from_utf8_lossy(&bytes);
                    if prompt.contains("GNET> ") {
                        // just in case the arm above didn't quite get it
                        break;
                    } else if prompt.starts_with("~") {
                        // it's some response from a prior command that came from somewhere
                        continue;
                    }

                    return Err(VirtualDeviceError::from(format!(
                        "unrecognized prompt: /{}/",
                        prompt
                    )));
                }
            }
        } else {
            break;
        }
    }

    Ok(telnet)
}

fn send_line(telnet: &mut MyTelnet, line: &str) -> Result<(), VirtualDeviceError> {
    telnet.write(line.as_bytes())?;
    telnet.write(b"\r\n")?;
    Ok(())
}

fn send_command(telnet: &mut MyTelnet, command: &str) -> Result<Vec<String>, VirtualDeviceError> {
    tracing::info!("lutron command: {}", command);
    telnet.write(command.as_bytes())?;
    telnet.write(b"\r\n")?;

    let mut responses = Vec::new();
    while let Event::Data(response) = telnet.read_timeout(Duration::from_millis(1000))? {
        let response = String::from_utf8_lossy(&response);
        if response.contains("GNET> ") {
            return Ok(responses);
        } else if response.contains("~ERROR") {
            return Err(VirtualDeviceError::from(response));
        }
        responses.push(response.to_string());
    }
    Err(VirtualDeviceError::from("unrecognized telnet event: {}"))
}

fn deser_bool<'de, D>(input: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    struct V;
    impl<'a> Visitor<'a> for V {
        type Value = bool;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            write!(formatter, "a boolean")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: Error,
        {
            match v.to_lowercase().as_str() {
                "true" => Ok(true),
                "false" => Ok(false),
                other => Err(Error::invalid_value(
                    Unexpected::Str(&other),
                    &"true/false of any case",
                )),
            }
        }
    }
    input.deserialize_str(V)
}

impl VirtualDevice for Ra2MainRepeater {
    fn turn_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        Ok(VirtualDeviceState::On)
    }

    fn turn_off(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        // doesn't turn off
        Ok(VirtualDeviceState::On)
    }

    fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        Ok(VirtualDeviceState::On)
    }
}

impl VirtualDevice for Device {
    fn turn_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.turn_on(33.0, Duration::from_secs(3))?;
        Ok(VirtualDeviceState::On)
    }

    fn turn_off(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.turn_off()?;
        Ok(VirtualDeviceState::Off)
    }

    fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.state()
    }
}
