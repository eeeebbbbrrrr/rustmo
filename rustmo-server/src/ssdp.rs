use std::net::Ipv4Addr;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;

use net2::unix::UnixUdpBuilderExt;

use crate::RustmoDevice;

pub(crate) struct SsdpListener();

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
    pub(crate) fn listen(interface: Ipv4Addr, devices: Arc<Mutex<Vec<RustmoDevice>>>) -> Self {
        thread::spawn(move || {
            let mut buf = [0; 65535];
            let socket = net2::UdpBuilder::new_v4().unwrap()
                .reuse_address(true).unwrap()
                .reuse_port(true).unwrap()
                .bind("0.0.0.0:1900").unwrap();
            socket
                .join_multicast_v4(&Ipv4Addr::from_str("239.255.255.250").unwrap(), &interface)
                .unwrap();

            loop {
                let (len, src) = socket
                    .recv_from(&mut buf)
                    .expect("problem receiving data while listening");
                let dgram = String::from_utf8_lossy(&buf[..len]).to_string();

                if SsdpListener::is_discovery_request(dgram) {
                    // someone wants to know what devices we have
                    for device in devices.lock().unwrap().iter() {
                        println!("DISCOVERED: {} by {}", device.name, src.ip());
                        socket
                            .send_to(
                                SsdpListener::build_discovery_response(device).as_bytes(),
                                src,
                            )
                            .unwrap();
                    }
                }
            }
        });

        SsdpListener()
    }

    fn build_discovery_response(device: &RustmoDevice) -> String {
        let mut response = String::new();
        response.push_str("HTTP/1.1 200 OK\r\n");
        response.push_str("CACHE-CONTROL: max-age=86400\r\n");
        response.push_str("DATE: Sat, 26 Nov 2016 04:56:29 GMT\r\n");
        response.push_str("EXT:\r\n");
        response.push_str(
            format!(
                "LOCATION: http://{}:{}/setup.xml\r\n",
                device.ip_address.to_string(),
                device.port
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

    fn is_discovery_request(dgram: String) -> bool {
        let dgram = dgram.to_lowercase();

        // NOTE:  make sure these patterns are all lowercase
        dgram.contains("man: \"ssdp:discover\"")
            && (dgram.contains("st: urn:belkin:device:**")
                || dgram.contains("st: upnp:rootdevice")
                || dgram.contains("st: ssdp:all"))
    }

    pub(crate) fn stop(&self) {
        // TODO:  how to stop the socket?
    }
}

impl Drop for SsdpListener {
    fn drop(&mut self) {
        self.stop()
    }
}
