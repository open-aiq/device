use std::sync::{mpsc, Arc, Mutex};

use esp32_nimble::{uuid128, BLEAdvertisementData, BLEDevice, NimbleProperties};

use crate::ble::{notification_chunks, BleTransport, Command, CommandFramer};

type NotifyFn = dyn Fn(&[u8]);

pub struct NimbleTransport {
    receiver: mpsc::Receiver<Command>,
    notify: Box<NotifyFn>,
}

impl NimbleTransport {
    pub fn start(device_name: &str) -> anyhow::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let framer = Arc::new(Mutex::new(CommandFramer::default()));
        let ble_device = BLEDevice::take();
        let server = ble_device.get_server();

        let disconnect_framer = framer.clone();
        server.on_disconnect(move |_description, reason| {
            log::info!("BLE disconnected: {reason:?}");
            if let Ok(mut framer) = disconnect_framer.lock() {
                framer.clear();
            }
        });
        server.on_connect(|_, description| log::info!("BLE connected: {description:?}"));

        let service = server.create_service(uuid128!("6e400001-b5a3-f393-e0a9-e50e24dcca9e"));
        let tx = service.lock().create_characteristic(
            uuid128!("6e400003-b5a3-f393-e0a9-e50e24dcca9e"),
            NimbleProperties::READ | NimbleProperties::NOTIFY,
        );
        let rx = service.lock().create_characteristic(
            uuid128!("6e400002-b5a3-f393-e0a9-e50e24dcca9e"),
            NimbleProperties::WRITE | NimbleProperties::WRITE_NO_RSP,
        );
        let write_framer = framer;
        rx.lock().on_write(move |args| {
            if let Ok(mut framer) = write_framer.lock() {
                match framer.push(args.recv_data()) {
                    Ok(commands) => {
                        for command in commands {
                            let _ = sender.send(command);
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Command::ProtocolError(error));
                    }
                }
            }
        });

        ble_device.get_advertising().lock().set_data(
            BLEAdvertisementData::new()
                .name(device_name)
                .add_service_uuid(uuid128!("6e400001-b5a3-f393-e0a9-e50e24dcca9e")),
        )?;
        ble_device.get_advertising().lock().start()?;

        let notify = Box::new(move |value: &[u8]| {
            let mut characteristic = tx.lock();
            characteristic.set_value(value).notify();
        });
        Ok(Self { receiver, notify })
    }
}

impl BleTransport for NimbleTransport {
    type Error = anyhow::Error;

    fn poll_command(&mut self) -> Option<Command> {
        self.receiver.try_recv().ok()
    }

    fn send_line(&mut self, message: &str) -> Result<(), Self::Error> {
        for chunk in notification_chunks(message) {
            (self.notify)(&chunk);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Ok(())
    }
}
