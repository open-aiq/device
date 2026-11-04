use crate::{
    config::DeviceConfig,
    device_api::{esp::request, Method, DATA_PATH},
    sensors::SensorReadings,
    telemetry::{build_payload, TelemetrySink, UploadResult},
};

pub struct EspTelemetrySink;

impl TelemetrySink for EspTelemetrySink {
    type Error = anyhow::Error;

    fn upload(
        &mut self,
        config: &DeviceConfig,
        readings: &SensorReadings,
    ) -> Result<UploadResult, Self::Error> {
        if !config.has_device_credentials() {
            return Ok(UploadResult::default());
        }
        let payload = build_payload(readings, config)
            .map_err(|error| anyhow::anyhow!("payload error: {error:?}"))?;
        let response = request(config, Method::Post, DATA_PATH, Some(&payload))?;
        let code = i32::from(response.status);
        Ok(UploadResult {
            ok: code == 201,
            http_code: code,
        })
    }
}
