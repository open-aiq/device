use crate::config::DeviceConfig;

pub trait ConfigStore {
    type Error;
    fn load(&self) -> Result<DeviceConfig, Self::Error>;
    fn save(&self, config: &DeviceConfig) -> Result<(), Self::Error>;
    fn clear(&self) -> Result<(), Self::Error>;
}

#[cfg(target_os = "espidf")]
pub mod esp;
