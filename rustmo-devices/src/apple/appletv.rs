use std::{collections::BTreeMap, fmt, net::IpAddr, sync::Arc};

use atvrs::{
    AtvError, BlockingAppleTvSession, ClientOptions, DeviceCredentials, DeviceState, PowerState,
};
use parking_lot::Mutex;
use rustmo_server::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};

#[derive(Clone)]
pub struct Device {
    id: String,
    ip: IpAddr,
    raop_creds: String,
    airplay_creds: String,
    companion_creds: String,
    session: Arc<Mutex<Option<BlockingAppleTvSession>>>,
}

impl fmt::Debug for Device {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Device")
            .field("id", &self.id)
            .field("ip", &self.ip)
            .field("raop_creds", &"<redacted>")
            .field("airplay_creds", &"<redacted>")
            .field("companion_creds", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl Device {
    pub fn new<S: Into<String>>(
        id: S,
        ip: IpAddr,
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
            session: Arc::new(Mutex::new(None)),
        }
    }

    pub fn power_status(&self) -> Result<bool, VirtualDeviceError> {
        self.with_session(|session| session.power_state().map(|state| state == PowerState::On))
    }

    pub fn power_on(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::turn_on)
    }

    pub fn power_off(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::turn_off)
    }

    pub fn launch_app(&self, bundle_id: &str) -> Result<(), VirtualDeviceError> {
        self.with_session(|session| session.launch_app(bundle_id))
    }

    pub fn open_url(&self, url: &str) -> Result<(), VirtualDeviceError> {
        self.with_session(|session| session.open_url(url))
    }

    pub fn current_app(&self) -> Result<Option<(String, String)>, VirtualDeviceError> {
        self.with_session(|session| {
            session
                .current_app()
                .map(|app| app.and_then(atvrs::App::into_identifier_and_name))
        })
    }

    pub fn app_list(&self) -> Result<impl Iterator<Item = (String, String)>, VirtualDeviceError> {
        let apps: Vec<_> = self.with_session(|session| {
            session.app_list().map(|apps| {
                apps.into_iter()
                    .filter_map(atvrs::App::into_identifier_and_name)
                    .collect()
            })
        })?;
        Ok(apps.into_iter())
    }

    pub fn up(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::up)
    }

    pub fn down(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::down)
    }

    pub fn left(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::left)
    }

    pub fn right(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::right)
    }

    pub fn channel_down(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::channel_down)
    }

    pub fn channel_up(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::channel_up)
    }

    pub fn home(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::home)
    }

    pub fn home_hold(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::home_hold)
    }

    pub fn menu(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::menu)
    }

    pub fn top_menu(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::top_menu)
    }

    pub fn next(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::next)
    }

    pub fn previous(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::previous)
    }

    pub fn play(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::play)
    }

    pub fn pause(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::pause)
    }

    pub fn stop(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::stop)
    }

    pub fn select(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::select)
    }

    pub fn skip_backward(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::skip_backward)
    }

    pub fn skip_forward(&self) -> Result<(), VirtualDeviceError> {
        self.with_session(BlockingAppleTvSession::skip_forward)
    }

    pub fn playing(&self) -> Result<BTreeMap<String, String>, VirtualDeviceError> {
        self.with_session(|session| session.playing().map(|playing| playing.pyatv_fields()))
    }

    pub fn paused(&self) -> Result<bool, VirtualDeviceError> {
        self.with_session(|session| {
            session
                .device_state()
                .map(|state| state == DeviceState::Paused)
        })
    }

    fn with_session<T>(
        &self,
        action: impl FnOnce(&BlockingAppleTvSession) -> Result<T, AtvError>,
    ) -> Result<T, VirtualDeviceError> {
        let mut session = self.session.lock();
        if session.is_none() {
            *session = Some(
                BlockingAppleTvSession::connect_host_by_id(
                    self.ip,
                    self.id.clone(),
                    self.credentials(),
                    ClientOptions::default(),
                )
                .map_err(Self::atv_error)?,
            );
        }
        action(session.as_ref().expect("session was initialized")).map_err(Self::atv_error)
    }

    fn atv_error(error: AtvError) -> VirtualDeviceError {
        VirtualDeviceError::from(format!("appletv: {error}"))
    }

    fn credentials(&self) -> DeviceCredentials {
        DeviceCredentials::new()
            .with_airplay(self.airplay_creds.clone())
            .with_companion(self.companion_creds.clone())
            .with_raop(self.raop_creds.clone())
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
