use std::{collections::BTreeMap, fmt, io::ErrorKind, net::IpAddr, sync::Arc};

use atvrs::{
    AtvError, BlockingAppleTvSession, ClientOptions, DeviceCredentials, DeviceState, PowerState,
    Protocol,
};
use parking_lot::Mutex;
use rustmo_server::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    Stored,
    Transient,
    Missing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProtocolAuth {
    Stored(String),
    Transient,
    Missing,
}

impl ProtocolAuth {
    pub fn stored(credentials: impl Into<String>) -> Self {
        Self::Stored(credentials.into())
    }

    pub fn transient() -> Self {
        Self::Transient
    }

    pub fn missing() -> Self {
        Self::Missing
    }

    fn from_export(
        label: &str,
        credentials: Option<String>,
        mode: Option<AuthMode>,
    ) -> Result<Self, VirtualDeviceError> {
        match (non_empty(credentials), mode) {
            (Some(credentials), None | Some(AuthMode::Stored)) => Ok(Self::Stored(credentials)),
            (Some(_), Some(AuthMode::Transient | AuthMode::Missing)) => Err(Self::invalid_export(
                label,
                "auth mode conflicts with stored credentials",
            )),
            (None, Some(AuthMode::Transient)) => Ok(Self::Transient),
            (None, Some(AuthMode::Missing) | None) => Ok(Self::Missing),
            (None, Some(AuthMode::Stored)) => Err(Self::invalid_export(
                label,
                "stored auth is missing credentials",
            )),
        }
    }

    fn apply_to(&self, credentials: &mut DeviceCredentials, protocol: Protocol) {
        if let Self::Stored(value) = self {
            credentials.set(protocol, value.clone());
        }
    }

    fn debug_label(&self) -> &'static str {
        match self {
            Self::Stored(_) => "<redacted>",
            Self::Transient => "<transient>",
            Self::Missing => "<missing>",
        }
    }

    fn invalid_export(label: &str, reason: &str) -> VirtualDeviceError {
        VirtualDeviceError::from(format!("appletv: invalid {label} auth: {reason}"))
    }
}

#[derive(Debug, Deserialize)]
pub struct RustmoAppleTvConfig {
    pub id: String,
    pub ip: IpAddr,
    pub airplay_creds: Option<String>,
    pub airplay_auth: Option<AuthMode>,
    pub companion_creds: Option<String>,
    pub raop_creds: Option<String>,
    pub raop_auth: Option<AuthMode>,
}

#[derive(Clone)]
pub struct Device {
    id: String,
    ip: IpAddr,
    raop_auth: ProtocolAuth,
    airplay_auth: ProtocolAuth,
    companion_creds: String,
    session: Arc<Mutex<Option<BlockingAppleTvSession>>>,
}

impl fmt::Debug for Device {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Device")
            .field("id", &self.id)
            .field("ip", &self.ip)
            .field("raop_auth", &self.raop_auth.debug_label())
            .field("airplay_auth", &self.airplay_auth.debug_label())
            .field("companion_creds", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl Device {
    pub fn new(
        id: impl Into<String>,
        ip: IpAddr,
        raop_creds: impl Into<String>,
        airplay_creds: impl Into<String>,
        companion_creds: impl Into<String>,
    ) -> Self {
        Self::with_auth(
            id,
            ip,
            ProtocolAuth::stored(raop_creds),
            ProtocolAuth::stored(airplay_creds),
            companion_creds,
        )
    }

    pub fn with_auth(
        id: impl Into<String>,
        ip: IpAddr,
        raop_auth: ProtocolAuth,
        airplay_auth: ProtocolAuth,
        companion_creds: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            ip,
            raop_auth,
            airplay_auth,
            companion_creds: companion_creds.into(),
            session: Arc::new(Mutex::new(None)),
        }
    }

    pub fn from_rustmo_config(config: RustmoAppleTvConfig) -> Result<Self, VirtualDeviceError> {
        let companion_creds = non_empty(config.companion_creds)
            .ok_or_else(|| VirtualDeviceError::from("appletv: missing companion credentials"))?;
        let airplay_auth =
            ProtocolAuth::from_export("airplay", config.airplay_creds, config.airplay_auth)?;
        let raop_auth = ProtocolAuth::from_export("raop", config.raop_creds, config.raop_auth)?;

        Ok(Self::with_auth(
            config.id,
            config.ip,
            raop_auth,
            airplay_auth,
            companion_creds,
        ))
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
        action: impl Fn(&BlockingAppleTvSession) -> Result<T, AtvError>,
    ) -> Result<T, VirtualDeviceError> {
        let mut session = self.session.lock();
        if session.is_none() {
            *session = Some(self.connect()?);
        }

        let result = action(session.as_ref().expect("session was initialized"));
        if let Err(error) = &result {
            if Self::should_reconnect(error) {
                tracing::warn!("reconnecting Apple TV after transport error: {error}");
                *session = Some(self.connect()?);
                return action(session.as_ref().expect("session was reinitialized"))
                    .map_err(Self::atv_error);
            }
        }

        result.map_err(Self::atv_error)
    }

    fn connect(&self) -> Result<BlockingAppleTvSession, VirtualDeviceError> {
        BlockingAppleTvSession::connect_host_by_id(
            self.ip,
            self.id.clone(),
            self.credentials(),
            ClientOptions::default(),
        )
        .map_err(Self::atv_error)
    }

    fn should_reconnect(error: &AtvError) -> bool {
        match error {
            AtvError::Io(error) => matches!(
                error.kind(),
                ErrorKind::BrokenPipe
                    | ErrorKind::ConnectionAborted
                    | ErrorKind::ConnectionReset
                    | ErrorKind::NotConnected
                    | ErrorKind::TimedOut
                    | ErrorKind::UnexpectedEof
            ),
            AtvError::Timeout => true,
            _ => false,
        }
    }

    fn atv_error(error: AtvError) -> VirtualDeviceError {
        VirtualDeviceError::from(format!("appletv: {error}"))
    }

    fn credentials(&self) -> DeviceCredentials {
        let mut credentials = DeviceCredentials::new().with_companion(self.companion_creds.clone());
        self.airplay_auth
            .apply_to(&mut credentials, Protocol::AirPlay);
        self.raop_auth.apply_to(&mut credentials, Protocol::Raop);
        credentials
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_constructor_populates_all_credentials() {
        let device = Device::new(
            "appletv-id",
            "127.0.0.1".parse().unwrap(),
            "raop-creds",
            "airplay-creds",
            "companion-creds",
        );

        let credentials = device.credentials();
        assert_eq!(Some("airplay-creds"), credentials.airplay.as_deref());
        assert_eq!(Some("companion-creds"), credentials.companion.as_deref());
        assert_eq!(Some("raop-creds"), credentials.raop.as_deref());
    }

    #[test]
    fn rustmo_export_with_transient_auth_only_stores_companion_credentials() {
        let config: RustmoAppleTvConfig = serde_json::from_str(
            r#"{
                "id": "airplay-id",
                "ip": "127.0.0.1",
                "airplay_creds": null,
                "airplay_auth": "transient",
                "companion_creds": "companion-creds",
                "raop_creds": null,
                "raop_auth": "transient"
            }"#,
        )
        .unwrap();

        let device = Device::from_rustmo_config(config).unwrap();
        let credentials = device.credentials();
        assert_eq!(None, credentials.airplay.as_deref());
        assert_eq!(Some("companion-creds"), credentials.companion.as_deref());
        assert_eq!(None, credentials.raop.as_deref());
    }

    #[test]
    fn reconnects_after_transport_errors() {
        let error = AtvError::Io(std::io::Error::from(ErrorKind::BrokenPipe));
        assert!(Device::should_reconnect(&error));

        let error = AtvError::Io(std::io::Error::from(ErrorKind::UnexpectedEof));
        assert!(Device::should_reconnect(&error));

        assert!(Device::should_reconnect(&AtvError::Timeout));
    }

    #[test]
    fn does_not_reconnect_after_auth_errors() {
        let error = AtvError::Authentication("bad credentials".to_string());
        assert!(!Device::should_reconnect(&error));

        let error = AtvError::InvalidCredentials("bad credentials".to_string());
        assert!(!Device::should_reconnect(&error));
    }

    #[test]
    fn rustmo_export_rejects_conflicting_transient_and_stored_auth() {
        let config: RustmoAppleTvConfig = serde_json::from_str(
            r#"{
                "id": "airplay-id",
                "ip": "127.0.0.1",
                "airplay_creds": "airplay-creds",
                "airplay_auth": "transient",
                "companion_creds": "companion-creds",
                "raop_creds": null,
                "raop_auth": "transient"
            }"#,
        )
        .unwrap();

        let error = Device::from_rustmo_config(config).unwrap_err();
        assert!(error
            .to_string()
            .contains("invalid airplay auth: auth mode conflicts"));
    }
}
