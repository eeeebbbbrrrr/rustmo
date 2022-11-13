extern crate rustmo_server;
#[macro_use]
extern crate serde_derive;

pub mod anthem;
pub mod apple;
pub mod kaleidescape;
pub mod lutron;
pub mod madvr;
pub mod oppo;
pub mod sony;

pub mod devices {
    pub use crate::anthem::avm70::Device as Avm70;
    pub use crate::apple::appletv::Device as AppleTV;
    pub use crate::kaleidescape::kscp::Device as Kaleidescape;
    pub use crate::lutron::ra2::Device as Ra2;
    pub use crate::lutron::ra2::Ra2MainRepeater;
    pub use crate::madvr::envy::Device as Envy;
    pub use crate::oppo::dvd_players::udp_203::Device as Udp203;
    pub use crate::rustmo_server::virtual_device::SynchronizedDevice;
    pub use crate::rustmo_server::virtual_device::VirtualDevice;
    pub use crate::rustmo_server::virtual_device::VirtualDeviceError;
    pub use crate::rustmo_server::virtual_device::VirtualDeviceState;
    pub use crate::sony::projectors::pj_talk::Device as PjTalk;
    pub use crate::sony::receivers::str_za5000_es::Device as StrZa500ES;
}
