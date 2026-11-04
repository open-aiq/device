use std::time::Instant;

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{delay::FreeRtos, peripherals::Peripherals},
    log::EspLogger,
    nvs::EspDefaultNvsPartition,
};

use device_rust::{
    ble::{esp::NimbleTransport, BleTransport, Command, FrameError},
    board::Board,
    buttons::{ButtonEvent, ButtonSource},
    config::{mock_provisioning, wipe_config_on_boot, DeviceConfig},
    display::{DisplayPort, OledDisplay, Screen, ScreenModel, SELECTED_CONTROLLER},
    sensors::{esp::SharedI2c, readings, I2cBus, Sht30, OLED_ADDRESSES, SHT30_ADDRESSES},
    storage::{esp::NvsConfigStore, ConfigStore},
    telemetry::{esp::EspTelemetrySink, Aggregator, TelemetrySink, UploadResult},
    wifi::{esp::EspWifiPort, WifiPort},
    SENSOR_INTERVAL_MS, UPLOAD_INTERVAL_MS,
};

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    EspLogger::initialize_default();
    log::info!("AirMonitor Rust firmware booting");

    let peripherals = Peripherals::take()?;
    let event_loop = EspSystemEventLoop::take()?;
    let nvs_partition = EspDefaultNvsPartition::take()?;
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
        mut buttons,
    } = Board::new(peripherals.pins, peripherals.i2c0, peripherals.uart1)?;
    let addresses: Vec<u8> = (0x08..=0x77)
        .filter(|address| shared_i2c.probe(*address))
        .collect();
    log::info!("I2C devices: {:02x?}", addresses);
    let sht30 = unique_candidate(&addresses, &SHT30_ADDRESSES, "SHT30")
        .map(|address| Sht30::new(shared_i2c.clone(), address));
    let mut display = unique_candidate(&addresses, &OLED_ADDRESSES, "OLED").and_then(|address| {
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
    if let Err(error) = wifi.connect(&device_config) {
        log::warn!("Initial WiFi connection failed: {error:#}");
    }
    let mut ble = NimbleTransport::start(&ble_device_name())?;

    let mut telemetry = EspTelemetrySink;
    let mut screen = Screen::Aqi;
    let mut latest = None;
    let mut aggregator = Aggregator::default();
    let boot = Instant::now();
    let mut last_sample_ms = 0;
    let mut last_upload_tick_ms = 0;
    let mut last_upload: Option<(UploadResult, u64)> = None;
    render(
        &mut display,
        screen,
        latest.as_ref(),
        &wifi,
        last_upload,
        elapsed_ms(boot),
    );

    loop {
        let now_ms = elapsed_ms(boot);
        while let Some(command) = ble.poll_command() {
            handle_ble(command, &mut ble, &store, &mut wifi, &mut device_config);
        }
        if let Some(event) = buttons.poll(now_ms) {
            screen = match event {
                ButtonEvent::Right => screen.next(),
                ButtonEvent::Left => screen.previous(),
            };
            render(
                &mut display,
                screen,
                latest.as_ref(),
                &wifi,
                last_upload,
                now_ms,
            );
        }
        if now_ms.saturating_sub(last_sample_ms) >= SENSOR_INTERVAL_MS {
            last_sample_ms = now_ms;
            let pm = match pms5003.read() {
                Ok(value) => Some(value),
                Err(error) => {
                    log::warn!("PMS5003 read failed: {error:?}");
                    None
                }
            };
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
            let current = readings(pm, climate);
            aggregator.add(&current);
            latest = Some(current);
            render(
                &mut display,
                screen,
                latest.as_ref(),
                &wifi,
                last_upload,
                now_ms,
            );
        }
        let upload_due = aggregator.complete()
            && (last_upload_tick_ms == 0
                || now_ms.saturating_sub(last_upload_tick_ms) >= UPLOAD_INTERVAL_MS);
        if upload_due {
            last_upload_tick_ms = now_ms;
            if !wifi.status().connected {
                if let Err(error) = wifi.connect(&device_config) {
                    log::warn!("WiFi reconnect failed: {error:#}");
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
            last_upload = Some((result, now_ms));
            aggregator.reset();
            render(
                &mut display,
                screen,
                latest.as_ref(),
                &wifi,
                last_upload,
                now_ms,
            );
        }
        FreeRtos::delay_ms(50);
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

fn ble_device_name() -> String {
    let mut mac = [0u8; 6];
    let _ = unsafe { esp_idf_svc::sys::esp_efuse_mac_get_default(mac.as_mut_ptr()) };
    format!("AirMonitor-{:02X}{:02X}", mac[4], mac[5])
}

fn render(
    display: &mut Option<OledDisplay<SharedI2c>>,
    screen: Screen,
    readings: Option<&device_rust::sensors::SensorReadings>,
    wifi: &EspWifiPort<'_>,
    last_upload: Option<(UploadResult, u64)>,
    now_ms: u64,
) {
    if let Some(display) = display {
        let status = wifi.status();
        if let Err(error) = display.show(&ScreenModel {
            screen,
            readings,
            wifi: &status,
            last_upload,
            now_ms,
        }) {
            log::error!("OLED update failed: {error:?}");
        }
    }
}

fn handle_ble<B, S, W>(
    command: Command,
    ble: &mut B,
    store: &S,
    wifi: &mut W,
    config: &mut DeviceConfig,
) where
    B: BleTransport,
    B::Error: core::fmt::Debug,
    S: ConfigStore,
    S::Error: core::fmt::Debug,
    W: WifiPort,
    W::Error: core::fmt::Debug,
{
    let mut send = |message: &str| {
        if let Err(error) = ble.send_line(message) {
            log::warn!("BLE send failed: {error:?}");
        }
    };
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
            match store.save(config) {
                Ok(()) => {
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
}
