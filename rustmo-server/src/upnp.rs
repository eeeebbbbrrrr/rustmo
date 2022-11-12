use std::borrow::BorrowMut;
use std::io::Read;

use hyper::server::{Fresh, Handler, Request, Response};
use regex::Regex;
use serde_xml_rs::from_reader;

use crate::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};
use crate::RustmoDevice;

#[derive(Debug, Deserialize)]
pub(crate) struct BinaryState {
    #[serde(rename = "BinaryState")]
    pub(crate) binary_state: u8,
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

impl Handler for DeviceHttpServerHandler {
    fn handle<'r, 'k>(&'r self, mut request: Request<'r, 'k>, mut response: Response<'r, Fresh>) {
        eprintln!(
            "REQUEST: http://{}:{}{} from {}",
            self.device.info.ip_address.to_string(),
            self.device.info.port,
            request.uri.to_string(),
            request.remote_addr.to_string()
        );
        let body = match request.uri.to_string().as_str() {
            "/setup.xml" => Some(self.handle_setup()),
            "/eventservice.xml" => Some(self.handle_eventservice()),
            "/metainfoservice.xml" => Some(self.handle_metainfoservice()),
            "/upnp/control/basicevent1" => Some(self.handle_basicevent(request.borrow_mut())),
            _ => {
                eprintln!("Unrecognized request: {:?}", request.uri);
                *response.status_mut() = hyper::status::StatusCode::NotFound;
                None
            }
        };

        if let Some(data) = body {
            *response.status_mut() = hyper::status::StatusCode::Ok;
            response
                .headers_mut()
                .append_raw("CONTENT-TYPE", "text/xml".to_string().into_bytes());
            response.send(data.as_slice()).unwrap();
        }
    }
}

impl DeviceHttpServerHandler {
    pub(crate) fn new(device: RustmoDevice) -> Self {
        DeviceHttpServerHandler { device }
    }

    fn handle_basicevent<'r, 'b>(&self, request: &mut Request<'r, 'b>) -> Vec<u8> {
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

        let envelope: UpnpEnvelope = from_reader(content.as_bytes()).unwrap();
        let get_or_set;
        let on_off = match action {
            "GetBinaryState" => {
                eprintln!(
                    "GET_BINARY_STATE: {} by {}",
                    self.device.info.name,
                    request.remote_addr.ip().to_string()
                );

                get_or_set = "Get";
                self.device.check_is_on()
            }
            "SetBinaryState" => {
                get_or_set = "Set";
                match envelope.body.set_binary_state {
                    Some(state) => {
                        if state.binary_state == 1 {
                            eprintln!(
                                "TURN_ON: {} by {}",
                                self.device.info.name,
                                request.remote_addr.ip().to_string()
                            );
                            self.device.turn_on()
                        } else {
                            eprintln!(
                                "TURN_OFF: {} by {}",
                                self.device.info.name,
                                request.remote_addr.ip().to_string()
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
                eprintln!(
                    "ERROR:  Unknown capture value: /{}/ from /{}/",
                    capture, action
                );
                return vec![];
            }
        };

        match on_off {
            Ok(state) => DeviceHttpServerHandler::make_basicevent_response(state, get_or_set),
            Err(e) => {
                eprintln!("ERROR:  Problem with {}: {}", self.device.info.name, e.0);
                return vec![];
            }
        }
    }

    fn make_basicevent_response(state: VirtualDeviceState, get_or_set: &str) -> Vec<u8> {
        let soap = format!(
            "<s:Envelope xmlns:s='http://schemas.xmlsoap.org/soap/envelope/'
                        s:encodingStyle='http://schemas.xmlsoap.org/soap/encoding/'>
                <s:Body>
                    <u:{action}BinaryStateResponse xmlns:u='urn:Belkin:service:basicevent:1'>
                        <BinaryState>{state}</BinaryState>
                    </u:{action}BinaryStateResponse>
                </s:Body>
            </s:Envelope>",
            action = get_or_set,
            state = match state {
                VirtualDeviceState::On => 1,
                VirtualDeviceState::Off => 0,
            }
        );

        soap.as_bytes().to_vec()
    }

    fn handle_setup(&self) -> Vec<u8> {
        eprintln!("SETUP: {}", self.device.info.name);
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

    fn handle_eventservice(&self) -> Vec<u8> {
        eprintln!("EVENTSERVICE: {}", self.device.info.name);
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
            </serviceStateTable>
        </scpd>"
            .to_string()
            .into_bytes()
    }

    fn handle_metainfoservice(&self) -> Vec<u8> {
        eprintln!("NETAINFO: {}", self.device.info.name);
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
