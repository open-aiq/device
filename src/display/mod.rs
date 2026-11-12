use embedded_graphics::{
    mono_font::{
        ascii::{FONT_10X20, FONT_6X10},
        MonoTextStyle,
    },
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};

use crate::{
    aqi::{AqiCategory, BANDS},
    sensors::{I2cBus, SensorError, SensorReadings},
    telemetry::UploadResult,
    wifi::{signal_bars, WifiStatus},
};

pub const WIDTH: u32 = 128;
pub const HEIGHT: u32 = 64;
const BUFFER_LEN: usize = (WIDTH as usize * HEIGHT as usize) / 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OledController {
    Sh1106,
    Ssd1306,
}

#[cfg(oled_ssd1306)]
pub const SELECTED_CONTROLLER: OledController = OledController::Ssd1306;
#[cfg(not(oled_ssd1306))]
pub const SELECTED_CONTROLLER: OledController = OledController::Sh1106;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Screen {
    /// The AQI number, its band, what the air feels like and the WiFi state.
    Aqi,
    /// Every particle reading, with temperature and humidity.
    Readings,
    /// The network, its signal, the last upload and the firmware version.
    Network,
}

impl Screen {
    pub fn next(self) -> Self {
        match self {
            Self::Aqi => Self::Readings,
            Self::Readings => Self::Network,
            Self::Network => Self::Aqi,
        }
    }
    pub fn previous(self) -> Self {
        match self {
            Self::Aqi => Self::Network,
            Self::Readings => Self::Aqi,
            Self::Network => Self::Readings,
        }
    }
}

/// Full-screen states that temporarily replace the normal screens.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Overlay {
    /// Shown from power-on until the first real screen.
    Starting,
    Pairing {
        remaining_s: u64,
        passkey: Option<u32>,
        connected: bool,
    },
    Updating {
        percent: u8,
    },
}

pub struct ScreenModel<'a> {
    pub screen: Screen,
    pub readings: Option<&'a SensorReadings>,
    pub wifi: &'a WifiStatus,
    /// The saved network name; empty until WiFi is provisioned.
    pub ssid: &'a str,
    pub version: &'a str,
    pub last_upload: Option<(UploadResult, u64)>,
    pub now_ms: u64,
    pub overlay: Option<Overlay>,
    /// Whether WiFi and device credentials are provisioned; drives the setup hint.
    pub provisioned: bool,
}

/// Formats a passkey as `123 456` (always six digits).
pub fn format_passkey(passkey: u32) -> String {
    let digits = format!("{:06}", passkey % 1_000_000);
    format!("{} {}", &digits[..3], &digits[3..])
}

pub trait DisplayPort {
    type Error;
    fn show(&mut self, model: &ScreenModel<'_>) -> Result<(), Self::Error>;
}

pub struct OledDisplay<B> {
    bus: B,
    address: u8,
    controller: OledController,
    buffer: [u8; BUFFER_LEN],
}

impl<B: I2cBus> OledDisplay<B> {
    pub fn new(bus: B, address: u8, controller: OledController) -> Self {
        Self {
            bus,
            address,
            controller,
            buffer: [0; BUFFER_LEN],
        }
    }

    pub fn init(&self) -> Result<(), SensorError> {
        #[rustfmt::skip]
        let common = [
            0xae,       // display off while it's set up
            0xd5, 0x80, // clock divide and oscillator: the default
            0xa8, 0x3f, // multiplex ratio: 64 rows
            0xd3, 0x00, // no vertical offset
            0x40,       // start at row 0
            // Reverse columns and rows: the usual setting for these modules,
            // upright with the pin header along the top edge, as Proto2
            // holds it. If yours shows upside down, use 0xa0, 0xc0.
            0xa1, 0xc8,
            0xda, 0x12, // COM pin layout for a 128x64 panel
            0x81, 0x7f, // contrast: half
            0xd9, 0xf1, // pre-charge period
            0xdb, 0x40, // VCOMH deselect level
            0xa4,       // show the frame buffer, not all pixels on
            0xa6,       // normal, not inverted
        ];
        self.command(&common)?;
        match self.controller {
            // DC-DC converter on.
            OledController::Sh1106 => self.command(&[0xad, 0x8b])?,
            // Charge pump on, then horizontal addressing.
            OledController::Ssd1306 => self.command(&[0x8d, 0x14, 0x20, 0x00])?,
        }
        // Display on.
        self.command(&[0xaf])
    }

    fn command(&self, commands: &[u8]) -> Result<(), SensorError> {
        let mut packet = Vec::with_capacity(commands.len() + 1);
        packet.push(0x00);
        packet.extend_from_slice(commands);
        self.bus.write(self.address, &packet)
    }

    pub fn flush(&self) -> Result<(), SensorError> {
        const PAGE_LEN: usize = WIDTH as usize;
        for page in 0..(HEIGHT / 8) as u8 {
            match self.controller {
                // Page, then column 2: the SH1106's RAM is 132 wide and the
                // panel sits in the middle of it.
                OledController::Sh1106 => self.command(&[0xb0 + page, 0x02, 0x10])?,
                // Column range, then this one page.
                OledController::Ssd1306 => {
                    self.command(&[0x21, 0, WIDTH as u8 - 1, 0x22, page, page])?
                }
            }
            let row = &self.buffer[usize::from(page) * PAGE_LEN..usize::from(page + 1) * PAGE_LEN];
            for chunk in row.chunks(16) {
                let mut packet = [0u8; 17];
                packet[0] = 0x40;
                packet[1..=chunk.len()].copy_from_slice(chunk);
                self.bus.write(self.address, &packet[..=chunk.len()])?;
            }
        }
        Ok(())
    }
}

impl<B> OriginDimensions for OledDisplay<B> {
    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }
}

impl<B> DrawTarget for OledDisplay<B> {
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            if point.x < 0 || point.y < 0 || point.x >= WIDTH as i32 || point.y >= HEIGHT as i32 {
                continue;
            }
            let x = point.x as usize;
            let y = point.y as usize;
            let index = x + (y / 8) * WIDTH as usize;
            let mask = 1u8 << (y & 7);
            match color {
                BinaryColor::On => self.buffer[index] |= mask,
                BinaryColor::Off => self.buffer[index] &= !mask,
            }
        }
        Ok(())
    }
}

impl<B: I2cBus> DisplayPort for OledDisplay<B> {
    type Error = SensorError;
    fn show(&mut self, model: &ScreenModel<'_>) -> Result<(), Self::Error> {
        render(self, model);
        self.flush()
    }
}

const SMALL: MonoTextStyle<'static, BinaryColor> = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
const LARGE: MonoTextStyle<'static, BinaryColor> = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);

/// Width of one small character, in pixels.
const CHAR_W: i32 = 6;
const RIGHT: i32 = WIDTH as i32 - 1;
const MIDDLE: i32 = WIDTH as i32 / 2;
/// Left edge of body text on the list screens.
const MARGIN: i32 = 2;

/// The setup hint: 20 characters, the widest line the list screens take.
const PAIR_HINT: &str = "Hold both btns: pair";

pub fn render<D>(target: &mut D, model: &ScreenModel<'_>)
where
    D: DrawTarget<Color = BinaryColor> + OriginDimensions,
{
    let _ = target.clear(BinaryColor::Off);
    match model.overlay {
        Some(Overlay::Starting) => message(target, "Open AIQ", "starting..."),
        Some(Overlay::Pairing {
            remaining_s,
            passkey,
            connected,
        }) => {
            text(target, "PAIRING", MIDDLE, 4, SMALL, Alignment::Center);
            match passkey {
                Some(passkey) => {
                    let code = format_passkey(passkey);
                    text(target, &code, MIDDLE, 20, LARGE, Alignment::Center);
                    let hint = "Enter code on phone";
                    text(target, hint, MIDDLE, 46, SMALL, Alignment::Center);
                }
                None => {
                    let state = if connected {
                        "Phone connected"
                    } else {
                        "Open the app"
                    };
                    text(target, state, MIDDLE, 24, SMALL, Alignment::Center);
                    let left = format!("{}:{:02} left", remaining_s / 60, remaining_s % 60);
                    text(target, &left, MIDDLE, 44, SMALL, Alignment::Center);
                }
            }
        }
        Some(Overlay::Updating { percent }) => {
            message(target, &format!("{percent}%"), "Updating, keep power")
        }
        None => match model.screen {
            Screen::Aqi => aqi_screen(target, model),
            Screen::Readings => readings_screen(target, model),
            Screen::Network => network_screen(target, model),
        },
    }
}

/// The big number, with the band underneath as a name and a six-step gauge.
fn aqi_screen<D>(target: &mut D, model: &ScreenModel<'_>)
where
    D: DrawTarget<Color = BinaryColor> + OriginDimensions,
{
    /// The number is centred in the left of the screen, drawn at twice the size.
    const NUMBER_MIDDLE: i32 = 36;
    const NUMBER_TOP: i32 = 4;
    const FEELS_LEFT: i32 = 80;

    let aqi = model.readings.and_then(|value| value.aqi);
    let climate = model.readings.and_then(|value| value.climate.as_ref());

    text(target, "AQI", 0, 0, SMALL, Alignment::Left);
    wifi_icon(target, RIGHT - 12, 0, model.wifi.connected);

    let number = aqi.map_or("--".to_owned(), |value| value.aqi.to_string());
    text(
        &mut Scaled { target, factor: 2 },
        &number,
        NUMBER_MIDDLE / 2,
        NUMBER_TOP / 2,
        LARGE,
        Alignment::Center,
    );

    text(target, "Feels", FEELS_LEFT, 16, SMALL, Alignment::Left);
    let feels = climate.map_or("--".to_owned(), |value| {
        format!("{:.1} C", value.heat_index_c)
    });
    text(target, &feels, FEELS_LEFT, 28, SMALL, Alignment::Left);

    let band = match aqi {
        Some(value) => value.category.label(),
        None if !model.provisioned => PAIR_HINT,
        None => "warming up...",
    };
    text(target, band, MIDDLE, 43, SMALL, Alignment::Center);
    band_gauge(target, 56, aqi.map(|value| value.category));
}

/// One box per AQI band, from Good on the left; the current band is filled.
fn band_gauge<D>(target: &mut D, top: i32, current: Option<AqiCategory>)
where
    D: DrawTarget<Color = BinaryColor>,
{
    const STEP: i32 = 21;
    const BOX: Size = Size::new(20, 6);
    let left = (WIDTH as i32 - (STEP * BANDS.len() as i32 - 1)) / 2;
    for index in 0..BANDS.len() {
        let style = if current.is_some_and(|category| category.index() == index) {
            PrimitiveStyle::with_fill(BinaryColor::On)
        } else {
            PrimitiveStyle::with_stroke(BinaryColor::On, 1)
        };
        let _ = Rectangle::new(Point::new(left + STEP * index as i32, top), BOX)
            .into_styled(style)
            .draw(target);
    }
}

/// Every particle size the PMS5003 reports, then temperature and humidity.
fn readings_screen<D>(target: &mut D, model: &ScreenModel<'_>)
where
    D: DrawTarget<Color = BinaryColor> + OriginDimensions,
{
    const VALUE_RIGHT: i32 = 80;
    const UNIT_LEFT: i32 = 92;

    let pm = model.readings.and_then(|value| value.pm.as_ref());
    let climate = model.readings.and_then(|value| value.climate.as_ref());
    let rows = [
        ("PM1.0", pm.map(|value| value.pm1_0)),
        ("PM2.5", pm.map(|value| value.pm2_5)),
        ("PM10", pm.map(|value| value.pm10_0)),
    ];
    for (row, (name, value)) in rows.into_iter().enumerate() {
        let top = 1 + 13 * row as i32;
        let value = value.map_or("--".to_owned(), |value| value.to_string());
        text(target, name, MARGIN, top, SMALL, Alignment::Left);
        text(target, &value, VALUE_RIGHT, top, SMALL, Alignment::Right);
        text(target, "ug/m3", UNIT_LEFT, top, SMALL, Alignment::Left);
    }

    let _ = Line::new(Point::new(0, 43), Point::new(RIGHT, 43))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(target);
    let (temperature, humidity) = match climate {
        Some(value) => (
            format!("Temp {:.1}C", value.temperature_c),
            format!("Hum {:.0}%", value.humidity),
        ),
        None => ("Temp --".to_owned(), "Hum --".to_owned()),
    };
    text(target, &temperature, MARGIN, 50, SMALL, Alignment::Left);
    text(target, &humidity, RIGHT - 1, 50, SMALL, Alignment::Right);
}

/// How the monitor reaches the internet, and whether readings are getting out.
fn network_screen<D>(target: &mut D, model: &ScreenModel<'_>)
where
    D: DrawTarget<Color = BinaryColor> + OriginDimensions,
{
    /// Signal bars sit at the top right; the name gets the rest of the line.
    const BARS_LEFT: i32 = RIGHT - 15;
    const NAME_CHARS: usize = ((BARS_LEFT - MARGIN - 3) / CHAR_W) as usize;

    let name = if model.ssid.is_empty() {
        "No WiFi yet".to_owned()
    } else {
        fit(model.ssid, NAME_CHARS)
    };
    text(target, &name, MARGIN, 1, SMALL, Alignment::Left);
    let bars = match model.wifi.rssi {
        Some(rssi) if model.wifi.connected => signal_bars(rssi),
        _ => 0,
    };
    signal_icon(target, BARS_LEFT, 0, bars);

    let address = if !model.provisioned {
        PAIR_HINT.to_owned()
    } else if model.wifi.connected {
        format!("IP {}", model.wifi.ip.as_deref().unwrap_or("--"))
    } else {
        "Offline".to_owned()
    };
    text(target, &address, MARGIN, 15, SMALL, Alignment::Left);
    // Nothing can be sent before setup, so a failed upload isn't news then.
    let upload = if model.provisioned {
        fit(&upload_line(model.last_upload, model.now_ms), 20)
    } else {
        "Not set up yet".to_owned()
    };
    text(target, &upload, MARGIN, 29, SMALL, Alignment::Left);

    let _ = Line::new(Point::new(0, 43), Point::new(RIGHT, 43))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(target);
    let version = fit(&format!("Firmware v{}", model.version), 20);
    text(target, &version, MARGIN, 50, SMALL, Alignment::Left);
}

/// The last upload in plain words, at most 20 characters.
pub fn upload_line(last_upload: Option<(UploadResult, u64)>, now_ms: u64) -> String {
    let Some((result, at_ms)) = last_upload else {
        return "No upload yet".to_owned();
    };
    if !result.ok {
        return match result.http_code {
            // WiFi was down, so nothing was sent.
            0 => "Send failed: offline".to_owned(),
            // The request never got an answer.
            code if code < 0 => "Send failed: network".to_owned(),
            code => format!("Send failed ({code})"),
        };
    }
    match now_ms.saturating_sub(at_ms) / 60_000 {
        0 => "Sent just now".to_owned(),
        minutes @ 1..=59 => format!("Sent {minutes}m ago"),
        minutes => format!("Sent {}h ago", (minutes / 60).min(999)),
    }
}

/// Cuts `text` to `max` characters, ending in `..` when something was dropped.
/// Anything outside printable ASCII becomes `?`, because the font has no
/// glyph for it.
pub fn fit(text: &str, max: usize) -> String {
    let printable = |c: char| if (' '..='~').contains(&c) { c } else { '?' };
    if text.chars().count() <= max {
        return text.chars().map(printable).collect();
    }
    let mut cut: String = text
        .chars()
        .take(max.saturating_sub(2))
        .map(printable)
        .collect();
    cut.push_str("..");
    cut
}

/// A 13x10 WiFi fan. Offline, a slash runs through it.
fn wifi_icon<D>(target: &mut D, left: i32, top: i32, online: bool)
where
    D: DrawTarget<Color = BinaryColor>,
{
    const ONLINE: [&str; 10] = [
        "...#######...",
        ".##.......##.",
        "#...........#",
        "....#####....",
        "..##.....##..",
        ".............",
        "......#......",
        ".....###.....",
        ".....###.....",
        "......#......",
    ];
    const OFFLINE: [&str; 10] = [
        "...#######.#.",
        ".##.......#..",
        "#........#..#",
        "....###.#....",
        "..##...#.##..",
        "......#......",
        ".....#.......",
        "....#.##.....",
        "...#.###.....",
        "..#...#......",
    ];
    bitmap(target, left, top, if online { &ONLINE } else { &OFFLINE });
}

/// Four rising bars, 15x10. Bars past `bars` shrink to a dot on the baseline.
fn signal_icon<D>(target: &mut D, left: i32, top: i32, bars: u8)
where
    D: DrawTarget<Color = BinaryColor>,
{
    for index in 0..4 {
        let height = if index < bars as i32 {
            3 + 2 * index
        } else {
            1
        };
        let _ = Rectangle::new(
            Point::new(left + 4 * index, top + 10 - height),
            Size::new(3, height as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(target);
    }
}

/// Draws rows of `#` (on) and `.` (untouched).
fn bitmap<D>(target: &mut D, left: i32, top: i32, rows: &[&str])
where
    D: DrawTarget<Color = BinaryColor>,
{
    let pixels = rows.iter().enumerate().flat_map(|(y, row)| {
        row.bytes()
            .enumerate()
            .filter(|(_, cell)| *cell == b'#')
            .map(move |(x, _)| Pixel(Point::new(left + x as i32, top + y as i32), BinaryColor::On))
    });
    let _ = target.draw_iter(pixels);
}

/// One line of text with its top at `top`. `x` is the left edge, the middle
/// or the right edge, depending on `alignment`.
fn text<D>(
    target: &mut D,
    content: &str,
    x: i32,
    top: i32,
    font: MonoTextStyle<'_, BinaryColor>,
    alignment: Alignment,
) where
    D: DrawTarget<Color = BinaryColor>,
{
    let style = TextStyleBuilder::new()
        .alignment(alignment)
        .baseline(Baseline::Top)
        .build();
    let _ = Text::with_text_style(content, Point::new(x, top), font, style).draw(target);
}

/// A large line over a small one, centred: the full-screen notices.
fn message<D>(target: &mut D, first: &str, second: &str)
where
    D: DrawTarget<Color = BinaryColor>,
{
    text(target, first, MIDDLE, 12, LARGE, Alignment::Center);
    text(target, second, MIDDLE, 39, SMALL, Alignment::Center);
}

/// Draws onto `target` with every pixel enlarged to a `factor`-sized square,
/// so the built-in fonts can be shown bigger. Coordinates are in enlarged
/// pixels.
struct Scaled<'a, D> {
    target: &'a mut D,
    factor: u32,
}

impl<D: OriginDimensions> OriginDimensions for Scaled<'_, D> {
    fn size(&self) -> Size {
        self.target.size() / self.factor
    }
}

impl<D> DrawTarget for Scaled<'_, D>
where
    D: DrawTarget + OriginDimensions,
{
    type Color = D::Color;
    type Error = D::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let factor = self.factor as i32;
        for Pixel(point, color) in pixels {
            self.target.fill_solid(
                &Rectangle::new(point * factor, Size::new_equal(self.factor)),
                color,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        aqi::compute_us_aqi,
        sensors::{ClimateReading, PmReading},
    };

    /// A screen-sized canvas that fails the test when anything is drawn off
    /// the screen or on top of something else, which is how clipped and
    /// overlapping text shows up.
    struct Frame {
        lit: [[bool; WIDTH as usize]; HEIGHT as usize],
    }

    impl Frame {
        fn of(model: &ScreenModel<'_>) -> Self {
            let mut frame = Self {
                lit: [[false; WIDTH as usize]; HEIGHT as usize],
            };
            render(&mut frame, model);
            frame
        }

        fn lit_pixels(&self) -> usize {
            self.lit.iter().flatten().filter(|lit| **lit).count()
        }

        fn art(&self) -> String {
            self.lit
                .iter()
                .map(|row| {
                    let line: String = row.iter().map(|lit| if *lit { '#' } else { ' ' }).collect();
                    format!("|{line}|\n")
                })
                .collect()
        }
    }

    impl OriginDimensions for Frame {
        fn size(&self) -> Size {
            Size::new(WIDTH, HEIGHT)
        }
    }

    impl DrawTarget for Frame {
        type Color = BinaryColor;
        type Error = core::convert::Infallible;

        fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
        where
            I: IntoIterator<Item = Pixel<Self::Color>>,
        {
            for Pixel(point, color) in pixels {
                let on_screen =
                    (0..WIDTH as i32).contains(&point.x) && (0..HEIGHT as i32).contains(&point.y);
                assert!(on_screen, "drew off the screen at {point:?}");
                let cell = &mut self.lit[point.y as usize][point.x as usize];
                if color.is_on() {
                    assert!(!*cell, "drew twice at {point:?}");
                }
                *cell = color.is_on();
            }
            Ok(())
        }
    }

    fn readings(pm2_5: u16, pm10: u16, temperature_c: f32) -> SensorReadings {
        SensorReadings {
            pm: Some(PmReading {
                pm1_0: pm2_5 / 2,
                pm2_5,
                pm10_0: pm10,
                number_0_3: 0,
                number_0_5: 0,
                number_1_0: 0,
                number_2_5: 0,
                number_5_0: 0,
                number_10_0: 0,
            }),
            climate: Some(ClimateReading {
                temperature_c,
                humidity: 100.0,
                heat_index_c: temperature_c,
            }),
            aqi: Some(compute_us_aqi(pm2_5, pm10)),
        }
    }

    fn online() -> WifiStatus {
        WifiStatus {
            connected: true,
            ip: Some("255.255.255.255".to_owned()),
            rssi: Some(-60),
        }
    }

    fn model<'a>(
        screen: Screen,
        readings: Option<&'a SensorReadings>,
        wifi: &'a WifiStatus,
    ) -> ScreenModel<'a> {
        ScreenModel {
            screen,
            readings,
            wifi,
            ssid: "A network name that is 32 chars.",
            version: "0.4.0",
            last_upload: Some((
                UploadResult {
                    ok: true,
                    http_code: 201,
                },
                0,
            )),
            now_ms: 125_000,
            overlay: None,
            provisioned: true,
        }
    }

    const SCREENS: [Screen; 3] = [Screen::Aqi, Screen::Readings, Screen::Network];

    #[test]
    fn formats_passkey_with_leading_zeros() {
        assert_eq!(format_passkey(42), "000 042");
        assert_eq!(format_passkey(123_456), "123 456");
    }

    #[test]
    fn screens_cycle_both_ways() {
        for screen in SCREENS {
            assert_eq!(screen.next().previous(), screen);
            assert_ne!(screen.next(), screen);
        }
        assert_eq!(Screen::Aqi.next().next().next(), Screen::Aqi);
    }

    #[test]
    fn every_screen_fits_at_its_widest() {
        let wifi = online();
        let offline = WifiStatus::default();
        // One reading per AQI band, then the largest values the sensors report.
        let mut samples: Vec<SensorReadings> = [5, 20, 40, 100, 200, 300]
            .into_iter()
            .map(|pm2_5| readings(pm2_5, 0, 24.5))
            .collect();
        samples.push(readings(u16::MAX, u16::MAX, -40.0));
        samples.push(readings(u16::MAX, u16::MAX, 125.0));
        for screen in SCREENS {
            for sample in &samples {
                Frame::of(&model(screen, Some(sample), &wifi));
                Frame::of(&model(screen, Some(sample), &offline));
            }
            // Nothing measured yet, provisioned or not.
            Frame::of(&model(screen, None, &wifi));
            Frame::of(&ScreenModel {
                ssid: "",
                last_upload: None,
                provisioned: false,
                ..model(screen, None, &offline)
            });
            // The longest upload line.
            Frame::of(&ScreenModel {
                last_upload: Some((UploadResult::default(), 0)),
                ..model(screen, Some(&samples[0]), &wifi)
            });
        }
    }

    #[test]
    fn every_overlay_fits() {
        let wifi = online();
        let overlays = [
            Overlay::Starting,
            Overlay::Pairing {
                remaining_s: 299,
                passkey: None,
                connected: false,
            },
            Overlay::Pairing {
                remaining_s: 10,
                passkey: Some(7),
                connected: true,
            },
            Overlay::Updating { percent: 100 },
        ];
        for overlay in overlays {
            Frame::of(&ScreenModel {
                overlay: Some(overlay),
                ..model(Screen::Aqi, None, &wifi)
            });
        }
    }

    #[test]
    fn gauge_fills_only_the_current_band() {
        let wifi = online();
        let empty = Frame::of(&model(Screen::Aqi, None, &wifi));
        for pm2_5 in [5, 40, 300] {
            let sample = readings(pm2_5, 0, 24.5);
            let frame = Frame::of(&model(Screen::Aqi, Some(&sample), &wifi));
            let band = sample.aqi.unwrap().category.index();
            for index in 0..BANDS.len() {
                // The middle of each box: lit only when the box is filled.
                let x = 1 + 21 * index + 10;
                assert_eq!(frame.lit[59][x], index == band, "band {index}");
                assert!(!empty.lit[59][x]);
            }
        }
    }

    #[test]
    fn wifi_icon_changes_when_offline() {
        let online = online();
        let offline = WifiStatus::default();
        let up = Frame::of(&model(Screen::Aqi, None, &online));
        let down = Frame::of(&model(Screen::Aqi, None, &offline));
        assert_ne!(up.lit[..10], down.lit[..10]);
        assert_eq!(up.lit[10..], down.lit[10..]);
    }

    #[test]
    fn signal_bars_grow_with_the_signal() {
        let lit = |rssi| {
            let wifi = WifiStatus {
                rssi: Some(rssi),
                ..online()
            };
            Frame::of(&model(Screen::Network, None, &wifi)).lit_pixels()
        };
        assert!(lit(-50) > lit(-70));
        assert!(lit(-70) > lit(-90));
    }

    #[test]
    fn scaled_text_is_twice_the_size() {
        let wifi = online();
        let sample = readings(5, 0, 24.5);
        let frame = Frame::of(&model(Screen::Aqi, Some(&sample), &wifi));
        // Every lit pixel of the number belongs to a 2x2 block.
        for y in (12..40).step_by(2) {
            for x in (0..72).step_by(2) {
                let block = [
                    frame.lit[y][x],
                    frame.lit[y][x + 1],
                    frame.lit[y + 1][x],
                    frame.lit[y + 1][x + 1],
                ];
                assert!(block.iter().all(|lit| *lit == block[0]), "at {x},{y}");
            }
        }
    }

    #[test]
    fn network_screen_skips_upload_failures_before_setup() {
        let wifi = online();
        let failed = Some((UploadResult::default(), 0));
        let unprovisioned = |last_upload| {
            Frame::of(&ScreenModel {
                last_upload,
                provisioned: false,
                ..model(Screen::Network, None, &wifi)
            })
            .lit
        };
        assert_eq!(unprovisioned(failed), unprovisioned(None));
        let provisioned = Frame::of(&ScreenModel {
            last_upload: failed,
            ..model(Screen::Network, None, &wifi)
        });
        assert_ne!(provisioned.lit, unprovisioned(failed));
    }

    #[test]
    fn upload_line_reads_in_plain_words() {
        let ok = UploadResult {
            ok: true,
            http_code: 201,
        };
        let failed = |http_code| {
            Some((
                UploadResult {
                    ok: false,
                    http_code,
                },
                0,
            ))
        };
        assert_eq!(upload_line(None, 0), "No upload yet");
        assert_eq!(upload_line(Some((ok, 1_000)), 30_000), "Sent just now");
        assert_eq!(upload_line(Some((ok, 0)), 125_000), "Sent 2m ago");
        assert_eq!(upload_line(Some((ok, 0)), 3 * 3_600_000), "Sent 3h ago");
        assert_eq!(upload_line(failed(0), 0), "Send failed: offline");
        assert_eq!(upload_line(failed(-1), 0), "Send failed: network");
        assert_eq!(upload_line(failed(500), 0), "Send failed (500)");
        for line in [
            upload_line(Some((ok, 0)), u64::MAX),
            upload_line(failed(599), 0),
            upload_line(failed(i32::MIN), 0),
        ] {
            assert!(line.len() <= 20, "{line}");
        }
    }

    #[test]
    fn fit_shortens_long_names() {
        assert_eq!(fit("home", 17), "home");
        assert_eq!(fit("exactly seventeen", 17), "exactly seventeen");
        assert_eq!(fit("a much longer network name", 17), "a much longer n..");
        assert_eq!(fit("café ☕", 17), "caf? ?");
    }

    /// Prints every screen as ASCII art: `make screens`.
    #[test]
    #[ignore = "prints the screens for a visual check"]
    fn print_screens() {
        let wifi = online();
        let offline = WifiStatus::default();
        let sample = readings(40, 60, 24.5);
        let worst = readings(u16::MAX, u16::MAX, -40.0);
        let home = |screen, readings, wifi| ScreenModel {
            ssid: "Home WiFi",
            ..model(screen, readings, wifi)
        };
        for screen in SCREENS {
            println!(
                "{screen:?}\n{}",
                Frame::of(&home(screen, Some(&sample), &wifi)).art()
            );
            println!(
                "{screen:?}, offline, largest values\n{}",
                Frame::of(&model(screen, Some(&worst), &offline)).art()
            );
            println!(
                "{screen:?}, not set up\n{}",
                Frame::of(&ScreenModel {
                    ssid: "",
                    last_upload: None,
                    provisioned: false,
                    ..model(screen, None, &offline)
                })
                .art()
            );
        }
        for overlay in [Overlay::Starting, Overlay::Updating { percent: 40 }] {
            println!(
                "{overlay:?}\n{}",
                Frame::of(&ScreenModel {
                    overlay: Some(overlay),
                    ..model(Screen::Aqi, None, &wifi)
                })
                .art()
            );
        }
    }
}
