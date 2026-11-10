pub const NUS_SERVICE_UUID: &str = "6E400001-B5A3-F393-E0A9-E50E24DCCA9E";
pub const NUS_RX_UUID: &str = "6E400002-B5A3-F393-E0A9-E50E24DCCA9E";
pub const NUS_TX_UUID: &str = "6E400003-B5A3-F393-E0A9-E50E24DCCA9E";
pub const RX_BUFFER_LIMIT: usize = 512;
/// Notification payload for the default 23-byte ATT MTU; larger MTUs use `mtu - 3`.
pub const NOTIFICATION_PAYLOAD: usize = 20;
/// MTU the device asks for after connecting.
pub const PREFERRED_MTU: u16 = 247;

/// Pairing window opened by the button hold (or `BTN:PAIR`).
pub const PAIRING_WINDOW_MS: u64 = 5 * 60_000;
/// While connected, the window closes after this much inactivity.
pub const CONNECTED_IDLE_MS: u64 = 10 * 60_000;
/// After a disconnect, keep advertising briefly so bonding reconnects succeed.
pub const RECONNECT_GRACE_MS: u64 = 60_000;

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Ping,
    Status,
    Save,
    Load,
    Clear,
    Info,
    Scan,
    Read,
    Live(bool),
    OtaCheck,
    SetSsid(String),
    SetPassword(String),
    SetLatitude(f64),
    SetLongitude(f64),
    SetDeviceId(String),
    SetDeviceKey(String),
    ProtocolError(FrameError),
    /// A setter whose value can't be used; the name goes in the `ERROR:` reply.
    Invalid(&'static str),
    /// Not a command. The text isn't kept, so a mistyped secret is never
    /// logged or sent back.
    Unknown,
}

/// Splits `NAME:value` on a known setter name, in any letter case.
fn setter<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let head = line.get(..name.len())?;
    head.eq_ignore_ascii_case(name).then(|| &line[name.len()..])
}

/// A latitude or longitude: a finite number within `limit` degrees.
fn coordinate(value: &str, limit: f64) -> Option<f64> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|degrees| degrees.is_finite() && degrees.abs() <= limit)
}

pub fn parse_command(value: &str) -> Command {
    if value.eq_ignore_ascii_case("PING") {
        Command::Ping
    } else if value.eq_ignore_ascii_case("STATUS") {
        Command::Status
    } else if value.eq_ignore_ascii_case("SAVE") {
        Command::Save
    } else if value.eq_ignore_ascii_case("LOAD") {
        Command::Load
    } else if value.eq_ignore_ascii_case("CLEAR") {
        Command::Clear
    } else if value.eq_ignore_ascii_case("INFO") {
        Command::Info
    } else if value.eq_ignore_ascii_case("SCAN") {
        Command::Scan
    } else if value.eq_ignore_ascii_case("READ") {
        Command::Read
    } else if value.eq_ignore_ascii_case("LIVE:ON") {
        Command::Live(true)
    } else if value.eq_ignore_ascii_case("LIVE:OFF") {
        Command::Live(false)
    } else if value.eq_ignore_ascii_case("OTA:CHECK") {
        Command::OtaCheck
    } else if let Some(value) = setter(value, "SSID:") {
        Command::SetSsid(value.to_owned())
    } else if let Some(value) = setter(value, "PASS:") {
        Command::SetPassword(value.to_owned())
    } else if let Some(value) = setter(value, "LAT:") {
        coordinate(value, 90.0).map_or(Command::Invalid("LAT"), Command::SetLatitude)
    } else if let Some(value) = setter(value, "LON:") {
        coordinate(value, 180.0).map_or(Command::Invalid("LON"), Command::SetLongitude)
    } else if let Some(value) = setter(value, "DEVID:") {
        Command::SetDeviceId(value.to_owned())
    } else if let Some(value) = setter(value, "DEVKEY:") {
        Command::SetDeviceKey(value.to_owned())
    } else {
        Command::Unknown
    }
}

#[derive(Default)]
pub struct CommandFramer {
    buffer: Vec<u8>,
}

impl CommandFramer {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Command>, FrameError> {
        self.buffer.extend_from_slice(bytes);
        if self.buffer.len() > RX_BUFFER_LIMIT {
            self.buffer.clear();
            return Err(FrameError::TooLong);
        }
        let mut commands = Vec::new();
        while let Some(newline) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=newline).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.is_empty() {
                continue;
            }
            let value = std::str::from_utf8(&line).map_err(|_| FrameError::Utf8)?;
            commands.push(parse_command(value));
        }
        Ok(commands)
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameError {
    TooLong,
    Utf8,
}

/// Splits a newline-terminated reply into notifications that fit the negotiated MTU.
pub fn notification_chunks(message: &str, mtu: u16) -> Vec<Vec<u8>> {
    let payload = usize::from(mtu.saturating_sub(3)).max(NOTIFICATION_PAYLOAD);
    let framed = format!("{message}\n");
    framed
        .as_bytes()
        .chunks(payload)
        .map(<[u8]>::to_vec)
        .collect()
}

pub trait BleTransport {
    type Error;
    fn send_line(&mut self, message: &str) -> Result<(), Self::Error>;
}

/// Everything the BLE stack reports to the main loop.
#[derive(Clone, Debug, PartialEq)]
pub enum BleEvent {
    Command(Command),
    Connected,
    Disconnected,
    /// The phone started passkey pairing; show this passkey to the user.
    PasskeyDisplayed(u32),
    PairingFinished {
        ok: bool,
    },
}

/// What the main loop must ask the BLE stack to do after a [`PairingWindow`] change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PairingAction {
    StartAdvertising,
    StopAdvertising,
    /// Stop advertising and drop any connected phone.
    Close,
}

/// BLE is only reachable while the pairing window is open (opened by a button hold).
///
/// Pure state machine so the timing rules are host-testable.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PairingWindow {
    deadline_ms: Option<u64>,
    connected: bool,
    advertising: bool,
    passkey: Option<u32>,
}

impl PairingWindow {
    pub fn is_open(&self) -> bool {
        self.deadline_ms.is_some()
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Passkey currently displayed for an in-progress pairing.
    pub fn passkey(&self) -> Option<u32> {
        self.passkey
    }

    pub fn remaining_ms(&self, now_ms: u64) -> u64 {
        self.deadline_ms
            .map_or(0, |deadline| deadline.saturating_sub(now_ms))
    }

    pub fn open(&mut self, now_ms: u64) -> Option<PairingAction> {
        self.deadline_ms = Some(now_ms + PAIRING_WINDOW_MS);
        (!self.connected && !self.advertising).then(|| {
            self.advertising = true;
            PairingAction::StartAdvertising
        })
    }

    pub fn on_connected(&mut self, now_ms: u64) -> Option<PairingAction> {
        self.connected = true;
        self.deadline_ms = Some(now_ms + CONNECTED_IDLE_MS);
        self.advertising.then(|| {
            self.advertising = false;
            PairingAction::StopAdvertising
        })
    }

    pub fn on_activity(&mut self, now_ms: u64) {
        if self.connected {
            self.deadline_ms = Some(now_ms + CONNECTED_IDLE_MS);
        }
    }

    pub fn on_passkey(&mut self, passkey: u32) {
        self.passkey = Some(passkey);
    }

    pub fn on_pairing_finished(&mut self) {
        self.passkey = None;
    }

    pub fn on_disconnected(&mut self, now_ms: u64) -> Option<PairingAction> {
        self.connected = false;
        self.passkey = None;
        if !self.is_open() {
            return None;
        }
        self.deadline_ms = Some(now_ms + RECONNECT_GRACE_MS);
        (!self.advertising).then(|| {
            self.advertising = true;
            PairingAction::StartAdvertising
        })
    }

    pub fn tick(&mut self, now_ms: u64) -> Option<PairingAction> {
        match self.deadline_ms {
            Some(deadline) if now_ms >= deadline => {
                *self = Self {
                    connected: self.connected,
                    ..Self::default()
                };
                Some(PairingAction::Close)
            }
            _ => None,
        }
    }
}

#[cfg(target_os = "espidf")]
pub mod esp;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_fragmented_and_coalesced_commands() {
        let mut framer = CommandFramer::default();
        assert!(framer.push(b"PI").unwrap().is_empty());
        assert_eq!(
            framer.push(b"NG\r\nSSID:home\n").unwrap(),
            vec![Command::Ping, Command::SetSsid("home".into())]
        );
    }

    #[test]
    fn rejects_and_recovers_from_oversized_input() {
        let mut framer = CommandFramer::default();
        assert_eq!(
            framer.push(&vec![b'x'; RX_BUFFER_LIMIT + 1]),
            Err(FrameError::TooLong)
        );
        assert_eq!(framer.push(b"PING\n").unwrap(), vec![Command::Ping]);
    }

    #[test]
    fn notifications_are_mtu_safe_and_newline_terminated() {
        let chunks = notification_chunks("a message that is longer than twenty bytes", 23);
        assert!(chunks.iter().all(|chunk| chunk.len() <= 20));
        assert_eq!(chunks.concat().last(), Some(&b'\n'));
        let message = "x".repeat(300);
        let chunks = notification_chunks(&message, PREFERRED_MTU);
        assert_eq!(chunks[0].len(), 244);
        assert_eq!(chunks.concat().len(), 301);
        assert!(notification_chunks("hi", 0).iter().all(|c| c.len() <= 20));
    }

    #[test]
    fn parses_new_commands_case_insensitively() {
        assert_eq!(parse_command("info"), Command::Info);
        assert_eq!(parse_command("SCAN"), Command::Scan);
        assert_eq!(parse_command("Read"), Command::Read);
        assert_eq!(parse_command("live:on"), Command::Live(true));
        assert_eq!(parse_command("LIVE:OFF"), Command::Live(false));
        assert_eq!(parse_command("ota:check"), Command::OtaCheck);
        assert_eq!(parse_command("SSID:a:b"), Command::SetSsid("a:b".into()));
    }

    #[test]
    fn setters_take_any_case_and_keep_the_value_as_typed() {
        assert_eq!(parse_command("ssid:Home"), Command::SetSsid("Home".into()));
        assert_eq!(
            parse_command("Pass:S3cret"),
            Command::SetPassword("S3cret".into())
        );
        assert_eq!(
            parse_command("devid:dev_1"),
            Command::SetDeviceId("dev_1".into())
        );
        assert_eq!(
            parse_command("DevKey:sk_A"),
            Command::SetDeviceKey("sk_A".into())
        );
        assert_eq!(parse_command("PASS:"), Command::SetPassword(String::new()));
        // Not a setter, and not split in the middle of a character.
        assert_eq!(parse_command("pässword"), Command::Unknown);
        assert_eq!(parse_command("PASS"), Command::Unknown);
    }

    #[test]
    fn coordinates_must_be_real_places() {
        assert_eq!(parse_command("LAT:24.8607"), Command::SetLatitude(24.8607));
        assert_eq!(
            parse_command("lon: -67.0011 "),
            Command::SetLongitude(-67.0011)
        );
        assert_eq!(parse_command("LAT:-90"), Command::SetLatitude(-90.0));
        assert_eq!(parse_command("LON:180"), Command::SetLongitude(180.0));
        for bad in ["LAT:abc", "LAT:", "LAT:90.1", "LAT:nan", "LAT:inf"] {
            assert_eq!(parse_command(bad), Command::Invalid("LAT"), "{bad}");
        }
        for bad in ["LON:abc", "LON:-180.5", "LON:NaN"] {
            assert_eq!(parse_command(bad), Command::Invalid("LON"), "{bad}");
        }
    }

    #[test]
    fn pairing_window_opens_and_times_out() {
        let mut window = PairingWindow::default();
        assert!(!window.is_open());
        assert_eq!(window.tick(0), None);
        assert_eq!(window.open(1_000), Some(PairingAction::StartAdvertising));
        assert_eq!(
            window.open(2_000),
            None,
            "already advertising; extends deadline"
        );
        assert_eq!(window.remaining_ms(2_000), PAIRING_WINDOW_MS);
        assert_eq!(window.tick(2_000 + PAIRING_WINDOW_MS - 1), None);
        assert_eq!(
            window.tick(2_000 + PAIRING_WINDOW_MS),
            Some(PairingAction::Close)
        );
        assert!(!window.is_open());
    }

    #[test]
    fn pairing_window_tracks_connection_idle_and_reconnect_grace() {
        let mut window = PairingWindow::default();
        window.open(0);
        assert_eq!(
            window.on_connected(10),
            Some(PairingAction::StopAdvertising)
        );
        window.on_passkey(123_456);
        assert_eq!(window.passkey(), Some(123_456));
        window.on_pairing_finished();
        assert_eq!(window.passkey(), None);
        window.on_activity(1_000);
        assert_eq!(window.tick(1_000 + CONNECTED_IDLE_MS - 1), None);
        assert_eq!(
            window.on_disconnected(5_000),
            Some(PairingAction::StartAdvertising)
        );
        assert_eq!(
            window.tick(5_000 + RECONNECT_GRACE_MS),
            Some(PairingAction::Close)
        );
        assert_eq!(
            window.on_disconnected(999_999),
            None,
            "closed window stays closed"
        );
    }

    #[test]
    fn connected_idle_timeout_closes_window() {
        let mut window = PairingWindow::default();
        window.open(0);
        window.on_connected(0);
        assert_eq!(window.tick(CONNECTED_IDLE_MS), Some(PairingAction::Close));
        assert!(
            window.is_connected(),
            "close action disconnects; state follows event"
        );
    }
}
