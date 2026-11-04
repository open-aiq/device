use crate::config::DeviceConfig;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WifiStatus {
    pub connected: bool,
    pub ip: Option<String>,
}

pub trait WifiPort {
    type Error;
    fn connect(&mut self, config: &DeviceConfig) -> Result<bool, Self::Error>;
    fn status(&self) -> WifiStatus;
}

#[cfg(target_os = "espidf")]
pub mod esp;
