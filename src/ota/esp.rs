use embedded_svc::http::client::Client as HttpClient;
use esp_idf_svc::{ota::EspOta, sys};

use crate::{
    config::DeviceConfig,
    device_api::{
        esp::{http_client, request},
        Method, OTA_PATH, OTA_STATUS_PATH,
    },
    ota::{parse_check_response, ImageVerifier, OtaJob, ProgressThrottle, StatusReport},
};

/// True when the running image was just installed and has not confirmed itself yet.
pub fn running_image_pending_verify() -> bool {
    unsafe {
        let partition = sys::esp_ota_get_running_partition();
        if partition.is_null() {
            return false;
        }
        let mut state: sys::esp_ota_img_states_t = 0;
        sys::esp_ota_get_state_partition(partition, &mut state) == sys::ESP_OK
            && state == sys::esp_ota_img_states_t_ESP_OTA_IMG_PENDING_VERIFY
    }
}

/// Confirms the running image so the bootloader keeps it.
pub fn mark_running_valid() -> anyhow::Result<()> {
    EspOta::new()?.mark_running_slot_valid()?;
    Ok(())
}

/// Marks the running image invalid and reboots into the previous slot.
pub fn rollback_and_reboot() -> anyhow::Error {
    match EspOta::new() {
        Ok(mut ota) => ota.mark_running_slot_invalid_and_reboot().into(),
        Err(error) => error.into(),
    }
}

pub fn check(config: &DeviceConfig) -> anyhow::Result<Option<OtaJob>> {
    let response = request(config, Method::Get, OTA_PATH, None)?;
    if !response.is_success() {
        anyhow::bail!("OTA check returned HTTP {}", response.status);
    }
    Ok(parse_check_response(&response.body)?)
}

/// Reports job progress and returns the HTTP status (409: the job was cancelled or
/// already finished), or `None` when the request could not be sent.
pub fn report(config: &DeviceConfig, status: &StatusReport<'_>) -> Option<u16> {
    match request(
        config,
        Method::Post,
        OTA_STATUS_PATH,
        Some(&status.to_json()),
    ) {
        Ok(response) => {
            if !response.is_success() {
                log::warn!("OTA status report rejected: HTTP {}", response.status);
            }
            Some(response.status)
        }
        Err(error) => {
            log::warn!("OTA status report failed: {error:#}");
            None
        }
    }
}

/// Streams the image into the inactive slot, verifies it and sets it as the boot slot.
/// The caller reboots afterwards. `on_progress` receives 0–100 at 10 % steps.
pub fn install(job: &OtaJob, on_progress: &mut dyn FnMut(u8)) -> anyhow::Result<()> {
    let mut client: HttpClient<_> = http_client(30)?;
    let mut response = client.get(&job.url)?.submit()?;
    if response.status() != 200 {
        anyhow::bail!("image download returned HTTP {}", response.status());
    }

    let mut ota = EspOta::new()?;
    let mut update = ota.initiate_update_with_known_size(job.size as usize)?;
    let mut verifier = ImageVerifier::new(job);
    let mut throttle = ProgressThrottle::default();
    let mut buffer = vec![0u8; 4096];
    loop {
        let read = response.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let written = verifier
            .update(&buffer[..read])
            .map_err(|error| anyhow::anyhow!("image verify: {error:?}"))?;
        update.write(&buffer[..read])?;
        let percent = ProgressThrottle::percent(written, job.size);
        if throttle.should_report(percent) {
            on_progress(percent);
        }
    }
    // Dropping `update` on any error above aborts the OTA and leaves the boot slot unchanged.
    verifier
        .finish()
        .map_err(|error| anyhow::anyhow!("image verify: {error:?}"))?;
    update.complete()?;
    Ok(())
}

pub fn restart() -> ! {
    unsafe { sys::esp_restart() }
}
