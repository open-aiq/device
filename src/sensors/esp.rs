use std::sync::{Arc, Mutex};

use esp_idf_svc::hal::{delay::BLOCK, i2c::I2cDriver, uart::UartDriver};

use crate::sensors::{parse_pms5003_frame, I2cBus, PmReading, SensorError};

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
    buffer: Vec<u8>,
}

impl Pms5003 {
    pub fn new(uart: UartDriver<'static>) -> Self {
        Self {
            uart,
            buffer: Vec::with_capacity(64),
        }
    }

    pub fn read(&mut self) -> Result<PmReading, SensorError> {
        let started = std::time::Instant::now();
        while started.elapsed() < std::time::Duration::from_secs(3) {
            let mut bytes = [0u8; 32];
            let count = self
                .uart
                .read(&mut bytes, 100)
                .map_err(|_| SensorError::Bus)?;
            self.buffer.extend_from_slice(&bytes[..count]);
            while self.buffer.len() >= 2 && self.buffer[..2] != [0x42, 0x4d] {
                self.buffer.remove(0);
            }
            if self.buffer.len() >= 32 {
                let frame: Vec<u8> = self.buffer.drain(..32).collect();
                return parse_pms5003_frame(&frame);
            }
        }
        Err(SensorError::Timeout)
    }
}
