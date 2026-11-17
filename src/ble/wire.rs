//! The BLE provisioning vocabulary: every command the phone sends and every
//! line the device sends back. Mirrors `Command`, `Setter`, `Reply` and
//! `DeviceErrorCode` in `app/lib/ble/protocol.dart`; keep the two in step.
//!
//! Lines are `KEY` or `KEY:value`. Build them with the helpers here rather
//! than `format!` at the call site.

use core::fmt::{self, Display};

/// Splits a key from its value, as in `SSID:home`.
pub const SEPARATOR: char = ':';
/// Splits the parts of a value, as in `NET:-61,open,Home`.
pub const LIST_SEPARATOR: char = ',';

/// Phone -> device.
pub mod command {
    pub const PING: &str = "PING";
    pub const STATUS: &str = "STATUS";
    pub const INFO: &str = "INFO";
    pub const SCAN: &str = "SCAN";
    pub const READ: &str = "READ";
    pub const LIVE_ON: &str = "LIVE:ON";
    pub const LIVE_OFF: &str = "LIVE:OFF";
    pub const OTA_CHECK: &str = "OTA:CHECK";
    pub const SAVE: &str = "SAVE";
    pub const LOAD: &str = "LOAD";
    pub const CLEAR: &str = "CLEAR";

    // Setters, sent as `<KEY>:<value>`.
    pub const SSID: &str = "SSID";
    pub const PASS: &str = "PASS";
    pub const LAT: &str = "LAT";
    pub const LON: &str = "LON";
    pub const DEVID: &str = "DEVID";
    pub const DEVKEY: &str = "DEVKEY";
}

/// Device -> phone.
pub mod reply {
    use core::fmt::Display;

    use super::{ErrorCode, LIST_SEPARATOR, SEPARATOR};

    pub const PONG: &str = "PONG";
    pub const INFO_BEGIN: &str = "INFO_BEGIN";
    pub const INFO_END: &str = "INFO_END";
    pub const SCAN_BEGIN: &str = "SCAN_BEGIN";
    pub const SCAN_END: &str = "SCAN_END";
    pub const SAVE_BEGIN: &str = "SAVE_BEGIN";
    pub const SAVE_END: &str = "SAVE_END";
    pub const LOAD_BEGIN: &str = "LOAD_BEGIN";
    pub const LOAD_END: &str = "LOAD_END";
    pub const WIFI_CONNECTING: &str = "WIFI_CONNECTING";
    pub const SAVE_OK: &str = "SAVE_OK";
    pub const CLEARED: &str = "CLEARED all stored config";
    pub const OK_LIVE_ON: &str = "OK: live on";
    pub const OK_LIVE_OFF: &str = "OK: live off";
    pub const OK_OTA_CHECK: &str = "OK: ota check";
    pub const OK_PASSWORD_SET: &str = "OK: password set";
    pub const OK_DEVICE_KEY_SET: &str = "OK: device key set";
    pub const OTA_NONE: &str = "OTA:NONE";
    pub const OTA_FAILED: &str = "OTA:FAILED";
    /// Followed by a space and the version being installed.
    pub const OTA_STARTING: &str = "OTA:STARTING";

    // Field keys, sent as `<KEY>:<value>`.
    pub const FW: &str = "FW";
    pub const MAC: &str = "MAC";
    pub const DEVID: &str = "DEVID";
    pub const DEVKEY_SET: &str = "DEVKEY_SET";
    pub const PASS_SET: &str = "PASS_SET";
    pub const WIFI: &str = "WIFI";
    pub const SSID: &str = "SSID";
    pub const IP: &str = "IP";
    pub const LAT: &str = "LAT";
    pub const LON: &str = "LON";
    pub const LAST_UPLOAD: &str = "LAST_UPLOAD";
    pub const NET: &str = "NET";
    pub const READING: &str = "READING";
    pub const SAVE_FAILED: &str = "SAVE_FAILED";
    pub const ERROR: &str = "ERROR";

    // Values.
    pub const CONNECTED: &str = "connected";
    pub const DISCONNECTED: &str = "disconnected";
    pub const NO_IP: &str = "0.0.0.0";
    pub const NONE: &str = "none";
    pub const OPEN: &str = "open";
    pub const SECURED: &str = "secured";
    pub const FAILED_WIFI: &str = "WIFI";
    pub const FAILED_STORAGE: &str = "STORAGE";

    /// `<KEY>:<value>`.
    pub fn field(key: &str, value: impl Display) -> String {
        format!("{key}{SEPARATOR}{value}")
    }

    /// `<KEY>:1` or `<KEY>:0`.
    pub fn flag(key: &str, set: bool) -> String {
        field(key, u8::from(set))
    }

    /// `<KEY>:<a>,<b>,…`.
    pub fn list(key: &str, parts: &[&dyn Display]) -> String {
        let mut value = String::new();
        for (index, part) in parts.iter().enumerate() {
            if index > 0 {
                value.push(LIST_SEPARATOR);
            }
            value.push_str(&part.to_string());
        }
        field(key, value)
    }

    /// `OK: <KEY> set to <value>`, the answer to a setter.
    pub fn ok_set(key: &str, value: impl Display) -> String {
        format!("OK: {key} set to {value}")
    }

    /// `OTA:STARTING <version>`.
    pub fn ota_starting(version: impl Display) -> String {
        format!("{OTA_STARTING} {version}")
    }

    /// The `STATUS` answer: `WiFi: <connected|disconnected> IP: <ip>`.
    pub fn status(connected: bool, ip: Option<&str>) -> String {
        format!("WiFi: {} IP: {}", wifi(connected), ip.unwrap_or(NO_IP))
    }

    pub fn wifi(connected: bool) -> &'static str {
        if connected {
            CONNECTED
        } else {
            DISCONNECTED
        }
    }

    /// `ERROR:<code>`.
    pub fn error(code: ErrorCode) -> String {
        field(ERROR, code)
    }
}

/// What follows `ERROR:` in a failure reply.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    Scan,
    NoReading,
    Storage,
    CommandTooLong,
    InvalidUtf8,
    InvalidLat,
    InvalidLon,
    UnknownCommand,
}

impl ErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scan => "SCAN",
            Self::NoReading => "NO_READING",
            Self::Storage => "STORAGE",
            Self::CommandTooLong => "COMMAND_TOO_LONG",
            Self::InvalidUtf8 => "INVALID_UTF8",
            Self::InvalidLat => "INVALID_LAT",
            Self::InvalidLon => "INVALID_LON",
            Self::UnknownCommand => "UNKNOWN_COMMAND",
        }
    }
}

impl Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::{command, reply, ErrorCode};

    /// The one place the wire strings are spelled out. If this fails, the
    /// app (`app/lib/ble/protocol.dart`) and the protocol docs need the same
    /// change.
    #[test]
    fn wire_format_is_unchanged() {
        let commands = [
            (command::PING, "PING"),
            (command::STATUS, "STATUS"),
            (command::INFO, "INFO"),
            (command::SCAN, "SCAN"),
            (command::READ, "READ"),
            (command::LIVE_ON, "LIVE:ON"),
            (command::LIVE_OFF, "LIVE:OFF"),
            (command::OTA_CHECK, "OTA:CHECK"),
            (command::SAVE, "SAVE"),
            (command::LOAD, "LOAD"),
            (command::CLEAR, "CLEAR"),
            (command::SSID, "SSID"),
            (command::PASS, "PASS"),
            (command::LAT, "LAT"),
            (command::LON, "LON"),
            (command::DEVID, "DEVID"),
            (command::DEVKEY, "DEVKEY"),
        ];
        let replies = [
            (reply::PONG, "PONG"),
            (reply::INFO_BEGIN, "INFO_BEGIN"),
            (reply::INFO_END, "INFO_END"),
            (reply::SCAN_BEGIN, "SCAN_BEGIN"),
            (reply::SCAN_END, "SCAN_END"),
            (reply::SAVE_BEGIN, "SAVE_BEGIN"),
            (reply::SAVE_END, "SAVE_END"),
            (reply::LOAD_BEGIN, "LOAD_BEGIN"),
            (reply::LOAD_END, "LOAD_END"),
            (reply::WIFI_CONNECTING, "WIFI_CONNECTING"),
            (reply::SAVE_OK, "SAVE_OK"),
            (reply::CLEARED, "CLEARED all stored config"),
            (reply::OK_LIVE_ON, "OK: live on"),
            (reply::OK_LIVE_OFF, "OK: live off"),
            (reply::OK_OTA_CHECK, "OK: ota check"),
            (reply::OK_PASSWORD_SET, "OK: password set"),
            (reply::OK_DEVICE_KEY_SET, "OK: device key set"),
            (reply::OTA_NONE, "OTA:NONE"),
            (reply::OTA_FAILED, "OTA:FAILED"),
        ];
        for (actual, expected) in commands.into_iter().chain(replies) {
            assert_eq!(actual, expected);
        }

        assert_eq!(reply::field(reply::FW, "1.2.3"), "FW:1.2.3");
        assert_eq!(reply::flag(reply::DEVKEY_SET, true), "DEVKEY_SET:1");
        assert_eq!(reply::flag(reply::PASS_SET, false), "PASS_SET:0");
        assert_eq!(
            reply::list(reply::NET, &[&-61, &reply::OPEN, &"My, Wifi"]),
            "NET:-61,open,My, Wifi"
        );
        assert_eq!(
            reply::list(reply::LAST_UPLOAD, &[&201, &42]),
            "LAST_UPLOAD:201,42"
        );
        assert_eq!(
            reply::field(reply::LAST_UPLOAD, reply::NONE),
            "LAST_UPLOAD:none"
        );
        assert_eq!(
            reply::field(reply::SAVE_FAILED, reply::FAILED_STORAGE),
            "SAVE_FAILED:STORAGE"
        );
        assert_eq!(
            reply::field(reply::WIFI, reply::wifi(true)),
            "WIFI:connected"
        );
        assert_eq!(reply::field(reply::READING, "{}"), "READING:{}");
        assert_eq!(
            reply::ok_set(command::LAT, "24.860700"),
            "OK: LAT set to 24.860700"
        );
        assert_eq!(reply::ota_starting("1.4.0"), "OTA:STARTING 1.4.0");
        assert_eq!(reply::status(false, None), "WiFi: disconnected IP: 0.0.0.0");
        assert_eq!(
            reply::status(true, Some("10.0.0.7")),
            "WiFi: connected IP: 10.0.0.7"
        );

        let errors = [
            (ErrorCode::Scan, "ERROR:SCAN"),
            (ErrorCode::NoReading, "ERROR:NO_READING"),
            (ErrorCode::Storage, "ERROR:STORAGE"),
            (ErrorCode::CommandTooLong, "ERROR:COMMAND_TOO_LONG"),
            (ErrorCode::InvalidUtf8, "ERROR:INVALID_UTF8"),
            (ErrorCode::InvalidLat, "ERROR:INVALID_LAT"),
            (ErrorCode::InvalidLon, "ERROR:INVALID_LON"),
            (ErrorCode::UnknownCommand, "ERROR:UNKNOWN_COMMAND"),
        ];
        for (code, expected) in errors {
            assert_eq!(reply::error(code), expected);
        }
    }
}
