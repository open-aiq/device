use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition, EspNvs};

use crate::{config::DeviceConfig, storage::ConfigStore};

pub struct NvsConfigStore {
    nvs: EspDefaultNvs,
}

impl NvsConfigStore {
    pub fn new(partition: EspDefaultNvsPartition) -> anyhow::Result<Self> {
        Ok(Self {
            nvs: EspNvs::new(partition, "config", true)?,
        })
    }

    fn string(&self, key: &str) -> Result<String, esp_idf_svc::sys::EspError> {
        let length = self.nvs.str_len(key)?.unwrap_or(1);
        let mut buffer = vec![0u8; length];
        Ok(self.nvs.get_str(key, &mut buffer)?.unwrap_or("").to_owned())
    }
}

impl ConfigStore for NvsConfigStore {
    type Error = anyhow::Error;

    fn load(&self) -> Result<DeviceConfig, Self::Error> {
        Ok(DeviceConfig {
            ssid: self.string("ssid")?,
            password: self.string("pass")?,
            lat: self.string("lat")?.parse().unwrap_or_default(),
            lon: self.string("lon")?.parse().unwrap_or_default(),
            device_id: self.string("devid")?,
            device_key: self.string("devkey")?,
        })
    }

    fn save(&self, config: &DeviceConfig) -> Result<(), Self::Error> {
        config
            .validate()
            .map_err(|error| anyhow::anyhow!("invalid device config: {error:?}"))?;
        self.nvs.set_str("ssid", &config.ssid)?;
        self.nvs.set_str("pass", &config.password)?;
        self.nvs.set_str("lat", &format!("{:.8}", config.lat))?;
        self.nvs.set_str("lon", &format!("{:.8}", config.lon))?;
        self.nvs.set_str("devid", &config.device_id)?;
        self.nvs.set_str("devkey", &config.device_key)?;
        Ok(())
    }

    fn clear(&self) -> Result<(), Self::Error> {
        self.nvs.erase_all()?;
        Ok(())
    }
}
