use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use esp_idf_svc::{
    hal::{
        delay::{BLOCK, NON_BLOCK},
        gpio::{Output, PinDriver},
        i2c::I2cDriver,
        uart::UartDriver,
    },
    sys::ESP_ERR_TIMEOUT,
};

use crate::sensors::{I2cBus, PmReading, Pms5003Framer, SensorError, PMS5003_FRAME_LEN};

#[derive(Clone)]
pub struct SharedI2c(pub Arc<Mutex<I2cDriver<'static>>>);

impl I2cBus for SharedI2c {
    fn write(&self, address: u8, bytes: &[u8]) -> Result<(), SensorError> {
        self.0
            .lock()
            .map_err(|_| SensorError::Bus)?
            .write(address, bytes, BLOCK)
            .map_err(|_| SensorError::Bus)
    }
    fn read(&self, address: u8, bytes: &mut [u8]) -> Result<(), SensorError> {
        self.0
            .lock()
            .map_err(|_| SensorError::Bus)?
            .read(address, bytes, BLOCK)
            .map_err(|_| SensorError::Bus)
    }
}

pub struct Pms5003 {
    uart: UartDriver<'static>,
    /// SET pin: high (or floating) runs the fan and laser, low puts them to
    /// sleep (`docs/datasheets/`, page 4).
    set: PinDriver<'static, Output>,
    /// RESET pin: held high; low resets the sensor (the pin table in
    /// `docs/datasheets/`). Boards that leave it unwired are unaffected.
    reset: PinDriver<'static, Output>,
    framer: Pms5003Framer,
}

/// How long RESET is held low. The manual gives no minimum, so this is a
/// generous pulse for a 3.3 V TTL input.
const RESET_PULSE: Duration = Duration::from_millis(100);

impl Pms5003 {
    /// Takes the sensor's UART, SET and RESET pins and leaves the sensor
    /// running.
    pub fn new(
        uart: UartDriver<'static>,
        set: PinDriver<'static, Output>,
        reset: PinDriver<'static, Output>,
    ) -> Result<Self, SensorError> {
        let mut sensor = Self {
            uart,
            set,
            reset,
            framer: Pms5003Framer::default(),
        };
        sensor.reset.set_high().map_err(|_| SensorError::Bus)?;
        sensor.set.set_high().map_err(|_| SensorError::Bus)?;
        Ok(sensor)
    }

    /// Pulses RESET low to restart a sensor that has stopped sending frames.
    /// Blocks for [`RESET_PULSE`]. SET is untouched, so a sleeping sensor
    /// stays asleep.
    pub fn reset(&mut self) -> Result<(), SensorError> {
        self.reset.set_low().map_err(|_| SensorError::Bus)?;
        std::thread::sleep(RESET_PULSE);
        self.reset.set_high().map_err(|_| SensorError::Bus)?;
        self.framer = Pms5003Framer::default();
        Ok(())
    }

    /// Stops the fan and laser.
    pub fn sleep(&mut self) -> Result<(), SensorError> {
        self.set.set_low().map_err(|_| SensorError::Bus)
    }

    /// Starts the fan and laser and forgets any bytes received before, so a
    /// frame from before the sleep is never averaged. Readings need 30 s to
    /// settle after this (`docs/datasheets/`, page 10).
    pub fn wake(&mut self) -> Result<(), SensorError> {
        self.set.set_high().map_err(|_| SensorError::Bus)?;
        self.uart.clear_rx().map_err(|_| SensorError::Bus)?;
        self.framer = Pms5003Framer::default();
        Ok(())
    }

    /// Returns the next frame if a whole one has arrived, without waiting.
    ///
    /// In active mode the sensor sends a frame every 2.3 s, or every
    /// 200–800 ms while the air is changing fast (`docs/datasheets/`, page 4).
    pub fn poll(&mut self) -> Result<Option<PmReading>, SensorError> {
        let mut bytes = [0u8; 2 * PMS5003_FRAME_LEN];
        match self.uart.read(&mut bytes, NON_BLOCK) {
            Ok(count) => self.framer.push(&bytes[..count]),
            // A non-blocking read reports "no bytes waiting" as a timeout.
            Err(error) if error.code() == ESP_ERR_TIMEOUT => {}
            Err(_) => return Err(SensorError::Bus),
        }
        self.framer.next_frame().transpose()
    }
}
