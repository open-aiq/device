use embedded_svc::{http::client::Client as HttpClient, io::Write};
use esp_idf_svc::http::client::{Configuration, EspHttpConnection};

use crate::{
    config::{DeviceConfig, BACKEND_BASE_URL},
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
        let http_config = Configuration {
            crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
            timeout: Some(std::time::Duration::from_secs(15)),
            ..Default::default()
        };
        let mut client = HttpClient::wrap(EspHttpConnection::new(&http_config)?);
        let content_length = payload.len().to_string();
        let headers = [
            ("content-type", "application/json"),
            ("content-length", content_length.as_str()),
            ("X-Device-ID", config.device_id.as_str()),
            ("X-Device-Key", config.device_key.as_str()),
        ];
        let url = format!("{BACKEND_BASE_URL}/data");
        let mut request = client.post(&url, &headers)?;
        request.write_all(payload.as_bytes())?;
        request.flush()?;
        let response = request.submit()?;
        let code = i32::from(response.status());
        Ok(UploadResult {
            ok: code == 201,
            http_code: code,
        })
    }
}
