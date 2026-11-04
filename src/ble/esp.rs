use std::sync::{
    atomic::{AtomicU16, AtomicU32, Ordering},
    mpsc, Arc, Mutex,
};

use esp32_nimble::{
    enums::{AuthReq, SecurityIOCap},
    uuid128, BLEAdvertisementData, BLEDevice, NimbleProperties,
};
use esp_idf_svc::sys;

use crate::ble::{
    notification_chunks, BleEvent, BleTransport, Command, CommandFramer, PREFERRED_MTU,
};

type NotifyFn = dyn Fn(&[u8]) + Send;

const NO_CONNECTION: u16 = u16::MAX;

/// Nordic UART Service transport protected by LE Secure Connections.
///
/// RX/TX require an encrypted, MITM-authenticated link, so a phone must complete
/// passkey pairing (6 digits shown on the OLED/serial log) before any command is
/// accepted. Advertising is off until [`NimbleTransport::start_advertising`] is called.
pub struct NimbleTransport {
    receiver: mpsc::Receiver<BleEvent>,
    notify: Box<NotifyFn>,
    passkey: Arc<AtomicU32>,
    conn_handle: Arc<AtomicU16>,
}

impl NimbleTransport {
    pub fn start(device_name: &str) -> anyhow::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let framer = Arc::new(Mutex::new(CommandFramer::default()));
        let passkey = Arc::new(AtomicU32::new(new_passkey()));
        let conn_handle = Arc::new(AtomicU16::new(NO_CONNECTION));
        let ble_device = BLEDevice::take();

        ble_device
            .security()
            .set_auth(AuthReq::Bond | AuthReq::Mitm | AuthReq::Sc)
            .set_io_cap(SecurityIOCap::DisplayOnly)
            .resolve_rpa();
        if let Err(error) = ble_device.set_preferred_mtu(PREFERRED_MTU) {
            log::warn!("BLE preferred MTU not applied: {error:?}");
        }

        let server = ble_device.get_server();
        server.advertise_on_disconnect(false);

        let connect_sender = sender.clone();
        let connect_handle = conn_handle.clone();
        server.on_connect(move |_, description| {
            log::info!("BLE connected: {:?}", description.address());
            connect_handle.store(description.conn_handle(), Ordering::Relaxed);
            let _ = connect_sender.send(BleEvent::Connected);
        });

        let disconnect_framer = framer.clone();
        let disconnect_sender = sender.clone();
        let disconnect_handle = conn_handle.clone();
        server.on_disconnect(move |_description, reason| {
            log::info!("BLE disconnected: {reason:?}");
            disconnect_handle.store(NO_CONNECTION, Ordering::Relaxed);
            if let Ok(mut framer) = disconnect_framer.lock() {
                framer.clear();
            }
            let _ = disconnect_sender.send(BleEvent::Disconnected);
        });

        let passkey_sender = sender.clone();
        let passkey_value = passkey.clone();
        server.on_passkey_request(move || {
            let value = passkey_value.load(Ordering::Relaxed);
            let _ = passkey_sender.send(BleEvent::PasskeyDisplayed(value));
            value
        });

        let auth_sender = sender.clone();
        server.on_authentication_complete(move |_, description, result| {
            let ok = result.is_ok() && description.authenticated() && description.encrypted();
            log::info!(
                "BLE pairing complete: ok={ok} bonded={} result={result:?}",
                description.bonded()
            );
            let _ = auth_sender.send(BleEvent::PairingFinished { ok });
        });

        let service = server.create_service(uuid128!("6e400001-b5a3-f393-e0a9-e50e24dcca9e"));
        let tx = service.lock().create_characteristic(
            uuid128!("6e400003-b5a3-f393-e0a9-e50e24dcca9e"),
            NimbleProperties::READ
                | NimbleProperties::NOTIFY
                | NimbleProperties::READ_ENC
                | NimbleProperties::READ_AUTHEN,
        );
        let rx = service.lock().create_characteristic(
            uuid128!("6e400002-b5a3-f393-e0a9-e50e24dcca9e"),
            NimbleProperties::WRITE
                | NimbleProperties::WRITE_NO_RSP
                | NimbleProperties::WRITE_ENC
                | NimbleProperties::WRITE_AUTHEN,
        );
        let write_framer = framer;
        rx.lock().on_write(move |args| {
            if let Ok(mut framer) = write_framer.lock() {
                match framer.push(args.recv_data()) {
                    Ok(commands) => {
                        for command in commands {
                            let _ = sender.send(BleEvent::Command(command));
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(BleEvent::Command(Command::ProtocolError(error)));
                    }
                }
            }
        });

        ble_device.get_advertising().lock().set_data(
            BLEAdvertisementData::new()
                .name(device_name)
                .add_service_uuid(uuid128!("6e400001-b5a3-f393-e0a9-e50e24dcca9e")),
        )?;
        log::info!("BLE ready (advertising off until pairing mode)");

        let notify = Box::new(move |value: &[u8]| {
            let mut characteristic = tx.lock();
            characteristic.set_value(value).notify();
        });
        Ok(Self {
            receiver,
            notify,
            passkey,
            conn_handle,
        })
    }

    pub fn poll_event(&mut self) -> Option<BleEvent> {
        self.receiver.try_recv().ok()
    }

    /// Starts advertising with a fresh random passkey for the next pairing.
    pub fn start_advertising(&mut self) -> anyhow::Result<u32> {
        let passkey = new_passkey();
        self.passkey.store(passkey, Ordering::Relaxed);
        let advertising = BLEDevice::take().get_advertising();
        if !advertising.lock().is_advertising() {
            advertising.lock().start()?;
        }
        Ok(passkey)
    }

    pub fn stop_advertising(&mut self) {
        let advertising = BLEDevice::take().get_advertising();
        if advertising.lock().is_advertising() {
            if let Err(error) = advertising.lock().stop() {
                log::warn!("BLE advertising stop failed: {error:?}");
            }
        }
    }

    pub fn disconnect(&mut self) {
        let handle = self.conn_handle.load(Ordering::Relaxed);
        if handle != NO_CONNECTION {
            if let Err(error) = BLEDevice::take().get_server().disconnect(handle) {
                log::warn!("BLE disconnect failed: {error:?}");
            }
        }
    }

    /// Removes every bonded phone (used by `CLEAR`).
    pub fn delete_bonds(&mut self) -> anyhow::Result<()> {
        BLEDevice::take()
            .delete_all_bonds()
            .map_err(|error| anyhow::anyhow!("delete bonds: {error:?}"))
    }

    fn current_mtu(&self) -> u16 {
        let handle = self.conn_handle.load(Ordering::Relaxed);
        if handle == NO_CONNECTION {
            return 23;
        }
        match unsafe { sys::ble_att_mtu(handle) } {
            0 => 23,
            mtu => mtu,
        }
    }
}

impl BleTransport for NimbleTransport {
    type Error = anyhow::Error;

    fn send_line(&mut self, message: &str) -> Result<(), Self::Error> {
        for chunk in notification_chunks(message, self.current_mtu()) {
            (self.notify)(&chunk);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Ok(())
    }
}

fn new_passkey() -> u32 {
    (unsafe { sys::esp_random() }) % 1_000_000
}
