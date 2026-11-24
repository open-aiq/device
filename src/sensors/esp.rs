use std::sync::{Arc, Mutex};

use esp_idf_svc::{
    hal::{
        delay::{BLOCK, NON_BLOCK},
        i2c::I2cDriver,
        uart::UartDriver,
    },
    sys::ESP_ERR_TIMEOUT,
};

use crate::sensors::{
    I2cBus, PmReading, Pms5003Command, Pms5003Framer, SensorError, PMS5003_FRAME_LEN,
};

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

/// The PMS5003 on a 4-wire cable (`TX RX VCC GND`). With no SET or RESET
/// wire, it sleeps and wakes by UART command instead.
pub struct Pms5003 {
    uart: UartDriver<'static>,
    framer: Pms5003Framer,
}

impl Pms5003 {
    /// Takes the sensor's UART and makes sure it's awake and streaming. The
    /// sensor keeps its power when only the ESP32 restarts (after an update,
    /// say), so it may still be asleep from before.
    pub fn new(uart: UartDriver<'static>) -> Result<Self, SensorError> {
        let mut sensor = Self {
            uart,
            framer: Pms5003Framer::default(),
        };
        sensor.wake()?;
        Ok(sensor)
    }

    /// Stops the fan and laser.
    pub fn sleep(&mut self) -> Result<(), SensorError> {
        self.send(Pms5003Command::Sleep)
    }

    /// Starts the fan and laser, asks for active mode, and forgets any bytes
    /// received before, so a frame from before the sleep is never averaged.
    /// Asking for active mode every time also brings back a sensor that
    /// somehow ended up in passive mode and went quiet. Readings need 30 s to
    /// settle after this (`docs/datasheets/`, page 10).
    pub fn wake(&mut self) -> Result<(), SensorError> {
        self.uart.clear_rx().map_err(|_| SensorError::Bus)?;
        self.framer = Pms5003Framer::default();
        self.send(Pms5003Command::Wake)?;
        self.send(Pms5003Command::ActiveMode)
    }

    fn send(&mut self, command: Pms5003Command) -> Result<(), SensorError> {
        let bytes = command.bytes();
        let written = self.uart.write(&bytes).map_err(|_| SensorError::Bus)?;
        if written != bytes.len() {
            return Err(SensorError::Bus);
        }
        self.uart.wait_tx_done(BLOCK).map_err(|_| SensorError::Bus)
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
