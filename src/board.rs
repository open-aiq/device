//! Wiring and hardware initialization for the ESP32-S3 AirMonitor board.

use std::sync::{Arc, Mutex};

use esp_idf_svc::hal::{
    gpio::{self, Pins},
    i2c::{I2cConfig, I2cDriver, I2C0},
    uart::{config::Config as UartConfig, UartDriver, UART1},
    units::{Hertz, KiloHertz},
};

use crate::{
    buttons::esp::EspButtons,
    sensors::esp::{Pms5003, SharedI2c},
};

pub struct Board {
    pub shared_i2c: SharedI2c,
    pub pms5003: Pms5003,
    pub buttons: EspButtons,
}

impl Board {
    pub fn new(pins: Pins, i2c0: I2C0<'static>, uart1: UART1<'static>) -> anyhow::Result<Self> {
        let i2c_config = I2cConfig::new().baudrate(KiloHertz(400).into());
        // Proto2 rev B pin map; it lets every trace fit on one copper layer.
        let i2c = I2cDriver::new(i2c0, pins.gpio5, pins.gpio4, &i2c_config)?;
        let shared_i2c = SharedI2c(Arc::new(Mutex::new(i2c)));

        let uart_config = UartConfig::new().baudrate(Hertz(9_600));
        let uart = UartDriver::new(
            uart1,
            pins.gpio18, // ESP TX -> PMS5003 RX
            pins.gpio17, // ESP RX <- PMS5003 TX
            Option::<gpio::Gpio0>::None,
            Option::<gpio::Gpio1>::None,
            &uart_config,
        )?;

        Ok(Self {
            shared_i2c,
            pms5003: Pms5003::new(uart),
            // Right, left. The pair button on GPIO 7 isn't read yet.
            buttons: EspButtons::new(pins.gpio15, pins.gpio6)?,
        })
    }
}
