use std::sync::{Arc, Mutex};

use esp_idf_svc::{
    hal::{
        delay::{BLOCK, NON_BLOCK},
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
    framer: Pms5003Framer,
}

impl Pms5003 {
    pub fn new(uart: UartDriver<'static>) -> Self {
        Self {
            uart,
            framer: Pms5003Framer::default(),
        }
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
