use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;
use std::thread;

use net2::unix::UnixUdpBuilderExt;

use crate::{RustmoDevice, RustmoDeviceInfo, VirtualDevicesList};

#[derive(Clone)]
pub(crate) struct SsdpListener;

///
/// `SsdpListener` joins a IPV4 multicast on `239.255.255.250` (as perscribed by the SSDP protocol spec)
/// and listens to the specified interface on port `1900`
///
impl SsdpListener {
    ///
    /// Begin listening on the the specified interface for SSDP discovery requests
    /// and respond with the list of devices.
    ///
    /// `devices` is guarded by a Mutex so that users of this listener can add/remove devices
    /// while we're listening
    ///
    pub(crate) fn listen(
        interface: IpAddr,
        devices: VirtualDevicesList,
        hue_bridge: Option<RustmoDeviceInfo>,
    ) -> Self {
        thread::spawn(move || {
            let mut buf = [0; 65535];
            let ip = if let IpAddr::V4(ip) = interface {
                ip
            } else {
                panic!("IPv4 is required")
            };
            let socket = net2::UdpBuilder::new_v4()
                .unwrap()
                .reuse_address(true)
                .unwrap()
                .reuse_port(true)
                .unwrap()
                .bind("0.0.0.0:1900")
                .unwrap();
            socket
                .join_multicast_v4(&Ipv4Addr::from_str("239.255.255.250").unwrap(), &ip)
                .unwrap();

            loop {
                let (len, src) = socket
                    .recv_from(&mut buf)
                    .expect("problem receiving data while listening");
                let dgram = String::from_utf8_lossy(&buf[..len]).to_string();

                // tracing::info!("SSDP discovery from {}:{}", src.ip(), src.port());
                if let Some(search_target) = SsdpListener::discovery_search_target(&dgram) {
                    // someone wants to know what devices we have
                    let devices = devices.read();
                    let responses = SsdpListener::build_discovery_responses(
                        &devices,
                        hue_bridge.as_ref(),
                        search_target,
                    );
                    let hue_bridge_available = hue_bridge.is_some();
                    tracing::info!(
                        "SSDP discovery from {} target={:?} devices={} hue_bridge={} responses={}",
                        src,
                        search_target,
                        devices.len(),
                        hue_bridge_available,
                        responses.len()
                    );
                    for response in responses {
                        socket.send_to(response.as_bytes(), src).unwrap();
                    }
                }
            }
        });

        SsdpListener
    }

    fn build_discovery_responses(
        devices: &[RustmoDevice],
        hue_bridge: Option<&RustmoDeviceInfo>,
        search_target: DiscoverySearchTarget,
    ) -> Vec<String> {
        let mut responses = Vec::new();

        if matches!(
            search_target,
            DiscoverySearchTarget::HueBasic
                | DiscoverySearchTarget::HueRootDevice
                | DiscoverySearchTarget::All
        ) && devices.iter().any(|device| device.supports_percent())
        {
            if let Some(hue_bridge) = hue_bridge {
                responses.extend(SsdpListener::build_hue_discovery_responses(
                    hue_bridge,
                    search_target,
                ));
            }
        }

        if matches!(
            search_target,
            DiscoverySearchTarget::Belkin | DiscoverySearchTarget::All
        ) {
            responses.extend(
                devices
                    .iter()
                    .filter(|device| !device.supports_percent())
                    .map(|device| SsdpListener::build_belkin_discovery_response(&device.info)),
            );
        }

        responses
    }

    fn build_belkin_discovery_response(device: &RustmoDeviceInfo) -> String {
        let mut response = String::new();
        response.push_str("HTTP/1.1 200 OK\r\n");
        response.push_str("CACHE-CONTROL: max-age=86400\r\n");
        response.push_str("DATE: Sat, 26 Nov 2016 04:56:29 GMT\r\n");
        response.push_str("EXT:\r\n");
        response.push_str(
            format!(
                "LOCATION: http://{}:{}/setup.xml\r\n",
                device.ip_address, device.port
            )
            .as_str(),
        );
        response.push_str("OPT: \"http://schemas.upnp.org/upnp/1/0/\"; ns=01\r\n");
        response.push_str("01-NLS: b9200ebb-736d-4b93-bf03-835149d13983\r\n");
        response.push_str("SERVER: Theater, UPnP/1.0, Unspecified\r\n");
        response.push_str("ST: urn:Belkin:device:**\r\n");
        response.push_str(format!("USN: uuid:{}::urn:Belkin:device:**\r\n", device.uuid).as_str());
        response.push_str("\r\n");
        response
    }

    fn build_hue_discovery_responses(
        device: &RustmoDeviceInfo,
        search_target: DiscoverySearchTarget,
    ) -> Vec<String> {
        match search_target {
            DiscoverySearchTarget::HueBasic | DiscoverySearchTarget::All => {
                vec![SsdpListener::build_hue_discovery_response(
                    device,
                    "urn:schemas-upnp-org:device:basic:1",
                    format!("uuid:{}", device.uuid),
                )]
            }
            DiscoverySearchTarget::HueRootDevice => {
                vec![SsdpListener::build_hue_discovery_response(
                    device,
                    "upnp:rootdevice",
                    format!("uuid:{}::upnp:rootdevice", device.uuid),
                )]
            }
            DiscoverySearchTarget::Belkin => Vec::new(),
        }
    }

    fn build_hue_discovery_response(device: &RustmoDeviceInfo, st: &str, usn: String) -> String {
        let mut response = String::new();
        response.push_str("HTTP/1.1 200 OK\r\n");
        response.push_str("CACHE-CONTROL: max-age=60\r\n");
        response.push_str("EXT:\r\n");
        response.push_str(
            format!(
                "LOCATION: http://{}:{}/description.xml\r\n",
                device.ip_address, device.port
            )
            .as_str(),
        );
        response.push_str("SERVER: FreeRTOS/6.0.5, UPnP/1.0, IpBridge/1.16.0\r\n");
        response.push_str(format!("hue-bridgeid: {}\r\n", hue_bridge_id(device)).as_str());
        response.push_str(format!("ST: {st}\r\n").as_str());
        response.push_str(format!("USN: {usn}\r\n").as_str());
        response.push_str("\r\n");
        response
    }

    fn discovery_search_target(dgram: &str) -> Option<DiscoverySearchTarget> {
        let dgram = dgram.to_lowercase();
        // NOTE:  make sure these patterns are all lowercase
        if !dgram.contains("man: \"ssdp:discover\"") {
            return None;
        }

        if dgram.contains("st: urn:belkin:device:**") {
            Some(DiscoverySearchTarget::Belkin)
        } else if dgram.contains("st: urn:schemas-upnp-org:device:basic:1") {
            Some(DiscoverySearchTarget::HueBasic)
        } else if dgram.contains("st: upnp:rootdevice") {
            Some(DiscoverySearchTarget::HueRootDevice)
        } else if dgram.contains("st: ssdp:all") {
            Some(DiscoverySearchTarget::All)
        } else {
            None
        }
    }

    pub(crate) fn stop(&self) {
        // TODO:  how to stop the socket?
    }
}

#[derive(Clone, Copy, Debug)]
enum DiscoverySearchTarget {
    All,
    Belkin,
    HueBasic,
    HueRootDevice,
}

fn hue_bridge_id(_device: &RustmoDeviceInfo) -> String {
    "001788FFFE23BFC2".to_string()
}

impl Drop for SsdpListener {
    fn drop(&mut self) {
        self.stop()
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use uuid::Uuid;

    use super::*;
    use crate::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};

    struct BinaryDevice;

    impl VirtualDevice for BinaryDevice {
        fn turn_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            Ok(VirtualDeviceState::On)
        }

        fn turn_off(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            Ok(VirtualDeviceState::Off)
        }

        fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
            Ok(VirtualDeviceState::Off)
        }
    }

    struct PercentDevice;

    impl VirtualDevice for PercentDevice {
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

    fn rustmo_device(device: Box<dyn VirtualDevice>) -> RustmoDevice {
        RustmoDevice {
            info: RustmoDeviceInfo {
                name: "Light".to_string(),
                ip_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: 1100,
                uuid: Uuid::nil(),
            },
            device,
        }
    }

    #[test]
    fn percent_devices_do_not_answer_belkin_searches() {
        let device = rustmo_device(Box::new(PercentDevice));
        let responses = SsdpListener::build_discovery_responses(
            &[device],
            Some(&hue_bridge()),
            DiscoverySearchTarget::Belkin,
        );

        assert!(responses.is_empty());
    }

    #[test]
    fn percent_devices_answer_hue_searches() {
        let device = rustmo_device(Box::new(PercentDevice));
        let bridge = hue_bridge();
        let responses = SsdpListener::build_discovery_responses(
            &[device],
            Some(&bridge),
            DiscoverySearchTarget::HueBasic,
        );

        assert_eq!(responses.len(), 1);
        assert!(responses[0].contains("ST: urn:schemas-upnp-org:device:basic:1"));
        assert!(responses[0].contains("USN: uuid:00000000-0000-0000-0000-000000000000\r\n"));
        assert!(responses[0].contains("LOCATION: http://127.0.0.1:80/description.xml"));
        assert!(responses[0].contains("hue-bridgeid: "));
    }

    #[test]
    fn percent_devices_answer_rootdevice_searches() {
        let device = rustmo_device(Box::new(PercentDevice));
        let bridge = hue_bridge();
        let responses = SsdpListener::build_discovery_responses(
            &[device],
            Some(&bridge),
            DiscoverySearchTarget::HueRootDevice,
        );

        assert_eq!(responses.len(), 1);
        assert!(responses[0].contains("ST: upnp:rootdevice"));
        assert!(responses[0]
            .contains("USN: uuid:00000000-0000-0000-0000-000000000000::upnp:rootdevice"));
    }

    #[test]
    fn binary_devices_still_answer_belkin_searches() {
        let device = rustmo_device(Box::new(BinaryDevice));
        let responses =
            SsdpListener::build_discovery_responses(&[device], None, DiscoverySearchTarget::Belkin);

        assert_eq!(responses.len(), 1);
        assert!(responses[0].contains("ST: urn:Belkin:device:**"));
        assert!(responses[0].contains("LOCATION: http://127.0.0.1:1100/setup.xml"));
    }

    #[test]
    fn all_searches_return_one_hue_bridge_device_response_for_many_percent_devices() {
        let first = rustmo_device(Box::new(PercentDevice));
        let second = rustmo_device(Box::new(PercentDevice));
        let bridge = hue_bridge();
        let responses = SsdpListener::build_discovery_responses(
            &[first, second],
            Some(&bridge),
            DiscoverySearchTarget::All,
        );

        assert_eq!(responses.len(), 1);
        assert!(responses[0].contains("LOCATION: http://127.0.0.1:80/description.xml"));
        assert!(responses[0].contains("ST: urn:schemas-upnp-org:device:basic:1"));
    }

    fn hue_bridge() -> RustmoDeviceInfo {
        RustmoDeviceInfo {
            name: "Rustmo Hue Bridge".to_string(),
            ip_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 80,
            uuid: Uuid::nil(),
        }
    }
}
