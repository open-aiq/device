use embedded_svc::{
    http::{client::Client as HttpClient, Method as HttpMethod},
    io::Write,
};
use esp_idf_svc::http::client::{Configuration, EspHttpConnection};

use crate::{
    config::{DeviceConfig, BACKEND_BASE_URL},
    device_api::{ApiResponse, Method, FIRMWARE_VERSION_HEADER},
    FIRMWARE_VERSION,
};

/// Responses larger than this are truncated; device endpoints return small JSON.
const RESPONSE_LIMIT: usize = 4 * 1024;

pub fn http_client(timeout_secs: u64) -> anyhow::Result<HttpClient<EspHttpConnection>> {
    let http_config = Configuration {
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        timeout: Some(std::time::Duration::from_secs(timeout_secs)),
        // Redirect `Location` headers (e.g. GitHub release assets) exceed the 512 B default.
        buffer_size: Some(4096),
        buffer_size_tx: Some(2048),
        ..Default::default()
    };
    Ok(HttpClient::wrap(EspHttpConnection::new(&http_config)?))
}

/// Sends one device-authenticated request to `BACKEND_BASE_URL + path`.
pub fn request(
    config: &DeviceConfig,
    method: Method,
    path: &str,
    body: Option<&str>,
) -> anyhow::Result<ApiResponse> {
    if !config.has_device_credentials() {
        anyhow::bail!("device credentials are not provisioned");
    }
    let mut client = http_client(15)?;
    let content_length = body.map(|value| value.len().to_string());
    let mut headers = vec![
        ("accept", "application/json"),
        ("X-Device-ID", config.device_id.as_str()),
        ("X-Device-Key", config.device_key.as_str()),
        (FIRMWARE_VERSION_HEADER, FIRMWARE_VERSION),
    ];
    if let Some(length) = content_length.as_deref() {
        headers.push(("content-type", "application/json"));
        headers.push(("content-length", length));
    }
    let url = format!("{BACKEND_BASE_URL}{path}");
    let method = match method {
        Method::Get => HttpMethod::Get,
        Method::Post => HttpMethod::Post,
    };
    let mut request = client.request(method, &url, &headers)?;
    if let Some(body) = body {
        request.write_all(body.as_bytes())?;
        request.flush()?;
    }
    let mut response = request.submit()?;
    let status = response.status();
    let mut body = Vec::new();
    let mut buffer = [0u8; 512];
    loop {
        let read = response.read(&mut buffer)?;
        if read == 0 || body.len() >= RESPONSE_LIMIT {
            break;
        }
        body.extend_from_slice(&buffer[..read]);
    }
    body.truncate(RESPONSE_LIMIT);
    Ok(ApiResponse { status, body })
}
