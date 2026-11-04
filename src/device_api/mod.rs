//! Device-authenticated backend endpoints (`X-Device-ID` / `X-Device-Key`).

pub const DATA_PATH: &str = "/data";
pub const OTA_PATH: &str = "/device/ota";
pub const OTA_STATUS_PATH: &str = "/device/ota/status";
pub const LOGS_PATH: &str = "/device/logs";
pub const FIRMWARE_VERSION_HEADER: &str = "X-Firmware-Version";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Method {
    Get,
    Post,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ApiResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

impl ApiResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

#[cfg(target_os = "espidf")]
pub mod esp;
