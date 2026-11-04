pub mod aqi;
pub mod ble;
#[cfg(target_os = "espidf")]
pub mod board;
pub mod buttons;
pub mod config;
pub mod device_api;
pub mod devlog;
pub mod display;
pub mod sensors;
pub mod storage;
pub mod telemetry;
pub mod wifi;

pub const SENSOR_INTERVAL_MS: u64 = 20_000;
pub const UPLOAD_INTERVAL_MS: u64 = 10 * 60_000;
/// Firmware version reported over BLE (`FW:`) and HTTP (`X-Firmware-Version`).
pub const FIRMWARE_VERSION: &str = env!("CARGO_PKG_VERSION");
