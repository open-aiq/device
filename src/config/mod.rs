use serde::{Deserialize, Serialize};

pub const BACKEND_BASE_URL: &str = match option_env!("BACKEND_BASE_URL") {
    Some(value) => value,
    None => "https://api.air-iq.net/api/v1",
};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub ssid: String,
    pub password: String,
    pub lat: f64,
    pub lon: f64,
    pub device_id: String,
    pub device_key: String,
}

impl DeviceConfig {
    pub fn has_wifi(&self) -> bool {
        !self.ssid.is_empty()
    }

    pub fn has_device_credentials(&self) -> bool {
        !self.device_id.is_empty() && !self.device_key.is_empty()
    }

    pub fn has_location(&self) -> bool {
        self.lat != 0.0 || self.lon != 0.0
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(-90.0..=90.0).contains(&self.lat) {
            return Err(ConfigError::Latitude);
        }
        if !(-180.0..=180.0).contains(&self.lon) {
            return Err(ConfigError::Longitude);
        }
        Ok(())
    }

    pub fn redacted_summary(&self) -> String {
        format!(
            "ssid={} password={} lat={:.6} lon={:.6} device_id={} device_key={}",
            if self.ssid.is_empty() {
                "(none)"
            } else {
                &self.ssid
            },
            if self.password.is_empty() {
                "(none)"
            } else {
                "(set)"
            },
            self.lat,
            self.lon,
            if self.device_id.is_empty() {
                "(none)"
            } else {
                &self.device_id
            },
            if self.device_key.is_empty() {
                "(none)"
            } else {
                "(set)"
            },
        )
    }
}

pub fn wipe_config_on_boot() -> bool {
    option_env!("WIPE_CONFIG_ON_BOOT") == Some("1")
}

pub fn mock_provisioning() -> Option<DeviceConfig> {
    let ssid = option_env!("MOCK_WIFI_SSID")?;
    Some(DeviceConfig {
        ssid: ssid.to_owned(),
        password: option_env!("MOCK_WIFI_PASSWORD")
            .unwrap_or_default()
            .to_owned(),
        lat: option_env!("MOCK_LAT")
            .and_then(|value| value.parse().ok())
            .unwrap_or_default(),
        lon: option_env!("MOCK_LON")
            .and_then(|value| value.parse().ok())
            .unwrap_or_default(),
        device_id: option_env!("MOCK_DEVICE_ID").unwrap_or_default().to_owned(),
        device_key: option_env!("MOCK_DEVICE_KEY")
            .unwrap_or_default()
            .to_owned(),
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigError {
    Latitude,
    Longitude,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_coordinates_and_redacts_secrets() {
        let config = DeviceConfig {
            ssid: "office".into(),
            password: "secret".into(),
            lat: 24.86,
            lon: 67.01,
            device_id: "dev_1".into(),
            device_key: "sk_private".into(),
        };
        assert_eq!(config.validate(), Ok(()));
        let summary = config.redacted_summary();
        assert!(!summary.contains("secret"));
        assert!(!summary.contains("sk_private"));
        assert!(summary.contains("password=(set)"));
    }

    #[test]
    fn rejects_invalid_coordinates() {
        let mut config = DeviceConfig {
            lat: 91.0,
            ..Default::default()
        };
        assert_eq!(config.validate(), Err(ConfigError::Latitude));
        config.lat = 0.0;
        config.lon = -181.0;
        assert_eq!(config.validate(), Err(ConfigError::Longitude));
    }
}
