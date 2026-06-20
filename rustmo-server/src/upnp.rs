use std::borrow::BorrowMut;
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::thread;

use hyper::method::Method;
use hyper::server::{Fresh, Handler, Request, Response};
use regex::Regex;
use serde_xml_rs::from_reader;

use crate::virtual_device::{VirtualDeviceError, VirtualDeviceState};
use crate::{RustmoDevice, RustmoDeviceInfo, VirtualDevicesList};

#[derive(Debug, Deserialize)]
pub(crate) struct BinaryState {
    #[serde(rename = "BinaryState")]
    pub(crate) binary_state: Option<u8>,
    #[serde(rename = "brightness")]
    pub(crate) brightness: Option<u8>,
    #[serde(rename = "Brightness")]
    pub(crate) brightness_upper: Option<u8>,
    #[serde(rename = "level")]
    pub(crate) level: Option<u8>,
    #[serde(rename = "Level")]
    pub(crate) level_upper: Option<u8>,
}

impl BinaryState {
    fn percent(&self) -> Option<u8> {
        self.brightness
            .or(self.brightness_upper)
            .or(self.level)
            .or(self.level_upper)
            .map(|percent| percent.min(100))
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct UpnpBody {
    #[serde(rename = "GetBinaryState")]
    pub(crate) get_binary_state: Option<BinaryState>,

    #[serde(rename = "SetBinaryState")]
    pub(crate) set_binary_state: Option<BinaryState>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpnpEnvelope {
    #[serde(rename = "Body")]
    pub(crate) body: UpnpBody,
}

pub(crate) struct DeviceHttpServerHandler {
    device: RustmoDevice,
}

unsafe impl Sync for DeviceHttpServerHandler {}
unsafe impl Send for DeviceHttpServerHandler {}

pub(crate) fn start_hue_bridge_http_server(
    bridge: RustmoDeviceInfo,
    bind_port: u16,
    devices: VirtualDevicesList,
) -> bool {
    let bind_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), bind_port);
    let server = match hyper::Server::http(bind_address) {
        Ok(server) => server,
        Err(error) => {
            tracing::warn!(
                "unable to start Hue bridge server on {} advertised as {}:{}: {}",
                bind_address,
                bridge.ip_address,
                bridge.port,
                error
            );
            return false;
        }
    };

    tracing::info!(
        "starting Hue bridge HTTP server on {} advertised as http://{}:{}/description.xml",
        bind_address,
        bridge.ip_address,
        bridge.port
    );

    thread::spawn(move || {
        server
            .handle(HueBridgeHttpServerHandler::new(bridge, devices))
            .unwrap();
    });

    true
}

impl Handler for DeviceHttpServerHandler {
    fn handle<'r>(&'r self, mut request: Request<'r, '_>, mut response: Response<'r, Fresh>) {
        tracing::info!(
            "UPNP request: http://{}:{}{} from {}",
            self.device.info.ip_address,
            self.device.info.port,
            request.uri,
            request.remote_addr
        );
        let request_method = request.method.clone();
        let body = match request.uri.to_string().as_str() {
            "/setup.xml" => Some(Body::Xml(self.handle_setup())),
            "/description.xml" if self.device.supports_percent() => {
                Some(Body::Xml(self.handle_hue_description()))
            }
            "/eventservice.xml" => Some(Body::Xml(self.handle_eventservice())),
            "/metainfoservice.xml" => Some(Body::Xml(self.handle_metainfoservice())),
            "/upnp/control/basicevent1" => {
                Some(Body::Xml(self.handle_basicevent(request.borrow_mut())))
            }
            path if self.device.supports_percent() && path.starts_with("/api") => Some(Body::Json(
                self.handle_hue_api(&request_method, path, request.borrow_mut()),
            )),
            _ => {
                tracing::warn!("Unrecognized request: {:?}", request.uri);
                *response.status_mut() = hyper::status::StatusCode::NotFound;
                None
            }
        };

        if let Some(data) = body {
            *response.status_mut() = hyper::status::StatusCode::Ok;
            let content_type = data.content_type();
            response
                .headers_mut()
                .append_raw("CONTENT-TYPE", content_type.as_bytes().to_vec());
            response.send(data.as_slice()).unwrap();
        }
    }
}

pub(crate) struct HueBridgeHttpServerHandler {
    bridge: RustmoDeviceInfo,
    devices: VirtualDevicesList,
}

unsafe impl Sync for HueBridgeHttpServerHandler {}
unsafe impl Send for HueBridgeHttpServerHandler {}

impl Handler for HueBridgeHttpServerHandler {
    fn handle<'r>(&'r self, mut request: Request<'r, '_>, mut response: Response<'r, Fresh>) {
        tracing::info!(
            "HUE bridge request: http://{}:{}{} from {}",
            self.bridge.ip_address,
            self.bridge.port,
            request.uri,
            request.remote_addr
        );

        let method = request.method.clone();
        let path = request.uri.to_string();
        let body = match path.as_str() {
            "/description.xml" => Some(Body::Xml(self.handle_description())),
            path if path.starts_with("/api") => Some(Body::Json(self.handle_api(
                &method,
                path,
                request.borrow_mut(),
            ))),
            _ => {
                tracing::warn!("Unrecognized Hue bridge request: {:?}", request.uri);
                *response.status_mut() = hyper::status::StatusCode::NotFound;
                None
            }
        };

        if let Some(data) = body {
            *response.status_mut() = hyper::status::StatusCode::Ok;
            response
                .headers_mut()
                .append_raw("CONTENT-TYPE", data.content_type().as_bytes().to_vec());
            response.send(data.as_slice()).unwrap();
        }
    }
}

impl HueBridgeHttpServerHandler {
    fn new(bridge: RustmoDeviceInfo, devices: VirtualDevicesList) -> Self {
        Self { bridge, devices }
    }

    fn handle_description(&self) -> Vec<u8> {
        tracing::info!(
            "HUE bridge description requested, advertised as http://{}:{}/description.xml",
            self.bridge.ip_address,
            self.bridge.port
        );
        make_hue_description(&self.bridge).into_bytes()
    }

    fn handle_api(&self, method: &Method, path: &str, request: &mut Request<'_, '_>) -> Vec<u8> {
        tracing::info!("HUE bridge API request: {} {}", method, path);
        match (method, path) {
            (&Method::Post, "/api") | (&Method::Post, "/api/") => {
                tracing::info!("HUE bridge link request accepted with username `rustmo`");
                br#"[{"success":{"username":"rustmo"}}]"#.to_vec()
            }
            (&Method::Get, "/api") | (&Method::Get, "/api/") => self.make_bridge_response(),
            (&Method::Get, path) if is_hue_username_path(path) => self.make_bridge_response(),
            (&Method::Get, path) if path.ends_with("/config") => {
                make_hue_config_response(&self.bridge)
            }
            (&Method::Get, path) if path.ends_with("/groups") => br#"{}"#.to_vec(),
            (&Method::Get, path) if path.ends_with("/lights") => self.make_lights_response(),
            (&Method::Get, path) if path.contains("/lights/") && !path.ends_with("/state") => {
                self.make_light_response(path)
            }
            (&Method::Put, path) if path.ends_with("/state") => {
                self.handle_set_light_state(path, request)
            }
            _ => br#"[]"#.to_vec(),
        }
    }

    fn make_bridge_response(&self) -> Vec<u8> {
        let light_count = self
            .devices
            .read()
            .iter()
            .filter(|device| device.supports_percent())
            .count();
        tracing::info!("HUE bridge full state requested, exposing {light_count} dimmable lights");
        serde_json::to_vec(&serde_json::json!({
            "lights": self.hue_lights_json(),
            "config": make_hue_config_json(&self.bridge)
        }))
        .unwrap()
    }

    fn make_lights_response(&self) -> Vec<u8> {
        let light_count = self
            .devices
            .read()
            .iter()
            .filter(|device| device.supports_percent())
            .count();
        tracing::info!("HUE bridge lights requested, exposing {light_count} dimmable lights");
        serde_json::to_vec(&self.hue_lights_json()).unwrap()
    }

    fn make_light_response(&self, path: &str) -> Vec<u8> {
        let Some(id) = hue_light_id_from_path(path) else {
            return br#"[]"#.to_vec();
        };
        let devices = self.devices.read();
        let Some((_, device)) = percent_devices(&devices)
            .into_iter()
            .find(|(device_id, _)| *device_id == id)
        else {
            tracing::warn!("HUE bridge light request for unknown light id {id}");
            return br#"[]"#.to_vec();
        };

        tracing::info!("HUE bridge light {id} requested: {}", device.info.name);
        serde_json::to_vec(&hue_light_json(device)).unwrap()
    }

    fn handle_set_light_state(&self, path: &str, request: &mut Request<'_, '_>) -> Vec<u8> {
        let Some(id) = hue_light_id_from_path(path) else {
            return br#"[]"#.to_vec();
        };

        let mut content = String::new();
        request.read_to_string(&mut content).unwrap();
        let command = serde_json::from_str::<serde_json::Value>(&content).unwrap_or_default();
        let devices = self.devices.read();
        let Some((_, device)) = percent_devices(&devices)
            .into_iter()
            .find(|(device_id, _)| *device_id == id)
        else {
            tracing::warn!("HUE bridge state request for unknown light id {id}");
            return br#"[]"#.to_vec();
        };

        tracing::info!(
            "HUE bridge state update for light {id}: {}",
            device.info.name
        );
        handle_hue_set_state(device, id, command)
    }

    fn hue_lights_json(&self) -> serde_json::Map<String, serde_json::Value> {
        let devices = self.devices.read();
        percent_devices(&devices)
            .into_iter()
            .map(|(id, device)| (id.to_string(), hue_light_json(device)))
            .collect()
    }
}

enum Body {
    Json(Vec<u8>),
    Xml(Vec<u8>),
}

impl Body {
    fn as_slice(&self) -> &[u8] {
        match self {
            Body::Json(body) | Body::Xml(body) => body.as_slice(),
        }
    }

    fn content_type(&self) -> &'static str {
        match self {
            Body::Json(_) => "application/json",
            Body::Xml(_) => "text/xml",
        }
    }
}

impl DeviceHttpServerHandler {
    pub(crate) fn new(device: RustmoDevice) -> Self {
        DeviceHttpServerHandler { device }
    }

    fn handle_basicevent(&self, request: &mut Request<'_, '_>) -> Vec<u8> {
        let re = Regex::new("^\".*[#](.*)\"$").unwrap();
        let action =
            String::from_utf8(request.headers.get_raw("SOAPACTION").unwrap()[0].clone()).unwrap();
        let action = re
            .captures(action.as_str())
            .unwrap()
            .get(1)
            .unwrap()
            .as_str();

        let mut content = String::new();
        request.read_to_string(&mut content).unwrap();
        content = content.replace("\"s:", "\" s:");

        let envelope: UpnpEnvelope =
            from_reader(content.as_bytes()).unwrap_or_else(|e| panic!("{}:\n{}", e, content));
        let get_or_set;
        let mut percent = None;
        let on_off = match action {
            "GetBinaryState" => {
                tracing::info!(
                    "UPNP get binary state: {} by {}",
                    self.device.info.name,
                    request.remote_addr.ip()
                );

                get_or_set = "Get";
                percent = self.device.check_percent().unwrap_or(None);
                self.device.check_is_on()
            }
            "SetBinaryState" => {
                get_or_set = "Set";
                match envelope.body.set_binary_state {
                    Some(state) => {
                        percent = state.percent();
                        if let Some(percent) = percent {
                            tracing::info!(
                                "UPNP set brightness: {} to {}% by {}",
                                self.device.info.name,
                                percent,
                                request.remote_addr.ip()
                            );
                            self.device.set_percent(percent)
                        } else if state.binary_state == Some(1) {
                            tracing::info!(
                                "UPNP turn on: {} by {}",
                                self.device.info.name,
                                request.remote_addr.ip()
                            );
                            self.device.turn_on()
                        } else {
                            tracing::info!(
                                "PNP turn off: {} by {}",
                                self.device.info.name,
                                request.remote_addr.ip()
                            );
                            self.device.turn_off()
                        }
                    }
                    None => Err(VirtualDeviceError::new(
                        "No BinaryState data for SetBinaryState",
                    )),
                }
            }
            capture => {
                tracing::error!("Unknown capture value: /{}/ from /{}/", capture, action);
                return Vec::new();
            }
        };

        match on_off {
            Ok(state) => {
                DeviceHttpServerHandler::make_basicevent_response(state, get_or_set, percent)
            }
            Err(e) => {
                tracing::error!("Problem with {}: {}", self.device.info.name, e.0);
                vec![]
            }
        }
    }

    fn make_basicevent_response(
        state: VirtualDeviceState,
        get_or_set: &str,
        percent: Option<u8>,
    ) -> Vec<u8> {
        let brightness = percent
            .map(|percent| format!("<brightness>{percent}</brightness>"))
            .unwrap_or_default();
        let soap = format!(
            "<s:Envelope xmlns:s='http://schemas.xmlsoap.org/soap/envelope/'
                        s:encodingStyle='http://schemas.xmlsoap.org/soap/encoding/'>
                <s:Body>
                    <u:{action}BinaryStateResponse xmlns:u='urn:Belkin:service:basicevent:1'>
                        <BinaryState>{state}</BinaryState>
                        {brightness}
                    </u:{action}BinaryStateResponse>
                </s:Body>
            </s:Envelope>",
            action = get_or_set,
            state = match state {
                VirtualDeviceState::On => 1,
                VirtualDeviceState::Off => 0,
            },
            brightness = brightness
        );

        soap.as_bytes().to_vec()
    }

    fn handle_setup(&self) -> Vec<u8> {
        tracing::info!("UPNP set: {}", self.device.info.name);
        format!(
            "<root>
                <device>
                    <deviceType>urn:Belkin:device:controllee:1</deviceType>
                    <friendlyName>{device_name}</friendlyName>
                    <manufacturer>Belkin International Inc.</manufacturer>
                    <modelName>Socket</modelName>
                    <modelNumber>3.1415</modelNumber>
                    <modelDescription>Belkin Plugin Socket 1.0</modelDescription>
                    <UDN>uuid:{uuid}</UDN>
                    <serialNumber>221517K0101769</serialNumber>
                    <binaryState>0</binaryState>
                    <serviceList>
                        <service>
                            <serviceType>urn:Belkin:service:basicevent:1</serviceType>
                            <serviceId>urn:Belkin:serviceId:basicevent1</serviceId>
                            <controlURL>/upnp/control/basicevent1</controlURL>
                            <eventSubURL>/upnp/event/basicevent1</eventSubURL>
                            <SCPDURL>/eventservice.xml</SCPDURL>
                        </service>
                    </serviceList>
                </device>
            </root>",
            device_name = self.device.info.name,
            uuid = self.device.info.uuid
        )
        .as_bytes()
        .to_vec()
    }

    fn handle_hue_description(&self) -> Vec<u8> {
        tracing::info!("HUE description: {}", self.device.info.name);
        make_hue_description(&self.device.info).into_bytes()
    }

    fn handle_hue_api(
        &self,
        method: &Method,
        path: &str,
        request: &mut Request<'_, '_>,
    ) -> Vec<u8> {
        tracing::info!(
            "HUE request: {} {} for {}",
            method,
            path,
            self.device.info.name
        );

        match (method, path) {
            (&Method::Post, "/api") | (&Method::Post, "/api/") => {
                br#"[{"success":{"username":"rustmo"}}]"#.to_vec()
            }
            (&Method::Get, "/api") | (&Method::Get, "/api/") => self.make_hue_bridge_response(),
            (&Method::Get, path) if is_hue_username_path(path) => self.make_hue_bridge_response(),
            (&Method::Get, path) if path.ends_with("/config") => self.make_hue_config_response(),
            (&Method::Get, path) if path.ends_with("/groups") => br#"{}"#.to_vec(),
            (&Method::Get, path) if path.ends_with("/lights") => self.make_hue_lights_response(),
            (&Method::Get, path) if path.ends_with("/lights/1") => self.make_hue_light_response(),
            (&Method::Put, path) if path.ends_with("/lights/1/state") => {
                self.handle_hue_set_state(request)
            }
            _ => br#"[]"#.to_vec(),
        }
    }

    fn handle_hue_set_state(&self, request: &mut Request<'_, '_>) -> Vec<u8> {
        let mut content = String::new();
        request.read_to_string(&mut content).unwrap();
        let command = serde_json::from_str::<serde_json::Value>(&content).unwrap_or_default();
        handle_hue_set_state(&self.device, 1, command)
    }

    fn make_hue_bridge_response(&self) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "lights": {
                "1": hue_light_json(&self.device)
            },
            "config": make_hue_config_json(&self.device.info)
        }))
        .unwrap()
    }

    fn make_hue_lights_response(&self) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "1": hue_light_json(&self.device)
        }))
        .unwrap()
    }

    fn make_hue_light_response(&self) -> Vec<u8> {
        serde_json::to_vec(&hue_light_json(&self.device)).unwrap()
    }

    fn make_hue_config_response(&self) -> Vec<u8> {
        make_hue_config_response(&self.device.info)
    }

    fn handle_eventservice(&self) -> Vec<u8> {
        tracing::info!("UPNP eventservice: {}", self.device.info.name);
        let dimmer_set_argument = if self.device.supports_percent() {
            "<argument>
                <name>brightness</name>
                <relatedStateVariable>brightness</relatedStateVariable>
                <direction>in</direction>
            </argument>"
        } else {
            ""
        };
        let dimmer_state_variable = if self.device.supports_percent() {
            "<stateVariable sendEvents='yes'>
                <name>brightness</name>
                <dataType>ui1</dataType>
                <defaultValue>0</defaultValue>
                <allowedValueRange>
                    <minimum>0</minimum>
                    <maximum>100</maximum>
                    <step>1</step>
                </allowedValueRange>
            </stateVariable>"
        } else {
            ""
        };

        format!(
            "<scpd xmlns='urn:Belkin:service-1-0'>
            <actionList>
                <action>
                    <name>SetBinaryState</name>
                    <argumentList>
                        <argument>
                            <retval/>
                            <name>BinaryState</name>
                            <relatedStateVariable>BinaryState</relatedStateVariable>
                            <direction>in</direction>
                        </argument>
                        {dimmer_set_argument}
                    </argumentList>
                </action>
                <action>
                    <name>GetBinaryState</name>
                    <argumentList>
                        <argument>
                            <retval/>
                            <name>BinaryState</name>
                            <relatedStateVariable>BinaryState</relatedStateVariable>
                            <direction>out</direction>
                        </argument>
                    </argumentList>
                </action>
            </actionList>
            <serviceStateTable>
                <stateVariable sendEvents='yes'>
                    <name>BinaryState</name>
                    <dataType>Boolean</dataType>
                    <defaultValue>0</defaultValue>
                </stateVariable>
                <stateVariable sendEvents='yes'>
                    <name>level</name>
                    <dataType>string</dataType>
                    <defaultValue>0</defaultValue>
                </stateVariable>
                {dimmer_state_variable}
            </serviceStateTable>
        </scpd>",
            dimmer_set_argument = dimmer_set_argument,
            dimmer_state_variable = dimmer_state_variable
        )
        .into_bytes()
    }

    fn handle_metainfoservice(&self) -> Vec<u8> {
        tracing::info!("UPNP meta info service: {}", self.device.info.name);
        "<scpd xmlns='urn:Belkin:service-1-0'>
            <specVersion>
                <major>1</major>
                <minor>0</minor>
            </specVersion>
            <actionList>
                <action>
                    <name>GetMetaInfo</name>
                    <argumentList>
                        <retval/>
                        <name>GetMetaInfo</name>
                        <relatedStateVariable>MetaInfo</relatedStateVariable>
                        <direction>in</direction>
                    </argumentList>
                </action>
            </actionList>
            <serviceStateTable>
                <stateVariable sendEvents='yes'>
                    <name>MetaInfo</name>
                    <dataType>string</dataType>
                    <defaultValue>0</defaultValue>
                </stateVariable>
            </serviceStateTable>
        </scpd>"
            .to_string()
            .into_bytes()
    }
}

fn make_hue_description(info: &RustmoDeviceInfo) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>
        <root xmlns=\"urn:schemas-upnp-org:device-1-0\">
            <specVersion>
                <major>1</major>
                <minor>0</minor>
            </specVersion>
            <URLBase>http://{ip}:{port}/</URLBase>
            <device>
                <deviceType>urn:schemas-upnp-org:device:Basic:1</deviceType>
                <friendlyName>Philips hue ({ip})</friendlyName>
                <manufacturer>Royal Philips Electronics</manufacturer>
                <manufacturerURL>http://www.philips.com</manufacturerURL>
                <modelDescription>Philips hue Personal Wireless Lighting</modelDescription>
                <modelName>Philips hue bridge 2012</modelName>
                <modelNumber>929000226503</modelNumber>
                <modelURL>http://www.meethue.com</modelURL>
                <serialNumber>{serial}</serialNumber>
                <UDN>uuid:{uuid}</UDN>
                <serviceList>
                    <service>
                        <serviceType>(null)</serviceType>
                        <serviceId>(null)</serviceId>
                        <controlURL>(null)</controlURL>
                        <eventSubURL>(null)</eventSubURL>
                        <SCPDURL>(null)</SCPDURL>
                    </service>
                </serviceList>
                <presentationURL>index.html</presentationURL>
            </device>
        </root>",
        ip = info.ip_address,
        port = info.port,
        serial = hue_serial(info),
        uuid = info.uuid
    )
}

fn make_hue_config_response(info: &RustmoDeviceInfo) -> Vec<u8> {
    serde_json::to_vec(&make_hue_config_json(info)).unwrap()
}

fn make_hue_config_json(info: &RustmoDeviceInfo) -> serde_json::Value {
    serde_json::json!({
        "name": "Philips hue",
        "bridgeid": hue_serial(info),
        "mac": hue_mac(info),
        "dhcp": true,
        "ipaddress": info.ip_address.to_string(),
        "netmask": "255.255.255.0",
        "gateway": "0.0.0.0",
        "swversion": "01041302",
        "apiversion": "1.16.0",
        "linkbutton": true,
        "portalservices": false
    })
}

fn hue_light_json(device: &RustmoDevice) -> serde_json::Value {
    let percent = device.check_percent().unwrap_or(None).unwrap_or(0);
    let on = device.check_is_on().unwrap_or(VirtualDeviceState::Off) == VirtualDeviceState::On;
    serde_json::json!({
        "state": {
            "on": on,
            "bri": percent_to_hue_brightness(percent),
            "reachable": true,
            "mode": "homeautomation"
        },
        "type": "Dimmable light",
        "name": device.info.name,
        "modelid": "HASS123",
        "manufacturername": "Home Assistant",
        "uniqueid": hue_light_unique_id(&device.info),
        "swversion": "123"
    })
}

fn handle_hue_set_state(device: &RustmoDevice, id: usize, command: serde_json::Value) -> Vec<u8> {
    let mut responses = Vec::new();

    if let Some(on) = command.get("on").and_then(|value| value.as_bool()) {
        tracing::info!("HUE set `{}` on={}", device.info.name, on);
        let result = if on {
            device.turn_on()
        } else {
            device.turn_off()
        };
        if let Err(error) = result {
            tracing::error!("Problem with {}: {}", device.info.name, error.0);
        }
        responses.push(serde_json::json!({"success": {format!("/lights/{id}/state/on"): on}}));
    }

    if let Some(bri) = command.get("bri").and_then(|value| value.as_u64()) {
        let brightness = bri.min(254) as u8;
        let percent = hue_brightness_to_percent(brightness);
        tracing::info!(
            "HUE set `{}` bri={} percent={}",
            device.info.name,
            brightness,
            percent
        );
        if let Err(error) = device.set_percent(percent) {
            tracing::error!("Problem with {}: {}", device.info.name, error.0);
        }
        responses
            .push(serde_json::json!({"success": {format!("/lights/{id}/state/bri"): brightness}}));
    }

    serde_json::to_vec(&responses).unwrap()
}

fn hue_light_id_from_path(path: &str) -> Option<usize> {
    let lights = path.split("/lights/").nth(1)?;
    lights.split('/').next()?.parse().ok()
}

fn is_hue_username_path(path: &str) -> bool {
    let Some(username) = path.strip_prefix("/api/") else {
        return false;
    };

    !username.is_empty() && !username.contains('/')
}

fn percent_devices(devices: &[RustmoDevice]) -> Vec<(usize, &RustmoDevice)> {
    devices
        .iter()
        .filter(|device| device.supports_percent())
        .enumerate()
        .map(|(index, device)| (index + 1, device))
        .collect()
}

fn hue_serial(info: &RustmoDeviceInfo) -> String {
    if info.name == "Rustmo Hue Bridge" {
        "001788FFFE23BFC2".to_string()
    } else {
        info.uuid
            .simple()
            .to_string()
            .chars()
            .take(16)
            .collect::<String>()
            .to_uppercase()
    }
}

fn hue_mac(info: &RustmoDeviceInfo) -> String {
    let serial = hue_serial(info);
    serial
        .as_bytes()
        .chunks(2)
        .take(6)
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or("00"))
        .collect::<Vec<_>>()
        .join(":")
        .to_lowercase()
}

fn hue_light_unique_id(info: &RustmoDeviceInfo) -> String {
    let id = info.uuid.simple().to_string();
    format!(
        "00:{}:{}:{}:{}:{}:{}:{}-{}",
        &id[0..2],
        &id[2..4],
        &id[4..6],
        &id[6..8],
        &id[8..10],
        &id[10..12],
        &id[12..14],
        &id[14..16]
    )
}

fn percent_to_hue_brightness(percent: u8) -> u8 {
    (((percent.min(100) as f32 / 100.0) * 254.0).round() as u8).max(1)
}

fn hue_brightness_to_percent(brightness: u8) -> u8 {
    ((brightness.min(254) as f32 / 254.0) * 100.0).round() as u8
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::*;
    use crate::virtual_device::VirtualDevice;
    use uuid::Uuid;

    struct TestDevice;

    impl VirtualDevice for TestDevice {
        fn turn_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            Ok(VirtualDeviceState::On)
        }

        fn turn_off(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            Ok(VirtualDeviceState::Off)
        }

        fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            Ok(VirtualDeviceState::Off)
        }

        fn supports_percent(&self) -> bool {
            true
        }
    }

    #[test]
    fn binary_state_reads_wemo_brightness_percent() {
        let xml =
            "<BinaryState><BinaryState>1</BinaryState><brightness>90</brightness></BinaryState>";
        let state: BinaryState = from_reader(xml.as_bytes()).unwrap();

        assert_eq!(state.binary_state, Some(1));
        assert_eq!(state.percent(), Some(90));
    }

    #[test]
    fn binary_state_clamps_percent_at_one_hundred() {
        let xml =
            "<BinaryState><BinaryState>1</BinaryState><Brightness>255</Brightness></BinaryState>";
        let state: BinaryState = from_reader(xml.as_bytes()).unwrap();

        assert_eq!(state.percent(), Some(100));
    }

    #[test]
    fn basicevent_response_can_report_brightness() {
        let response = DeviceHttpServerHandler::make_basicevent_response(
            VirtualDeviceState::On,
            "Get",
            Some(90),
        );
        let response = String::from_utf8(response).unwrap();

        assert!(response.contains("<BinaryState>1</BinaryState>"));
        assert!(response.contains("<brightness>90</brightness>"));
    }

    #[test]
    fn percent_device_setup_keeps_belkin_switch_identity() {
        let handler = DeviceHttpServerHandler::new(RustmoDevice {
            info: crate::RustmoDeviceInfo {
                name: "Sconces".to_string(),
                ip_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: 1100,
                uuid: Uuid::nil(),
            },
            device: Box::new(TestDevice),
        });

        let setup = String::from_utf8(handler.handle_setup()).unwrap();

        assert!(setup.contains("<deviceType>urn:Belkin:device:controllee:1</deviceType>"));
        assert!(setup.contains("<modelName>Socket</modelName>"));
        assert!(!setup.contains("urn:Belkin:device:dimmer:1"));
    }

    #[test]
    fn hue_brightness_maps_to_percent() {
        assert_eq!(hue_brightness_to_percent(0), 0);
        assert_eq!(hue_brightness_to_percent(1), 0);
        assert_eq!(hue_brightness_to_percent(64), 25);
        assert_eq!(hue_brightness_to_percent(127), 50);
        assert_eq!(hue_brightness_to_percent(254), 100);
    }

    #[test]
    fn percent_maps_to_hue_brightness() {
        assert_eq!(percent_to_hue_brightness(0), 1);
        assert_eq!(percent_to_hue_brightness(1), 3);
        assert_eq!(percent_to_hue_brightness(25), 64);
        assert_eq!(percent_to_hue_brightness(50), 127);
        assert_eq!(percent_to_hue_brightness(100), 254);
    }
}
