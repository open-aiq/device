use crate::aqi::{compute_us_aqi, AqiResult};

pub const SHT30_ADDRESSES: [u8; 2] = [0x44, 0x45];
pub const OLED_ADDRESSES: [u8; 2] = [0x3c, 0x3d];

pub trait I2cBus: Send + Sync {
    fn write(&self, address: u8, bytes: &[u8]) -> Result<(), SensorError>;
    fn read(&self, address: u8, bytes: &mut [u8]) -> Result<(), SensorError>;

    fn probe(&self, address: u8) -> bool {
        self.write(address, &[]).is_ok()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryError {
    Missing,
    Ambiguous,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscoveredI2c {
    pub sht30: u8,
    pub oled: u8,
}

pub fn classify_i2c_addresses(addresses: &[u8]) -> Result<DiscoveredI2c, I2cDiscoveryErrors> {
    fn one(addresses: &[u8], candidates: &[u8]) -> Result<u8, DiscoveryError> {
        let mut found = addresses.iter().copied().filter(|a| candidates.contains(a));
        let first = found.next().ok_or(DiscoveryError::Missing)?;
        if found.next().is_some() {
            Err(DiscoveryError::Ambiguous)
        } else {
            Ok(first)
        }
    }
    let sht30 = one(addresses, &SHT30_ADDRESSES);
    let oled = one(addresses, &OLED_ADDRESSES);
    match (sht30, oled) {
        (Ok(sht30), Ok(oled)) => Ok(DiscoveredI2c { sht30, oled }),
        (sht30, oled) => Err(I2cDiscoveryErrors {
            sht30: sht30.err(),
            oled: oled.err(),
        }),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct I2cDiscoveryErrors {
    pub sht30: Option<DiscoveryError>,
    pub oled: Option<DiscoveryError>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SensorError {
    Bus,
    Timeout,
    InvalidFrame,
    Checksum,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PmReading {
    pub pm1_0: u16,
    pub pm2_5: u16,
    pub pm10_0: u16,
    pub number_0_3: u16,
    pub number_0_5: u16,
    pub number_1_0: u16,
    pub number_2_5: u16,
    pub number_5_0: u16,
    pub number_10_0: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClimateReading {
    pub temperature_c: f32,
    pub humidity: f32,
    pub heat_index_c: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SensorReadings {
    pub pm: Option<PmReading>,
    pub climate: Option<ClimateReading>,
    pub aqi: Option<AqiResult>,
}

/// Length of one PMS5003 data frame, from header to checksum.
pub const PMS5003_FRAME_LEN: usize = 32;
const PMS5003_HEADER: [u8; 2] = [0x42, 0x4d];
/// Length of the short reply the sensor sends after a command: header,
/// length (4), the command and its data, and a checksum. The manual doesn't
/// describe it, but sensors send one after a mode change or a sleep.
const PMS5003_REPLY_LEN: usize = 8;

/// A command the PMS5003 accepts over its UART (`docs/datasheets/`,
/// Appendix II, page 15). The manual lists three: read in passive mode
/// (`0xe2`), change mode (`0xe1`) and sleep or wake (`0xe4`). There's no
/// reset command; only the RESET pin or a power cycle restarts the sensor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pms5003Command {
    /// Stops the fan and laser.
    Sleep,
    /// Starts the fan and laser again. Readings need 30 s to settle.
    Wake,
    /// Sends a frame on its own every 200 ms–2.3 s. The default at power-up.
    ActiveMode,
}

impl Pms5003Command {
    /// The seven bytes to send: `0x42 0x4d CMD DATAH DATAL`, then the sum of
    /// those five bytes, high byte first.
    pub fn bytes(self) -> [u8; 7] {
        let (command, data) = match self {
            Self::Sleep => (0xe4, 0x00),
            Self::Wake => (0xe4, 0x01),
            Self::ActiveMode => (0xe1, 0x01),
        };
        let body = [PMS5003_HEADER[0], PMS5003_HEADER[1], command, 0x00, data];
        let [high, low] = body
            .iter()
            .map(|byte| u16::from(*byte))
            .sum::<u16>()
            .to_be_bytes();
        [body[0], body[1], body[2], body[3], body[4], high, low]
    }
}

/// Collects bytes from the PMS5003's UART and cuts them into frames.
///
/// Bytes before a frame header are dropped, so it finds its way back after
/// noise or a lost byte.
#[derive(Default)]
pub struct Pms5003Framer {
    buffer: Vec<u8>,
}

impl Pms5003Framer {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// Returns the next whole frame, or `None` until one has arrived.
    /// Replies to commands are dropped, so they never count as readings.
    pub fn next_frame(&mut self) -> Option<Result<PmReading, SensorError>> {
        loop {
            match self
                .buffer
                .windows(PMS5003_HEADER.len())
                .position(|pair| pair == PMS5003_HEADER)
            {
                Some(start) => {
                    self.buffer.drain(..start);
                }
                None => {
                    // Keep a trailing first header byte; its partner may be next.
                    let keep = usize::from(self.buffer.last() == Some(&PMS5003_HEADER[0]));
                    let drop = self.buffer.len() - keep;
                    self.buffer.drain(..drop);
                }
            }
            if self.buffer.len() < 4 {
                return None;
            }
            let length = usize::from(u16::from_be_bytes([self.buffer[2], self.buffer[3]]));
            if length == PMS5003_REPLY_LEN - 4 {
                if self.buffer.len() < PMS5003_REPLY_LEN {
                    return None;
                }
                self.buffer.drain(..PMS5003_REPLY_LEN);
                continue;
            }
            if self.buffer.len() < PMS5003_FRAME_LEN {
                return None;
            }
            let frame: Vec<u8> = self.buffer.drain(..PMS5003_FRAME_LEN).collect();
            return Some(parse_pms5003_frame(&frame));
        }
    }
}

pub fn parse_pms5003_frame(frame: &[u8]) -> Result<PmReading, SensorError> {
    if frame.len() != PMS5003_FRAME_LEN
        || frame[0..2] != PMS5003_HEADER
        || u16::from_be_bytes([frame[2], frame[3]]) != 28
    {
        return Err(SensorError::InvalidFrame);
    }
    let expected = u16::from_be_bytes([frame[30], frame[31]]);
    let actual = frame[..30].iter().map(|byte| u16::from(*byte)).sum::<u16>();
    if actual != expected {
        return Err(SensorError::Checksum);
    }
    let word = |offset: usize| u16::from_be_bytes([frame[offset], frame[offset + 1]]);
    Ok(PmReading {
        pm1_0: word(10),
        pm2_5: word(12),
        pm10_0: word(14),
        number_0_3: word(16),
        number_0_5: word(18),
        number_1_0: word(20),
        number_2_5: word(22),
        number_5_0: word(24),
        number_10_0: word(26),
    })
}

pub fn sht30_crc(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0xff, |mut crc, byte| {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x31
            } else {
                crc << 1
            };
        }
        crc
    })
}

pub fn decode_sht30(bytes: &[u8; 6]) -> Result<ClimateReading, SensorError> {
    if sht30_crc(&bytes[0..2]) != bytes[2] || sht30_crc(&bytes[3..5]) != bytes[5] {
        return Err(SensorError::Checksum);
    }
    let raw_t = u16::from_be_bytes([bytes[0], bytes[1]]) as f32;
    let raw_h = u16::from_be_bytes([bytes[3], bytes[4]]) as f32;
    let temperature_c = -45.0 + 175.0 * raw_t / 65_535.0;
    let humidity = 100.0 * raw_h / 65_535.0;
    Ok(ClimateReading {
        temperature_c,
        humidity,
        heat_index_c: heat_index_c(temperature_c, humidity),
    })
}

// Adafruit/DHT-compatible NOAA heat-index algorithm.
pub fn heat_index_c(temperature_c: f32, humidity: f32) -> f32 {
    let t = temperature_c * 1.8 + 32.0;
    let mut hi = 0.5 * (t + 61.0 + (t - 68.0) * 1.2 + humidity * 0.094);
    if (hi + t) * 0.5 > 80.0 {
        hi = -42.379 + 2.049_015_3 * t + 10.143_332 * humidity
            - 0.224_755_4 * t * humidity
            - 0.006_837_83 * t * t
            - 0.054_817_17 * humidity * humidity
            + 0.001_228_74 * t * t * humidity
            + 0.000_852_82 * t * humidity * humidity
            - 0.000_001_99 * t * t * humidity * humidity;
        if humidity < 13.0 && (80.0..=112.0).contains(&t) {
            hi -= ((13.0 - humidity) * 0.25) * ((17.0 - (t - 95.0).abs()) * 0.058_823_53).sqrt();
        } else if humidity > 85.0 && (80.0..=87.0).contains(&t) {
            hi += ((humidity - 85.0) * 0.1) * ((87.0 - t) * 0.2);
        }
    }
    (hi - 32.0) / 1.8
}

pub struct Sht30<B> {
    bus: B,
    address: u8,
}

impl<B: I2cBus> Sht30<B> {
    pub fn new(bus: B, address: u8) -> Self {
        Self { bus, address }
    }

    pub fn read(&self) -> Result<ClimateReading, SensorError> {
        self.bus.write(self.address, &[0x24, 0x00])?;
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut bytes = [0; 6];
        self.bus.read(self.address, &mut bytes)?;
        decode_sht30(&bytes)
    }
}

pub fn readings(pm: Option<PmReading>, climate: Option<ClimateReading>) -> SensorReadings {
    let aqi = pm.as_ref().map(|pm| compute_us_aqi(pm.pm2_5, pm.pm10_0));
    SensorReadings { pm, climate, aqi }
}

#[cfg(target_os = "espidf")]
pub mod esp;
pub mod schedule;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_known_addresses() {
        assert_eq!(
            classify_i2c_addresses(&[0x20, 0x3d, 0x45]),
            Ok(DiscoveredI2c {
                sht30: 0x45,
                oled: 0x3d
            })
        );
        let missing = classify_i2c_addresses(&[]).unwrap_err();
        assert_eq!(missing.sht30, Some(DiscoveryError::Missing));
        let ambiguous = classify_i2c_addresses(&[0x44, 0x45, 0x3c]).unwrap_err();
        assert_eq!(ambiguous.sht30, Some(DiscoveryError::Ambiguous));
    }

    #[test]
    fn decodes_sht30_reference_sample() {
        let bytes = [
            0x66,
            0x66,
            sht30_crc(&[0x66, 0x66]),
            0x80,
            0x00,
            sht30_crc(&[0x80, 0x00]),
        ];
        let reading = decode_sht30(&bytes).unwrap();
        assert!((reading.temperature_c - 25.0).abs() < 0.01);
        assert!((reading.humidity - 50.0).abs() < 0.01);
    }

    /// A valid frame whose atmospheric PM2.5 is `pm2_5` and PM10 is 18.
    fn pms_frame(pm2_5: u16) -> [u8; 32] {
        let mut frame = [0u8; 32];
        frame[0] = 0x42;
        frame[1] = 0x4d;
        frame[2] = 0;
        frame[3] = 28;
        frame[12..14].copy_from_slice(&pm2_5.to_be_bytes());
        frame[14] = 0;
        frame[15] = 18;
        let checksum = frame[..30]
            .iter()
            .map(|v| u16::from(*v))
            .sum::<u16>()
            .to_be_bytes();
        frame[30..32].copy_from_slice(&checksum);
        frame
    }

    #[test]
    fn pms_checksum_is_enforced() {
        let mut frame = pms_frame(12);
        assert_eq!(parse_pms5003_frame(&frame).unwrap().pm2_5, 12);
        frame[5] = 1;
        assert_eq!(parse_pms5003_frame(&frame), Err(SensorError::Checksum));
    }

    #[test]
    fn framer_waits_for_a_whole_frame() {
        let frame = pms_frame(12);
        let mut framer = Pms5003Framer::default();
        framer.push(&frame[..20]);
        assert_eq!(framer.next_frame(), None);
        framer.push(&frame[20..]);
        assert_eq!(framer.next_frame().unwrap().unwrap().pm2_5, 12);
        assert_eq!(framer.next_frame(), None);
    }

    #[test]
    fn framer_skips_noise_before_a_header() {
        let mut framer = Pms5003Framer::default();
        framer.push(&[0x00, 0x4d, 0x42, 0x13, 0x42]);
        assert_eq!(framer.next_frame(), None);
        framer.push(&pms_frame(7)[1..]);
        // The trailing 0x42 above was the real header's first byte.
        assert_eq!(framer.next_frame().unwrap().unwrap().pm2_5, 7);
    }

    #[test]
    fn framer_returns_back_to_back_frames_in_order() {
        let mut framer = Pms5003Framer::default();
        framer.push(&pms_frame(1));
        framer.push(&pms_frame(2));
        assert_eq!(framer.next_frame().unwrap().unwrap().pm2_5, 1);
        assert_eq!(framer.next_frame().unwrap().unwrap().pm2_5, 2);
        assert_eq!(framer.next_frame(), None);
    }

    #[test]
    fn framer_reports_a_bad_frame_and_recovers() {
        let mut bad = pms_frame(5);
        bad[31] ^= 0xff;
        let mut framer = Pms5003Framer::default();
        framer.push(&bad);
        framer.push(&pms_frame(9));
        assert_eq!(framer.next_frame(), Some(Err(SensorError::Checksum)));
        assert_eq!(framer.next_frame().unwrap().unwrap().pm2_5, 9);
    }

    #[test]
    fn pms_commands_match_the_manual() {
        // `docs/datasheets/README.md` quotes these two from page 15.
        assert_eq!(
            Pms5003Command::Sleep.bytes(),
            [0x42, 0x4d, 0xe4, 0x00, 0x00, 0x01, 0x73]
        );
        assert_eq!(
            Pms5003Command::Wake.bytes(),
            [0x42, 0x4d, 0xe4, 0x00, 0x01, 0x01, 0x74]
        );
        assert_eq!(
            Pms5003Command::ActiveMode.bytes(),
            [0x42, 0x4d, 0xe1, 0x00, 0x01, 0x01, 0x71]
        );
    }

    #[test]
    fn framer_drops_command_replies() {
        // The reply to "sleep", as sent by the sensor.
        let reply = [0x42, 0x4d, 0x00, 0x04, 0xe4, 0x00, 0x01, 0x77];
        let mut framer = Pms5003Framer::default();
        framer.push(&reply[..5]);
        assert_eq!(framer.next_frame(), None);
        framer.push(&reply[5..]);
        framer.push(&pms_frame(4));
        assert_eq!(framer.next_frame().unwrap().unwrap().pm2_5, 4);
        assert_eq!(framer.next_frame(), None);
    }
}
