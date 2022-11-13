use byteorder::WriteBytesExt;
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::FromPrimitive;
use rustmo_server::virtual_device::{VirtualDevice, VirtualDeviceError, VirtualDeviceState};
use scraper::{Html, Selector};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Device {
    ip: IpAddr,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Movie {
    pub id: String,
    pub title: String,
    pub coverart: String,
}

#[derive(FromPrimitive, ToPrimitive, Eq, PartialEq, Copy, Clone, Hash, Ord, PartialOrd, Debug)]
#[allow(non_camel_case_types)]
pub enum Screen {
    Unknown = 00,
    MovieList = 01,
    MovieCollections = 02,
    MovieCovers = 03,
    ParentalControl = 04,
    unused_05 = 05,
    unused_06 = 06,
    PlayingMovie = 07,
    SystemStatus = 08,
    MusicList = 09,
    MusicCovers = 10,
    MusicCollections = 11,
    MusicNowPlaying = 12,
    unused_13 = 13,
    VaultSummary = 14,
    SystemSettings = 15,
    MovieStore = 16,
    reserved = 17,
    LibrarySearchResults = 18,
}

#[derive(FromPrimitive, ToPrimitive, Eq, PartialEq, Copy, Clone, Hash, Ord, PartialOrd, Debug)]
pub enum Popup {
    None = 00,
    DetailsPage = 01,
    MovieOverlayStatusPage = 02,
    MovieOverlay = 03,
}

#[derive(FromPrimitive, ToPrimitive, Eq, PartialEq, Copy, Clone, Hash, Ord, PartialOrd, Debug)]
pub enum Dialog {
    None = 00,
    KaleidescapeMenu = 01,
    PasscodeEntry = 02,
    SimpleQuestion = 03,
    InformationalMessage = 04,
    WarningMessage = 05,
    ErrorMessage = 06,
    Preplay = 07,
    ImportWarranty = 08,
    Keyboard = 09,
    IPConfiguration = 10,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct UiState {
    pub screen: Screen,
    pub popup: Popup,
    pub dialog: Dialog,
    pub saver: bool,
}

#[allow(dead_code)]
impl Device {
    pub fn new(ip: IpAddr) -> Self {
        Self { ip }
    }

    pub fn enter_standby(&mut self) -> Result<(), VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.send_command(&mut socket, 99, 1, "ENTER_STANDBY")
            .map(|_| ())
    }

    pub fn leave_standby(&mut self) -> Result<(), VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.send_command(&mut socket, 99, 1, "LEAVE_STANDBY")
            .map(|_| ())
    }

    pub fn power_state(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        let mut socket = self.connect()?;
        let response = self.send_command(&mut socket, 99, 1, "LEAVE_STANDBY")?;
        let mut parts = response.split(':');
        let _command = parts.next();
        match parts
            .next()
            .ok_or(VirtualDeviceError::from("no power state code"))?
        {
            "1" => Ok(VirtualDeviceState::On),
            _ => Ok(VirtualDeviceState::Off),
        }
    }

    pub fn up(&mut self) -> Result<(), VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.send_command(&mut socket, 99, 1, "UP").map(|_| ())
    }

    pub fn down(&mut self) -> Result<(), VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.send_command(&mut socket, 99, 1, "DOWN").map(|_| ())
    }

    pub fn left(&mut self) -> Result<(), VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.send_command(&mut socket, 99, 1, "LEFT").map(|_| ())
    }

    pub fn right(&mut self) -> Result<(), VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.send_command(&mut socket, 99, 1, "RIGHT").map(|_| ())
    }

    pub fn select(&mut self) -> Result<(), VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.send_command(&mut socket, 99, 1, "SELECT").map(|_| ())
    }

    pub fn play(&mut self) -> Result<(), VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.send_command(&mut socket, 99, 1, "PLAY").map(|_| ())
    }

    pub fn replay(&mut self) -> Result<(), VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.send_command(&mut socket, 99, 1, "REPLAY").map(|_| ())
    }

    pub fn pause(&mut self) -> Result<(), VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.send_command(&mut socket, 99, 1, "PAUSE").map(|_| ())
    }

    pub fn stop(&mut self) -> Result<(), VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.send_command(&mut socket, 99, 1, "STOP").map(|_| ())
    }

    pub fn fast_forward(&mut self) -> Result<(), VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.send_command(&mut socket, 99, 1, "SCAN_FORWARD")
            .map(|_| ())
    }

    pub fn rewind(&mut self) -> Result<(), VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.send_command(&mut socket, 99, 1, "SCAN_REVERSE")
            .map(|_| ())
    }

    pub fn next(&mut self) -> Result<(), VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.send_command(&mut socket, 99, 1, "NEXT").map(|_| ())
    }

    pub fn previous(&mut self) -> Result<(), VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.send_command(&mut socket, 99, 1, "PREVIOUS")
            .map(|_| ())
    }

    pub fn menu(&mut self) -> Result<(), VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.send_command(&mut socket, 99, 1, "KALEIDESCAPE_MENU_TOGGLE")
            .map(|_| ())
    }

    pub fn playing_title(&self) -> Result<String, VirtualDeviceError> {
        let mut socket = self.connect()?;
        let line = self.send_command(&mut socket, 99, 1, "GET_PLAYING_TITLE_NAME")?;
        let line = line.replace("\\:", "$COLON$");
        let line = line.replace("\\/", "$SLASH$");
        let mut parts = line.split(':');
        let _command = parts.next();
        parts
            .next()
            .ok_or(VirtualDeviceError::from("no movie title"))
            .map(|s| s.to_string())
    }

    pub fn ui_state(&self) -> Result<UiState, VirtualDeviceError> {
        let mut socket = self.connect()?;
        let response = self.send_command(&mut socket, 99, 1, "GET_UI_STATE")?;
        tracing::debug!("{response}");
        let mut parts = response.split(':');
        let _command = parts.next();
        let screen = Screen::from_isize(
            parts
                .next()
                .ok_or(VirtualDeviceError::new("no Screen number"))?
                .parse()?,
        )
        .ok_or(VirtualDeviceError::new("invalid Screen number"))?;

        let popup = Popup::from_isize(
            parts
                .next()
                .ok_or(VirtualDeviceError::new("no Popup number"))?
                .parse()?,
        )
        .ok_or(VirtualDeviceError::new("invalid Popup number"))?;

        let dialog = Dialog::from_isize(
            parts
                .next()
                .ok_or(VirtualDeviceError::new("no Dialog number"))?
                .parse()?,
        )
        .ok_or(VirtualDeviceError::new("invalid Dialog number"))?;

        let saver = parts
            .next()
            .ok_or(VirtualDeviceError::new("no saver bool"))?
            == "1";

        Ok(UiState {
            screen,
            popup,
            dialog,
            saver,
        })
    }

    pub fn highlighted_section(&self) -> Result<String, VirtualDeviceError> {
        let mut socket = self.connect()?;
        let line = self.send_command(&mut socket, 99, 1, "GET_HIGHLIGHTED_SELECTION")?;
        let mut parts = line.split(':');
        let _command = parts.next();
        let movie_id = parts
            .next()
            .ok_or(VirtualDeviceError::from("no movie_id"))?;
        Ok(movie_id.to_string())
    }

    pub fn play_movie<S: AsRef<str>>(&mut self, movie_id: S) -> Result<(), VirtualDeviceError> {
        let mut socket = self.connect()?;
        let _response = self.send_command(
            &mut socket,
            99,
            1,
            format!("SHOW_CONTROLLER_DETAILS:{}:", movie_id.as_ref()),
        )?;
        let mut retries = 30;
        while retries > 0 {
            let state = self.ui_state()?;
            if state.popup == Popup::DetailsPage {
                let mut retries = 30;
                while retries > 0 {
                    let selected = self.highlighted_section()?;
                    if selected == movie_id.as_ref() {
                        return self.play();
                    }
                    retries -= 1;
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
            std::thread::sleep(Duration::from_secs(1));
            retries -= 1;
        }
        Err(VirtualDeviceError::from("Unable to play movie"))
    }

    pub fn list_movies(&self) -> Result<BTreeSet<Movie>, VirtualDeviceError> {
        let mut movies = BTreeSet::new();
        let url = format!("http://{}/movies", self.ip);
        let result = ureq::get(&url).call()?;
        let body = result.into_string()?;
        let document = Html::parse_document(&body);
        let selector = Selector::parse(r#"tr.movie_container"#).expect("bad css selector");
        let matches = document.select(&selector);
        let mut socket = self.connect()?;
        for m in matches {
            let id = m.value().attr("selection_handle").map_or(
                Err(VirtualDeviceError::new("couldn't select movie id")),
                |s| Ok(s),
            )?;
            tracing::debug!("KALEDEISCAPE MOVIE ID: {id}");
            let details = self.movie_details_internal(&mut socket, id)?;

            movies.insert(Movie {
                id: format!("26-0.{id}"),
                title: details
                    .get("Title")
                    .map_or(Err(VirtualDeviceError::new("missing Title key")), |s| Ok(s))?
                    .clone(),
                coverart: details
                    .get("HiRes_cover_URL")
                    .map_or(
                        Err(VirtualDeviceError::new("missing HiRes_cover_URL key")),
                        |s| Ok(s),
                    )?
                    .clone(),
            });
        }

        Ok(movies)
    }

    pub fn movie_details<S: AsRef<str>>(
        &self,
        movie_id: S,
    ) -> Result<BTreeMap<String, String>, VirtualDeviceError> {
        let mut socket = self.connect()?;
        self.movie_details_internal(&mut socket, movie_id)
    }

    fn movie_details_internal<S: AsRef<str>>(
        &self,
        socket: &mut TcpStream,
        movie_id: S,
    ) -> Result<BTreeMap<String, String>, VirtualDeviceError> {
        let mut movies = BTreeMap::default();
        let overview = self.send_command(
            socket,
            99,
            1,
            format!("GET_CONTENT_DETAILS:1.{}:", movie_id.as_ref()),
        )?;
        let mut parts = overview.split(':');
        let _command = parts.next();
        let many = parts
            .next()
            .ok_or(VirtualDeviceError::new(
                "no length in command overview response",
            ))?
            .parse()?;

        for line in self.read_lines(socket, many)? {
            let line = line.replace("\\:", "$COLON$");
            let line = line.replace("\\/", "$SLASH$");
            let mut parts = line.split(':');
            let _command = parts.next();
            let _num = parts.next();
            let key = parts
                .next()
                .ok_or(VirtualDeviceError::new("no key in details"))?
                .replace("$COLON$", ":")
                .replace("$SLASH$", "/");
            let value = parts
                .next()
                .ok_or(VirtualDeviceError::new("no value in details"))?
                .replace("$COLON$", ":")
                .replace("$SLASH$", "/");

            movies.insert(key, value);
        }

        Ok(movies)
    }

    fn connect(&self) -> Result<TcpStream, VirtualDeviceError> {
        let socket = TcpStream::connect(&SocketAddr::new(self.ip, 10000))?;
        socket.set_read_timeout(Some(Duration::from_millis(1000)))?;
        Ok(socket)
    }

    fn read_line(&self, socket: &mut TcpStream) -> Result<String, VirtualDeviceError> {
        let mut reader = BufReader::new(socket.try_clone()?);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let (_, line) = line
            .trim()
            .split_once(':')
            .ok_or(VirtualDeviceError::new("invalid line format"))?;
        Ok(line.to_string())
    }

    fn read_lines(
        &self,
        socket: &mut TcpStream,
        many: usize,
    ) -> Result<Vec<String>, VirtualDeviceError> {
        let mut reader = BufReader::new(socket.try_clone()?);
        let mut lines = Vec::new();
        let mut cnt = 0;
        while cnt < many {
            let mut line = String::new();
            reader.read_line(&mut line)?;
            let (_, line) = line
                .trim()
                .split_once(':')
                .map_or(Err(VirtualDeviceError::new("invalid line format")), |s| {
                    Ok(s)
                })?;
            let line = line.trim();

            lines.push(line.to_string());
            cnt += 1;
        }
        Ok(lines)
    }

    fn send_command<S: AsRef<str> + Debug>(
        &self,
        socket: &mut TcpStream,
        device_id: usize,
        seq: usize,
        command: S,
    ) -> Result<String, VirtualDeviceError> {
        let command = format!("{device_id}/{seq}/{}:", command.as_ref());
        tracing::info!("kaleidescape command: {}", command);

        socket.write_all(command.as_bytes())?;
        socket.write_u8(b'\n')?;
        socket.flush()?;
        let line = self.read_line(socket)?;
        if line.starts_with("Device is in standby") {
            Err(VirtualDeviceError::from(line))
        } else {
            Ok(line)
        }
    }
}

impl VirtualDevice for Device {
    fn turn_on(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.leave_standby().map(|_| VirtualDeviceState::On)
    }

    fn turn_off(&mut self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.enter_standby().map(|_| VirtualDeviceState::Off)
    }

    fn check_is_on(&self) -> Result<VirtualDeviceState, VirtualDeviceError> {
        self.power_state()
    }
}
