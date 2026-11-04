use std::sync::{atomic::AtomicU32, OnceLock};

use esp_idf_svc::{
    log::EspIdfLogger,
    nvs::{EspDefaultNvsPartition, EspNvs},
    sys,
};

use crate::{
    config::DeviceConfig,
    device_api::{esp::request, Method, LOGS_PATH},
    devlog::{CaptureLogger, BUFFER},
};

static CONSOLE: EspIdfLogger = EspIdfLogger::new(());
static LOGGER: OnceLock<CaptureLogger> = OnceLock::new();

fn uptime_ms() -> u64 {
    (unsafe { sys::esp_timer_get_time() } / 1_000) as u64
}

/// Installs the capturing logger. Call first thing in `main`.
pub fn init() {
    let logger = LOGGER.get_or_init(|| CaptureLogger {
        inner: &CONSOLE,
        uptime_ms,
        boot_count: AtomicU32::new(0),
    });
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

/// Increments the persistent boot counter (NVS namespace `sys`, separate from the
/// provisioning namespace so `CLEAR` does not reset it) and records the boot event.
pub fn record_boot(partition: &EspDefaultNvsPartition, firmware_version: &str) {
    let boot_count = match EspNvs::new(partition.clone(), "sys", true) {
        Ok(nvs) => {
            let next = nvs
                .get_u32("boots")
                .ok()
                .flatten()
                .unwrap_or(0)
                .wrapping_add(1);
            if let Err(error) = nvs.set_u32("boots", next) {
                log::warn!("boot counter not saved: {error:?}");
            }
            next
        }
        Err(error) => {
            log::warn!("boot counter unavailable: {error:?}");
            0
        }
    };
    if let Some(logger) = LOGGER.get() {
        logger
            .boot_count
            .store(boot_count, std::sync::atomic::Ordering::Relaxed);
    }
    let reason = reset_reason();
    if matches!(
        reason,
        "panic" | "brownout" | "task_watchdog" | "interrupt_watchdog" | "watchdog"
    ) {
        log::error!(target: "event::boot", "boot #{boot_count} fw={firmware_version} reset={reason}");
    } else {
        crate::event!(
            "boot",
            "boot #{boot_count} fw={firmware_version} reset={reason}"
        );
    }
}

fn reset_reason() -> &'static str {
    #[allow(non_upper_case_globals)]
    match unsafe { sys::esp_reset_reason() } {
        sys::esp_reset_reason_t_ESP_RST_POWERON => "power_on",
        sys::esp_reset_reason_t_ESP_RST_EXT => "external",
        sys::esp_reset_reason_t_ESP_RST_SW => "software",
        sys::esp_reset_reason_t_ESP_RST_PANIC => "panic",
        sys::esp_reset_reason_t_ESP_RST_INT_WDT => "interrupt_watchdog",
        sys::esp_reset_reason_t_ESP_RST_TASK_WDT => "task_watchdog",
        sys::esp_reset_reason_t_ESP_RST_WDT => "watchdog",
        sys::esp_reset_reason_t_ESP_RST_DEEPSLEEP => "deep_sleep",
        sys::esp_reset_reason_t_ESP_RST_BROWNOUT => "brownout",
        sys::esp_reset_reason_t_ESP_RST_USB => "usb",
        sys::esp_reset_reason_t_ESP_RST_JTAG => "jtag",
        _ => "unknown",
    }
}

/// Uploads buffered entries in batches; entries are only removed after a 2xx response.
pub fn flush(config: &DeviceConfig) {
    for _ in 0..3 {
        let batch = match BUFFER.lock() {
            Ok(buffer) => buffer.batch_payload(),
            Err(_) => return,
        };
        let Some((count, payload)) = batch else {
            return;
        };
        match request(config, Method::Post, LOGS_PATH, Some(&payload)) {
            Ok(response) if response.is_success() => {
                if let Ok(mut buffer) = BUFFER.lock() {
                    buffer.acknowledge(count);
                }
            }
            // Use println! rather than log: a failed log upload must not log itself forever.
            Ok(response) => {
                println!("devlog upload rejected: HTTP {}", response.status);
                return;
            }
            Err(error) => {
                println!("devlog upload failed: {error:#}");
                return;
            }
        }
    }
}
