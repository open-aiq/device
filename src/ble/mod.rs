pub const NUS_SERVICE_UUID: &str = "6E400001-B5A3-F393-E0A9-E50E24DCCA9E";
pub const NUS_RX_UUID: &str = "6E400002-B5A3-F393-E0A9-E50E24DCCA9E";
pub const NUS_TX_UUID: &str = "6E400003-B5A3-F393-E0A9-E50E24DCCA9E";
pub const RX_BUFFER_LIMIT: usize = 512;
pub const NOTIFICATION_PAYLOAD: usize = 20;

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Ping,
    Status,
    Save,
    Load,
    Clear,
    SetSsid(String),
    SetPassword(String),
    SetLatitude(f64),
    SetLongitude(f64),
    SetDeviceId(String),
    SetDeviceKey(String),
    ProtocolError(FrameError),
    Unknown(String),
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
    } else if let Some(value) = value.strip_prefix("SSID:") {
        Command::SetSsid(value.to_owned())
    } else if let Some(value) = value.strip_prefix("PASS:") {
        Command::SetPassword(value.to_owned())
    } else if let Some(value) = value.strip_prefix("LAT:") {
        Command::SetLatitude(value.parse().unwrap_or(0.0))
    } else if let Some(value) = value.strip_prefix("LON:") {
        Command::SetLongitude(value.parse().unwrap_or(0.0))
    } else if let Some(value) = value.strip_prefix("DEVID:") {
        Command::SetDeviceId(value.to_owned())
    } else if let Some(value) = value.strip_prefix("DEVKEY:") {
        Command::SetDeviceKey(value.to_owned())
    } else {
        Command::Unknown(value.to_owned())
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

pub fn notification_chunks(message: &str) -> Vec<Vec<u8>> {
    let framed = format!("{message}\n");
    framed
        .as_bytes()
        .chunks(NOTIFICATION_PAYLOAD)
        .map(<[u8]>::to_vec)
        .collect()
}

pub trait BleTransport {
    type Error;
    fn poll_command(&mut self) -> Option<Command>;
    fn send_line(&mut self, message: &str) -> Result<(), Self::Error>;
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
        let chunks = notification_chunks("a message that is longer than twenty bytes");
        assert!(chunks.iter().all(|chunk| chunk.len() <= 20));
        assert_eq!(chunks.concat().last(), Some(&b'\n'));
    }
}
