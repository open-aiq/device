use embedded_graphics::{
    mono_font::{
        ascii::{FONT_10X20, FONT_6X10},
        MonoTextStyle,
    },
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};

use crate::{
    sensors::{I2cBus, SensorError, SensorReadings},
    telemetry::UploadResult,
    wifi::WifiStatus,
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
    Aqi,
    Climate,
    Status,
}

impl Screen {
    pub fn next(self) -> Self {
        match self {
            Self::Aqi => Self::Climate,
            Self::Climate => Self::Status,
            Self::Status => Self::Aqi,
        }
    }
    pub fn previous(self) -> Self {
        match self {
            Self::Aqi => Self::Status,
            Self::Climate => Self::Aqi,
            Self::Status => Self::Climate,
        }
    }
}

pub struct ScreenModel<'a> {
    pub screen: Screen,
    pub readings: Option<&'a SensorReadings>,
    pub wifi: &'a WifiStatus,
    pub last_upload: Option<(UploadResult, u64)>,
    pub now_ms: u64,
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
        let common = [
            0xae, 0xd5, 0x80, 0xa8, 0x3f, 0xd3, 0x00, 0x40, 0xa1, 0xc8, 0xda, 0x12, 0x81, 0x7f,
            0xd9, 0xf1, 0xdb, 0x40, 0xa4, 0xa6,
        ];
        self.command(&common)?;
        match self.controller {
            OledController::Sh1106 => self.command(&[0xad, 0x8b])?,
            OledController::Ssd1306 => self.command(&[0x8d, 0x14, 0x20, 0x00])?,
        }
        self.command(&[0xaf])
    }

    fn command(&self, commands: &[u8]) -> Result<(), SensorError> {
        let mut packet = Vec::with_capacity(commands.len() + 1);
        packet.push(0x00);
        packet.extend_from_slice(commands);
        self.bus.write(self.address, &packet)
    }

    pub fn flush(&self) -> Result<(), SensorError> {
        for page in 0..8u8 {
            match self.controller {
                OledController::Sh1106 => self.command(&[0xb0 + page, 0x02, 0x10])?,
                OledController::Ssd1306 => self.command(&[0x21, 0, 127, 0x22, page, page])?,
            }
            let row = &self.buffer[usize::from(page) * 128..usize::from(page + 1) * 128];
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

pub fn render<D>(target: &mut D, model: &ScreenModel<'_>)
where
    D: DrawTarget<Color = BinaryColor> + OriginDimensions,
{
    let _ = target.clear(BinaryColor::Off);
    let small = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let large = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
    let center = TextStyleBuilder::new()
        .alignment(Alignment::Center)
        .baseline(Baseline::Top)
        .build();
    let _ = Rectangle::new(Point::new(0, 0), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(target);

    match model.screen {
        Screen::Aqi => match model
            .readings
            .and_then(|value| value.pm.as_ref().zip(value.aqi))
        {
            Some((pm, aqi)) => {
                let _ = Text::with_text_style(
                    &format!("AQI {}", aqi.aqi),
                    Point::new(64, 4),
                    large,
                    center,
                )
                .draw(target);
                let _ =
                    Text::with_text_style(aqi.category.label(), Point::new(64, 28), small, center)
                        .draw(target);
                let _ = Text::with_text_style(
                    &format!("PM2.5 {} ug/m3", pm.pm2_5),
                    Point::new(64, 46),
                    small,
                    center,
                )
                .draw(target);
            }
            None => message(target, "AQI --", "warming up...", large, small, center),
        },
        Screen::Climate => match model.readings.and_then(|value| value.climate.as_ref()) {
            Some(value) => {
                let _ = Text::with_baseline(
                    &format!("Temp  {:.1} C", value.temperature_c),
                    Point::new(6, 7),
                    small,
                    Baseline::Top,
                )
                .draw(target);
                let _ = Text::with_baseline(
                    &format!("Hum   {:.0} %", value.humidity),
                    Point::new(6, 25),
                    small,
                    Baseline::Top,
                )
                .draw(target);
                let _ = Text::with_baseline(
                    &format!("Feels {:.1} C", value.heat_index_c),
                    Point::new(6, 43),
                    small,
                    Baseline::Top,
                )
                .draw(target);
            }
            None => message(
                target,
                "Climate --",
                "SHT30 unavailable",
                small,
                small,
                center,
            ),
        },
        Screen::Status => {
            let wifi = if model.wifi.connected {
                model.wifi.ip.as_deref().unwrap_or("connected")
            } else {
                "DOWN"
            };
            let upload = match model.last_upload {
                Some((result, at)) => format!(
                    "Up {} {}m ago",
                    result.http_code,
                    model.now_ms.saturating_sub(at) / 60_000
                ),
                None => "Up: never".to_owned(),
            };
            let _ =
                Text::with_baseline("STATUS", Point::new(6, 6), small, Baseline::Top).draw(target);
            let _ = Text::with_baseline(
                &format!("WiFi {wifi}"),
                Point::new(6, 25),
                small,
                Baseline::Top,
            )
            .draw(target);
            let _ =
                Text::with_baseline(&upload, Point::new(6, 44), small, Baseline::Top).draw(target);
        }
    }
}

fn message<D: DrawTarget<Color = BinaryColor>>(
    target: &mut D,
    first: &str,
    second: &str,
    first_style: MonoTextStyle<'_, BinaryColor>,
    second_style: MonoTextStyle<'_, BinaryColor>,
    text_style: embedded_graphics::text::TextStyle,
) {
    let _ = Text::with_text_style(first, Point::new(64, 12), first_style, text_style).draw(target);
    let _ =
        Text::with_text_style(second, Point::new(64, 39), second_style, text_style).draw(target);
}
