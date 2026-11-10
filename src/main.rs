use std::time::Instant;

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{delay::FreeRtos, peripherals::Peripherals},
    nvs::EspDefaultNvsPartition,
    sntp::EspSntp,
};

use device_rust::{
    ble::{
        esp::NimbleTransport, BleEvent, BleTransport, Command, FrameError, PairingAction,
        PairingWindow,
    },
    board::Board,
    buttons::{ButtonEvent, ButtonSource, CombinedButtons},
    config::{mock_provisioning, wipe_config_on_boot, DeviceConfig},
    devlog,
    display::{
        format_passkey, DisplayPort, OledDisplay, Overlay, Screen, ScreenModel, SELECTED_CONTROLLER,
    },
    event,
    ota::{self, JobStatus, OtaDecision, StatusReport, VERIFY_DEADLINE_MS},
    sensors::{
        esp::SharedI2c,
        readings,
        schedule::{PmsAction, PmsDutyCycle},
        I2cBus, SensorReadings, Sht30, OLED_ADDRESSES, SHT30_ADDRESSES,
    },
    storage::{esp::NvsConfigStore, ConfigStore},
    telemetry::{
        esp::EspTelemetrySink, live_reading_json, Aggregator, TelemetrySink, UploadResult,
    },
    wifi::{esp::EspWifiPort, WifiPort},
    CLIMATE_INTERVAL_MS, FIRMWARE_VERSION, UPLOAD_INTERVAL_MS,
};

type Display = Option<OledDisplay<SharedI2c>>;

/// Mutable state shared by the main loop, the renderer and the BLE handler.
struct State {
    screen: Screen,
    latest: Option<SensorReadings>,
    last_upload: Option<(UploadResult, u64)>,
    pairing: PairingWindow,
    live: bool,
    ota_progress: Option<u8>,
}

/// Side effects the BLE command handler asks the main loop to perform.
enum BleOutcome {
    Nothing,
    Saved,
    Cleared,
    Live(bool),
    OtaCheck,
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    devlog::esp::init();
    log::info!("AirMonitor Rust firmware {FIRMWARE_VERSION} booting");

    let peripherals = Peripherals::take()?;
    let event_loop = EspSystemEventLoop::take()?;
    let nvs_partition = EspDefaultNvsPartition::take()?;
    devlog::esp::record_boot(&nvs_partition, FIRMWARE_VERSION);
    let mut pending_verify = ota::esp::running_image_pending_verify();
    if pending_verify {
        event!("ota", "image {FIRMWARE_VERSION} pending verification");
    }

    let store = NvsConfigStore::new(nvs_partition.clone())?;
    if wipe_config_on_boot() {
        log::warn!("WIPE_CONFIG_ON_BOOT=1; clearing saved provisioning");
        store.clear()?;
    }
    let mut device_config = store.load()?;
    if !device_config.has_wifi() {
        if let Some(mock) = mock_provisioning() {
            log::warn!("Seeding empty NVS from development mock provisioning");
            store.save(&mock)?;
            device_config = mock;
        }
    }
    log::info!("Loaded config: {}", device_config.redacted_summary());

    let Board {
        shared_i2c,
        mut pms5003,
        buttons: gpio_buttons,
    } = Board::new(peripherals.pins, peripherals.i2c0, peripherals.uart1)?;
    let mut buttons = CombinedButtons {
        first: gpio_buttons,
        second: serial_buttons(),
    };
    let addresses: Vec<u8> = (0x08..=0x77)
        .filter(|address| shared_i2c.probe(*address))
        .collect();
    log::info!("I2C devices: {:02x?}", addresses);
    let sht30 = unique_candidate(&addresses, &SHT30_ADDRESSES, "SHT30")
        .map(|address| Sht30::new(shared_i2c.clone(), address));
    let mut display: Display =
        unique_candidate(&addresses, &OLED_ADDRESSES, "OLED").and_then(|address| {
            let oled = OledDisplay::new(shared_i2c.clone(), address, SELECTED_CONTROLLER);
            match oled.init() {
                Ok(()) => Some(oled),
                Err(error) => {
                    log::error!("OLED initialization failed: {error:?}");
                    None
                }
            }
        });

    let mut wifi = EspWifiPort::new(peripherals.modem, event_loop, nvs_partition)?;
    match wifi.connect(&device_config) {
        Ok(true) => event!(
            "wifi",
            "connected ip={}",
            wifi.status().ip.unwrap_or_default()
        ),
        Ok(false) => log::info!("WiFi not provisioned"),
        Err(error) => log::warn!(target: "event::wifi", "initial connection failed: {error:#}"),
    }
    let _sntp = match EspSntp::new_default() {
        Ok(sntp) => Some(sntp),
        Err(error) => {
            log::warn!("SNTP unavailable: {error:?}");
            None
        }
    };
    let mac = efuse_mac();
    let mut ble = NimbleTransport::start(&format!("AirMonitor-{}", &mac[8..]))?;

    let mut telemetry = EspTelemetrySink;
    let mut aggregator = Aggregator::default();
    let boot = Instant::now();
    let mut state = State {
        screen: Screen::Aqi,
        latest: None,
        last_upload: None,
        pairing: PairingWindow::default(),
        live: false,
        ota_progress: None,
    };
    let mut last_climate_ms = 0;
    let mut latest_pm = None;
    // Board::new leaves the PMS5003 running, which is how every cycle starts.
    let mut pms_duty = PmsDutyCycle::new_awake(0);
    let mut burst_frames: u32 = 0;
    let mut last_upload_tick_ms = 0;
    let mut last_render_ms = 0;
    let mut boot_logs_sent = false;
    let mut ota_check_requested = false;
    render(
        &mut display,
        &state,
        &wifi,
        &device_config,
        elapsed_ms(boot),
    );

    loop {
        let now_ms = elapsed_ms(boot);
        let mut dirty = false;

        while let Some(ble_event) = ble.poll_event() {
            dirty = true;
            match ble_event {
                BleEvent::Command(command) => {
                    state.pairing.on_activity(now_ms);
                    match handle_ble(
                        command,
                        &mut ble,
                        &store,
                        &mut wifi,
                        &mut device_config,
                        &state,
                        &mac,
                        now_ms,
                    ) {
                        BleOutcome::Nothing => {}
                        BleOutcome::Saved => {
                            event!(
                                "config",
                                "provisioning saved (wifi={})",
                                wifi.status().connected
                            );
                            // Upload (and confirm the backend sees the device) as soon as a sample exists.
                            last_upload_tick_ms = 0;
                            boot_logs_sent = false;
                        }
                        BleOutcome::Cleared => {
                            event!("config", "factory reset over BLE");
                            if let Err(error) = ble.delete_bonds() {
                                log::warn!("BLE bonds not cleared: {error:#}");
                            }
                        }
                        BleOutcome::Live(enabled) => state.live = enabled,
                        BleOutcome::OtaCheck => ota_check_requested = true,
                    }
                }
                BleEvent::Connected => {
                    state.live = false;
                    apply_pairing(&mut ble, state.pairing.on_connected(now_ms));
                    event!("ble", "phone connected");
                }
                BleEvent::Disconnected => {
                    state.live = false;
                    apply_pairing(&mut ble, state.pairing.on_disconnected(now_ms));
                }
                BleEvent::PasskeyDisplayed(passkey) => {
                    state.pairing.on_passkey(passkey);
                    // Console only: the passkey must never be uploaded with device logs.
                    println!("BLE PAIRING PASSKEY: {}", format_passkey(passkey));
                    event!("ble", "pairing started");
                }
                BleEvent::PairingFinished { ok } => {
                    state.pairing.on_pairing_finished();
                    if ok {
                        event!("ble", "pairing succeeded");
                    } else {
                        log::warn!(target: "event::ble", "pairing failed");
                    }
                }
            }
        }

        if let Some(action) = state.pairing.tick(now_ms) {
            apply_pairing(&mut ble, Some(action));
            state.live = false;
            event!("ble", "pairing window closed");
            dirty = true;
        }

        if let Some(button) = buttons.poll(now_ms) {
            match button {
                ButtonEvent::Right => state.screen = state.screen.next(),
                ButtonEvent::Left => state.screen = state.screen.previous(),
                ButtonEvent::PairHold => {
                    apply_pairing(&mut ble, state.pairing.open(now_ms));
                    event!("ble", "pairing window opened");
                }
            }
            dirty = true;
        }

        // PM: rest the sensor between bursts and average only settled frames.
        match pms_duty.tick(now_ms) {
            PmsAction::Wake => {
                if let Err(error) = pms5003.wake() {
                    log::warn!("PMS5003 wake failed: {error:?}");
                }
                log::info!("PMS5003 awake, warming up");
            }
            PmsAction::Sleep => {
                if let Err(error) = pms5003.sleep() {
                    log::warn!("PMS5003 sleep failed: {error:?}");
                }
                log::info!("PMS5003 asleep after {burst_frames} frames");
                if burst_frames == 0 {
                    // Don't keep showing a reading from an earlier burst.
                    latest_pm = None;
                    log::warn!(target: "event::sensor", "PMS5003 sent no frames in a burst");
                }
                burst_frames = 0;
            }
            PmsAction::Nothing => {}
        }
        if pms_duty.is_awake() {
            match pms5003.poll() {
                Ok(Some(pm)) if pms_duty.counts_frames(now_ms) => {
                    aggregator.add_pm(&pm);
                    latest_pm = Some(pm);
                    burst_frames += 1;
                }
                // Nothing yet, or the fan is still settling.
                Ok(_) => {}
                Err(error) => log::warn!("PMS5003 read failed: {error:?}"),
            }
        }

        if now_ms.saturating_sub(last_climate_ms) >= CLIMATE_INTERVAL_MS {
            last_climate_ms = now_ms;
            // PM from the latest burst; it stays on screen while the sensor rests.
            let pm = latest_pm.clone();
            let climate = match &sht30 {
                Some(sensor) => match sensor.read() {
                    Ok(value) => Some(value),
                    Err(error) => {
                        log::warn!("SHT30 read failed: {error:?}");
                        None
                    }
                },
                None => None,
            };
            if let Some(climate) = &climate {
                aggregator.add_climate(climate);
            }
            let current = readings(pm, climate);
            if state.live && state.pairing.is_connected() {
                send(
                    &mut ble,
                    &format!("READING:{}", live_reading_json(&current, now_ms)),
                );
            }
            state.latest = Some(current);
            dirty = true;
        }

        // Only while the sensor rests, so a burst is never split across uploads.
        let upload_due = !pms_duty.is_awake()
            && aggregator.complete()
            && (last_upload_tick_ms == 0
                || now_ms.saturating_sub(last_upload_tick_ms) >= UPLOAD_INTERVAL_MS);
        if upload_due {
            last_upload_tick_ms = now_ms.max(1);
            if !wifi.status().connected && device_config.has_wifi() {
                match wifi.connect(&device_config) {
                    Ok(true) => event!("wifi", "reconnected"),
                    Ok(false) => {}
                    Err(error) => log::warn!(target: "event::wifi", "reconnect failed: {error:#}"),
                }
            }
            let result = if !wifi.status().connected {
                UploadResult::default()
            } else if let Some(mean) = aggregator.mean() {
                match telemetry.upload(&device_config, &mean) {
                    Ok(result) => result,
                    Err(error) => {
                        log::error!("Upload failed: {error:#}");
                        UploadResult {
                            ok: false,
                            http_code: -1,
                        }
                    }
                }
            } else {
                UploadResult::default()
            };
            log::info!("Upload result: {result:?}");
            if !result.ok && device_config.has_device_credentials() && wifi.status().connected {
                log::warn!(target: "event::upload", "upload failed http={}", result.http_code);
            }
            state.last_upload = Some((result, now_ms));
            aggregator.reset();
            if result.ok {
                if pending_verify {
                    match ota::esp::mark_running_valid() {
                        Ok(()) => {
                            pending_verify = false;
                            event!("ota", "image {FIRMWARE_VERSION} verified");
                        }
                        Err(error) => log::error!("OTA mark valid failed: {error:#}"),
                    }
                }
                devlog::esp::flush(&device_config);
                boot_logs_sent = true;
                ota_check_requested = true;
            }
            dirty = true;
        }

        if !boot_logs_sent && wifi.status().connected && device_config.has_device_credentials() {
            boot_logs_sent = true;
            devlog::esp::flush(&device_config);
        }

        if ota_check_requested && !pending_verify && wifi.status().connected {
            ota_check_requested = false;
            run_ota_cycle(
                &device_config,
                &mut display,
                &mut state,
                &wifi,
                &mut ble,
                now_ms,
            );
            dirty = true;
        }

        if pending_verify && now_ms >= VERIFY_DEADLINE_MS {
            log::error!(target: "event::ota", "image {FIRMWARE_VERSION} not verified in time; rolling back");
            devlog::esp::flush(&device_config);
            let error = ota::esp::rollback_and_reboot();
            log::error!("OTA rollback failed: {error:#}");
            pending_verify = false;
        }

        // Keep the pairing countdown ticking on screen.
        if state.pairing.is_open() && now_ms.saturating_sub(last_render_ms) >= 1_000 {
            dirty = true;
        }
        if dirty {
            last_render_ms = now_ms;
            render(&mut display, &state, &wifi, &device_config, now_ms);
        }
        FreeRtos::delay_ms(50);
    }
}

#[cfg(feature = "serial-buttons")]
fn serial_buttons() -> Box<dyn ButtonSource> {
    match device_rust::buttons::serial::SerialButtons::start() {
        Ok(source) => Box::new(source),
        Err(error) => {
            log::warn!("serial buttons unavailable: {error:#}");
            Box::new(device_rust::buttons::NoButtons)
        }
    }
}

#[cfg(not(feature = "serial-buttons"))]
fn serial_buttons() -> Box<dyn ButtonSource> {
    Box::new(device_rust::buttons::NoButtons)
}

fn apply_pairing(ble: &mut NimbleTransport, action: Option<PairingAction>) {
    match action {
        Some(PairingAction::StartAdvertising) => match ble.start_advertising() {
            Ok(_) => log::info!("BLE advertising started (pairing mode)"),
            Err(error) => log::error!("BLE advertising failed: {error:#}"),
        },
        Some(PairingAction::StopAdvertising) => ble.stop_advertising(),
        Some(PairingAction::Close) => {
            ble.stop_advertising();
            ble.disconnect();
        }
        None => {}
    }
}

fn run_ota_cycle(
    config: &DeviceConfig,
    display: &mut Display,
    state: &mut State,
    wifi: &EspWifiPort<'_>,
    ble: &mut NimbleTransport,
    now_ms: u64,
) {
    let job = match ota::esp::check(config) {
        Ok(job) => job,
        Err(error) => {
            log::warn!("OTA check failed: {error:#}");
            return;
        }
    };
    let connected = state.pairing.is_connected();
    match ota::decide(job, FIRMWARE_VERSION) {
        OtaDecision::Nothing => {
            if connected {
                send(ble, "OTA:NONE");
            }
        }
        OtaDecision::ReportSucceeded(job) => {
            event!("ota", "update to {} succeeded", job.version);
            ota::esp::report(
                config,
                &report(&job.job_id, JobStatus::Succeeded, Some(100), None),
            );
        }
        OtaDecision::ReportRolledBack(job) => {
            log::error!(target: "event::ota", "update to {} rolled back", job.version);
            let error = format!("rolled back to {FIRMWARE_VERSION}");
            ota::esp::report(
                config,
                &report(&job.job_id, JobStatus::Failed, None, Some(error)),
            );
        }
        OtaDecision::Install(job) => {
            event!("ota", "installing {} ({} bytes)", job.version, job.size);
            if connected {
                send(ble, &format!("OTA:STARTING {}", job.version));
            }
            let accepted = ota::esp::report(
                config,
                &report(&job.job_id, JobStatus::Downloading, Some(0), None),
            );
            if accepted == Some(409) {
                log::warn!(target: "event::ota", "update to {} was cancelled", job.version);
                return;
            }
            state.ota_progress = Some(0);
            render(display, state, wifi, config, now_ms);
            let result = ota::esp::install(&job, &mut |percent| {
                state.ota_progress = Some(percent);
                render(display, state, wifi, config, now_ms);
                ota::esp::report(
                    config,
                    &report(&job.job_id, JobStatus::Downloading, Some(percent), None),
                );
            });
            match result {
                Ok(()) => {
                    event!("ota", "image {} activated; rebooting", job.version);
                    ota::esp::report(
                        config,
                        &report(&job.job_id, JobStatus::Installing, Some(100), None),
                    );
                    devlog::esp::flush(config);
                    ota::esp::restart();
                }
                Err(error) => {
                    state.ota_progress = None;
                    log::error!(target: "event::ota", "update to {} failed: {error:#}", job.version);
                    let message = format!("{error:#}");
                    ota::esp::report(
                        config,
                        &report(&job.job_id, JobStatus::Failed, None, Some(message)),
                    );
                    devlog::esp::flush(config);
                    if connected {
                        send(ble, "OTA:FAILED");
                    }
                }
            }
        }
    }
}

fn report(
    job_id: &str,
    status: JobStatus,
    progress: Option<u8>,
    error: Option<String>,
) -> StatusReport<'_> {
    StatusReport {
        job_id,
        status,
        progress,
        error,
        version: FIRMWARE_VERSION,
    }
}

fn unique_candidate(addresses: &[u8], candidates: &[u8], name: &str) -> Option<u8> {
    let found: Vec<_> = addresses
        .iter()
        .copied()
        .filter(|address| candidates.contains(address))
        .collect();
    match found.as_slice() {
        [address] => {
            log::info!("Discovered {name} at 0x{address:02x}");
            Some(*address)
        }
        [] => {
            log::error!("{name} was not found");
            None
        }
        _ => {
            log::error!("Multiple {name} candidates found: {found:02x?}");
            None
        }
    }
}

fn elapsed_ms(boot: Instant) -> u64 {
    boot.elapsed().as_millis() as u64
}

/// Factory MAC as 12 uppercase hex digits; the BLE name uses the last four.
fn efuse_mac() -> String {
    let mut mac = [0u8; 6];
    let _ = unsafe { esp_idf_svc::sys::esp_efuse_mac_get_default(mac.as_mut_ptr()) };
    mac.iter().map(|byte| format!("{byte:02X}")).collect()
}

fn render(
    display: &mut Display,
    state: &State,
    wifi: &EspWifiPort<'_>,
    config: &DeviceConfig,
    now_ms: u64,
) {
    let Some(display) = display else {
        return;
    };
    let overlay = match (state.ota_progress, state.pairing.is_open()) {
        (Some(percent), _) => Some(Overlay::Updating { percent }),
        (None, true) => Some(Overlay::Pairing {
            remaining_s: state.pairing.remaining_ms(now_ms) / 1_000,
            passkey: state.pairing.passkey(),
            connected: state.pairing.is_connected(),
        }),
        (None, false) => None,
    };
    let status = wifi.status();
    if let Err(error) = display.show(&ScreenModel {
        screen: state.screen,
        readings: state.latest.as_ref(),
        wifi: &status,
        last_upload: state.last_upload,
        now_ms,
        overlay,
        provisioned: config.has_wifi() && config.has_device_credentials(),
    }) {
        log::error!("OLED update failed: {error:?}");
    }
}

fn send<B: BleTransport>(ble: &mut B, message: &str)
where
    B::Error: core::fmt::Debug,
{
    if let Err(error) = ble.send_line(message) {
        log::warn!("BLE send failed: {error:?}");
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_ble<B, S, W>(
    command: Command,
    ble: &mut B,
    store: &S,
    wifi: &mut W,
    config: &mut DeviceConfig,
    state: &State,
    mac: &str,
    now_ms: u64,
) -> BleOutcome
where
    B: BleTransport,
    B::Error: core::fmt::Debug,
    S: ConfigStore,
    S::Error: core::fmt::Debug,
    W: WifiPort,
    W::Error: core::fmt::Debug,
{
    let mut send = |message: &str| send(ble, message);
    match command {
        Command::Ping => send("PONG"),
        Command::Status => {
            let status = wifi.status();
            send(&format!(
                "WiFi: {} IP: {}",
                if status.connected {
                    "connected"
                } else {
                    "disconnected"
                },
                status.ip.as_deref().unwrap_or("0.0.0.0")
            ));
        }
        Command::Info => {
            let status = wifi.status();
            send("INFO_BEGIN");
            send(&format!("FW:{FIRMWARE_VERSION}"));
            send(&format!("MAC:{mac}"));
            send(&format!("DEVID:{}", config.device_id));
            send(&format!(
                "DEVKEY_SET:{}",
                u8::from(!config.device_key.is_empty())
            ));
            send(&format!(
                "WIFI:{}",
                if status.connected {
                    "connected"
                } else {
                    "disconnected"
                }
            ));
            send(&format!("SSID:{}", config.ssid));
            send(&format!("IP:{}", status.ip.as_deref().unwrap_or("0.0.0.0")));
            match state.last_upload {
                Some((result, at)) => send(&format!(
                    "LAST_UPLOAD:{},{}",
                    result.http_code,
                    now_ms.saturating_sub(at) / 1_000
                )),
                None => send("LAST_UPLOAD:none"),
            }
            send("INFO_END");
        }
        Command::Scan => {
            send("SCAN_BEGIN");
            match wifi.scan() {
                Ok(networks) => {
                    for network in networks {
                        send(&network.ble_line());
                    }
                }
                Err(error) => {
                    log::warn!("WiFi scan failed: {error:?}");
                    send("ERROR:SCAN");
                }
            }
            send("SCAN_END");
        }
        Command::Read => match &state.latest {
            Some(latest) => send(&format!("READING:{}", live_reading_json(latest, now_ms))),
            None => send("ERROR:NO_READING"),
        },
        Command::Live(enabled) => {
            send(if enabled {
                "OK: live on"
            } else {
                "OK: live off"
            });
            return BleOutcome::Live(enabled);
        }
        Command::OtaCheck => {
            send("OK: ota check");
            return BleOutcome::OtaCheck;
        }
        Command::SetSsid(value) => {
            config.ssid = value;
            send(&format!("OK: SSID set to {}", config.ssid));
        }
        Command::SetPassword(value) => {
            config.password = value;
            send("OK: password set");
        }
        Command::SetLatitude(value) => {
            config.lat = value;
            send(&format!("OK: LAT set to {:.6}", config.lat));
        }
        Command::SetLongitude(value) => {
            config.lon = value;
            send(&format!("OK: LON set to {:.6}", config.lon));
        }
        Command::SetDeviceId(value) => {
            config.device_id = value;
            send(&format!("OK: DEVID set to {}", config.device_id));
        }
        Command::SetDeviceKey(value) => {
            config.device_key = value;
            send("OK: device key set");
        }
        Command::Save => {
            send("SAVE_BEGIN");
            let mut saved = false;
            match store.save(config) {
                Ok(()) => {
                    saved = true;
                    send("WIFI_CONNECTING");
                    match wifi.connect(config) {
                        Ok(true) => {
                            send("SAVE_OK");
                            send(&format!(
                                "IP:{}",
                                wifi.status().ip.as_deref().unwrap_or("0.0.0.0")
                            ));
                        }
                        _ => send("SAVE_FAILED:WIFI"),
                    }
                }
                Err(error) => {
                    log::error!("Config save failed: {error:?}");
                    send("SAVE_FAILED:STORAGE");
                }
            }
            send("SAVE_END");
            if saved {
                return BleOutcome::Saved;
            }
        }
        Command::Load => {
            send("LOAD_BEGIN");
            send(&format!("SSID:{}", config.ssid));
            send(&format!(
                "PASS_SET:{}",
                u8::from(!config.password.is_empty())
            ));
            send(&format!("LAT:{:.6}", config.lat));
            send(&format!("LON:{:.6}", config.lon));
            send(&format!("DEVID:{}", config.device_id));
            send(&format!(
                "DEVKEY_SET:{}",
                u8::from(!config.device_key.is_empty())
            ));
            send("LOAD_END");
        }
        Command::Clear => match store.clear() {
            Ok(()) => {
                *config = DeviceConfig::default();
                send("CLEARED all stored config");
                return BleOutcome::Cleared;
            }
            Err(error) => {
                log::error!("Config clear failed: {error:?}");
                send("ERROR:STORAGE");
            }
        },
        Command::ProtocolError(FrameError::TooLong) => send("ERROR:COMMAND_TOO_LONG"),
        Command::ProtocolError(FrameError::Utf8) => send("ERROR:INVALID_UTF8"),
        Command::Unknown(value) => send(&format!("echo: {value}")),
    }
    BleOutcome::Nothing
}
