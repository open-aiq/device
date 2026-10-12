use serde::Serialize;

use crate::{
    aqi::compute_us_aqi,
    config::DeviceConfig,
    sensors::{ClimateReading, PmReading, SensorReadings},
};

#[derive(Default)]
pub struct Aggregator {
    pub pm_count: u32,
    pub climate_count: u32,
    pm1_sum: f64,
    pm25_sum: f64,
    pm10_sum: f64,
    temperature_sum: f64,
    humidity_sum: f64,
    heat_index_sum: f64,
}

impl Aggregator {
    pub fn add(&mut self, readings: &SensorReadings) {
        if let Some(pm) = &readings.pm {
            self.pm_count += 1;
            self.pm1_sum += f64::from(pm.pm1_0);
            self.pm25_sum += f64::from(pm.pm2_5);
            self.pm10_sum += f64::from(pm.pm10_0);
        }
        if let Some(climate) = &readings.climate {
            self.climate_count += 1;
            self.temperature_sum += f64::from(climate.temperature_c);
            self.humidity_sum += f64::from(climate.humidity);
            self.heat_index_sum += f64::from(climate.heat_index_c);
        }
    }

    pub fn complete(&self) -> bool {
        self.pm_count > 0 && self.climate_count > 0
    }

    pub fn mean(&self) -> Option<SensorReadings> {
        if !self.complete() {
            return None;
        }
        let pm = PmReading {
            pm1_0: (self.pm1_sum / f64::from(self.pm_count)).round() as u16,
            pm2_5: (self.pm25_sum / f64::from(self.pm_count)).round() as u16,
            pm10_0: (self.pm10_sum / f64::from(self.pm_count)).round() as u16,
            number_0_3: 0,
            number_0_5: 0,
            number_1_0: 0,
            number_2_5: 0,
            number_5_0: 0,
            number_10_0: 0,
        };
        let climate = ClimateReading {
            temperature_c: (self.temperature_sum / f64::from(self.climate_count)) as f32,
            humidity: (self.humidity_sum / f64::from(self.climate_count)) as f32,
            heat_index_c: (self.heat_index_sum / f64::from(self.climate_count)) as f32,
        };
        let aqi = compute_us_aqi(pm.pm2_5, pm.pm10_0);
        Some(SensorReadings {
            pm: Some(pm),
            climate: Some(climate),
            aqi: Some(aqi),
        })
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Serialize)]
struct PmsPayload<'a> {
    pm1_0: u16,
    pm2_5: u16,
    pm10_0: u16,
    provider: &'a str,
}

#[derive(Serialize)]
struct TemperaturePayload<'a> {
    temperature: f32,
    humidity: f32,
    heat_index: f32,
    provider: &'a str,
}

#[derive(Serialize)]
struct LocationPayload<'a> {
    lat: f64,
    lon: f64,
    provider: &'a str,
}

#[derive(Serialize)]
struct UploadPayload<'a> {
    pms_data: PmsPayload<'a>,
    aqi: u16,
    temperature_data: TemperaturePayload<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<LocationPayload<'a>>,
}

pub fn build_payload(
    readings: &SensorReadings,
    config: &DeviceConfig,
) -> Result<String, PayloadError> {
    let pm = readings.pm.as_ref().ok_or(PayloadError::Incomplete)?;
    let climate = readings.climate.as_ref().ok_or(PayloadError::Incomplete)?;
    let aqi = readings.aqi.ok_or(PayloadError::Incomplete)?;
    let payload = UploadPayload {
        pms_data: PmsPayload {
            pm1_0: pm.pm1_0,
            pm2_5: pm.pm2_5,
            pm10_0: pm.pm10_0,
            provider: "pms5003",
        },
        aqi: aqi.aqi,
        temperature_data: TemperaturePayload {
            temperature: climate.temperature_c,
            humidity: climate.humidity,
            heat_index: climate.heat_index_c,
            provider: "sht30",
        },
        location: config.has_location().then_some(LocationPayload {
            lat: config.lat,
            lon: config.lon,
            provider: "mobile",
        }),
    };
    serde_json::to_string(&payload).map_err(|_| PayloadError::Serialize)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayloadError {
    Incomplete,
    Serialize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UploadResult {
    pub ok: bool,
    pub http_code: i32,
}

pub trait TelemetrySink {
    type Error;
    fn upload(
        &mut self,
        config: &DeviceConfig,
        readings: &SensorReadings,
    ) -> Result<UploadResult, Self::Error>;
}

#[cfg(target_os = "espidf")]
pub mod esp;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensors::readings;

    fn sample(pm25: u16, temp: f32) -> SensorReadings {
        readings(
            Some(PmReading {
                pm1_0: 5,
                pm2_5: pm25,
                pm10_0: 18,
                number_0_3: 0,
                number_0_5: 0,
                number_1_0: 0,
                number_2_5: 0,
                number_5_0: 0,
                number_10_0: 0,
            }),
            Some(ClimateReading {
                temperature_c: temp,
                humidity: 50.0,
                heat_index_c: temp,
            }),
        )
    }

    #[test]
    fn averages_and_resets() {
        let mut aggregator = Aggregator::default();
        aggregator.add(&sample(10, 20.0));
        aggregator.add(&sample(20, 30.0));
        let mean = aggregator.mean().unwrap();
        assert_eq!(mean.pm.unwrap().pm2_5, 15);
        assert_eq!(mean.climate.unwrap().temperature_c, 25.0);
        aggregator.reset();
        assert!(!aggregator.complete());
    }

    #[test]
    fn payload_uses_sht30_and_omits_empty_location() {
        let json = build_payload(&sample(12, 25.0), &DeviceConfig::default()).unwrap();
        assert!(json.contains("\"provider\":\"sht30\""));
        assert!(!json.contains("location"));
        let config = DeviceConfig {
            lat: 24.86,
            lon: 67.01,
            ..Default::default()
        };
        assert!(build_payload(&sample(12, 25.0), &config)
            .unwrap()
            .contains("location"));
    }
}
