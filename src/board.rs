//! Wiring and hardware initialization for the ESP32-S3 AirMonitor board.

use std::sync::{Arc, Mutex};

use esp_idf_svc::hal::{
    gpio::{self, PinDriver, Pins},
    i2c::{I2cConfig, I2cDriver, I2C0},
    uart::{config::Config as UartConfig, UartDriver, UART1},
    units::{Hertz, KiloHertz},
};

use crate::{
    buttons::esp::EspButtons,
    led::esp::{self as led, LedHandle},
    sensors::esp::{Pms5003, SharedI2c},
};

pub struct Board {
    pub shared_i2c: SharedI2c,
    pub pms5003: Pms5003,
    pub buttons: EspButtons,
    pub led: LedHandle,
}

impl Board {
    pub fn new(pins: Pins, i2c0: I2C0<'static>, uart1: UART1<'static>) -> anyhow::Result<Self> {
        // WS2812 status LED: D1 on the SMT board, and the onboard LED on
        // DevKitC-1 v1.0. (DevKitC-1 v1.1 moved it to GPIO 38.)
        let led = led::start(pins.gpio48);

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

        // PMS5003 SET: driven high to run, low to sleep. RESET (GPIO 16) is left alone.
        let pms_set = PinDriver::output(pins.gpio8)?;
        let pms5003 = Pms5003::new(uart, pms_set)
            .map_err(|error| anyhow::anyhow!("PMS5003 SET pin: {error:?}"))?;

        Ok(Self {
            shared_i2c,
            pms5003,
            // Right, left. The pair button on GPIO 7 isn't read yet.
            buttons: EspButtons::new(pins.gpio15, pins.gpio6)?,
            led,
        })
    }
}
