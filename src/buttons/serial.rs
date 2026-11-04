//! Serial-console stand-in for the physical buttons (cargo feature `serial-buttons`).
//!
//! Type `BTN:LEFT`, `BTN:RIGHT`, `BTN:PAIR` or `BTN:HELP` followed by Enter in
//! `make monitor`. Both the UART0 console and the native USB-Serial-JTAG port are
//! read, so it works whichever USB connector the board is plugged in through.

use std::sync::mpsc;

use esp_idf_svc::sys;

use crate::buttons::{parse_serial_input, ButtonEvent, ButtonSource, SerialInput, SERIAL_HELP};

const LINE_LIMIT: usize = 64;

pub struct SerialButtons {
    receiver: mpsc::Receiver<ButtonEvent>,
}

impl SerialButtons {
    pub fn start() -> anyhow::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        install_drivers();
        std::thread::Builder::new()
            .name("serial-buttons".into())
            .stack_size(3 * 1024)
            .spawn(move || reader_loop(sender))?;
        log::info!("{SERIAL_HELP}");
        Ok(Self { receiver })
    }
}

impl ButtonSource for SerialButtons {
    fn poll(&mut self, _now_ms: u64) -> Option<ButtonEvent> {
        self.receiver.try_recv().ok()
    }
}

fn install_drivers() {
    unsafe {
        // UART0 is the primary console; installing the driver keeps logging working
        // through VFS while letting us read bytes without blocking the main loop.
        if !sys::uart_is_driver_installed(0) {
            let rc = sys::uart_driver_install(0, 256, 0, 0, core::ptr::null_mut(), 0);
            if rc == sys::ESP_OK {
                sys::uart_vfs_dev_use_driver(0);
            } else {
                log::warn!("serial-buttons: UART0 driver install failed ({rc})");
            }
        }
        if !sys::usb_serial_jtag_is_driver_installed() {
            let mut config = sys::usb_serial_jtag_driver_config_t {
                tx_buffer_size: 256,
                rx_buffer_size: 256,
            };
            let rc = sys::usb_serial_jtag_driver_install(&mut config);
            if rc != sys::ESP_OK {
                log::warn!("serial-buttons: USB-Serial-JTAG driver install failed ({rc})");
            }
        }
    }
}

fn reader_loop(sender: mpsc::Sender<ButtonEvent>) {
    let mut uart_line = Vec::with_capacity(LINE_LIMIT);
    let mut usb_line = Vec::with_capacity(LINE_LIMIT);
    let mut buffer = [0u8; 32];
    loop {
        let uart_read = unsafe {
            if sys::uart_is_driver_installed(0) {
                sys::uart_read_bytes(0, buffer.as_mut_ptr().cast(), buffer.len() as u32, 2)
            } else {
                0
            }
        };
        if uart_read > 0 {
            feed(&buffer[..uart_read as usize], &mut uart_line, &sender);
        }
        let usb_read = unsafe {
            if sys::usb_serial_jtag_is_driver_installed() {
                sys::usb_serial_jtag_read_bytes(buffer.as_mut_ptr().cast(), buffer.len() as u32, 2)
            } else {
                0
            }
        };
        if usb_read > 0 {
            feed(&buffer[..usb_read as usize], &mut usb_line, &sender);
        }
        if uart_read <= 0 && usb_read <= 0 {
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
    }
}

fn feed(bytes: &[u8], line: &mut Vec<u8>, sender: &mpsc::Sender<ButtonEvent>) {
    for &byte in bytes {
        match byte {
            b'\n' | b'\r' => {
                if !line.is_empty() {
                    handle_line(&String::from_utf8_lossy(line), sender);
                    line.clear();
                }
            }
            _ if line.len() < LINE_LIMIT => line.push(byte),
            _ => line.clear(),
        }
    }
}

fn handle_line(line: &str, sender: &mpsc::Sender<ButtonEvent>) {
    match parse_serial_input(line) {
        Some(SerialInput::Button(event)) => {
            log::info!("serial-buttons: {event:?}");
            let _ = sender.send(event);
        }
        Some(SerialInput::Help) => log::info!("{SERIAL_HELP}"),
        None => log::info!("serial-buttons: ignored '{}' ({SERIAL_HELP})", line.trim()),
    }
}
