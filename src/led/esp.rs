//! Drives the WS2812 from a background thread.
//!
//! A WS2812 holds its last colour on its own, so animating means rewriting
//! it. The RMT peripheral clocks out each 24-bit frame in hardware (~30 µs),
//! and this thread redraws about 30 times a second. It keeps going while the
//! main loop is blocked in a WiFi connect, an upload or an OTA download.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use esp_idf_svc::hal::{
    delay::FreeRtos,
    gpio::OutputPin,
    rmt::{config::TxChannelConfig, encoder::CopyEncoder, PinState, Symbol, TxChannelDriver},
    units::Hertz,
};

use super::{colour, LedStatus, Rgb};

const RESOLUTION: Hertz = Hertz(10_000_000);
const FRAME_MS: u32 = 33;

/// The main loop's side of the LED: update the status, the thread redraws.
/// Cloning shares the same LED.
#[derive(Clone)]
pub struct LedHandle {
    status: Arc<Mutex<LedStatus>>,
    epoch: Instant,
}

impl LedHandle {
    /// Milliseconds on the LED clock, for timestamps in [`LedStatus`].
    pub fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    pub fn update(&self, change: impl FnOnce(&mut LedStatus)) {
        if let Ok(mut status) = self.status.lock() {
            change(&mut status);
        }
    }

    /// A PMS5003 frame was counted: flash the sampling tick.
    pub fn frame(&self) {
        let now = self.now_ms();
        self.update(|status| status.last_frame_ms = Some(now));
    }

    /// A pairing attempt finished: flash green or red.
    pub fn pairing_finished(&self, ok: bool) {
        let now = self.now_ms();
        self.update(|status| status.pair_result = Some((ok, now)));
    }
}

/// Starts the LED thread. If the LED can't start, the handle still works and
/// the firmware carries on without it.
pub fn start(pin: impl OutputPin + 'static) -> LedHandle {
    let handle = LedHandle {
        status: Arc::new(Mutex::new(LedStatus::default())),
        epoch: Instant::now(),
    };
    match spawn(pin, handle.clone()) {
        Ok(()) => log::info!("Status LED started"),
        Err(error) => log::error!("Status LED unavailable: {error:#}"),
    }
    handle
}

fn spawn(pin: impl OutputPin + 'static, handle: LedHandle) -> anyhow::Result<()> {
    let mut channel = TxChannelDriver::new(
        pin,
        &TxChannelConfig {
            resolution: RESOLUTION,
            ..Default::default()
        },
    )?;
    let bits = Bits::new()?;
    std::thread::Builder::new()
        .name("status-led".into())
        .stack_size(4096)
        .spawn(move || {
            // The encoder can't cross threads, so it's made here.
            let mut encoder = match CopyEncoder::new() {
                Ok(encoder) => encoder,
                Err(error) => {
                    log::error!("Status LED encoder unavailable: {error:?}");
                    return;
                }
            };
            let mut shown = None;
            loop {
                let status = handle.status.lock().map(|s| *s).unwrap_or_default();
                let rgb = colour(&status, handle.now_ms());
                if shown != Some(rgb) {
                    let frame = bits.frame(rgb);
                    match channel.send_and_wait(&mut encoder, &frame, &Default::default()) {
                        Ok(()) => shown = Some(rgb),
                        Err(error) => log::warn!("Status LED write failed: {error:?}"),
                    }
                }
                FreeRtos::delay_ms(FRAME_MS);
            }
        })?;
    Ok(())
}

/// WS2812 bit timings, as RMT symbols.
struct Bits {
    zero: Symbol,
    one: Symbol,
}

impl Bits {
    fn new() -> anyhow::Result<Self> {
        let symbol = |high_ns, low_ns| {
            Symbol::new_with(
                RESOLUTION,
                PinState::High,
                Duration::from_nanos(high_ns),
                PinState::Low,
                Duration::from_nanos(low_ns),
            )
        };
        Ok(Self {
            zero: symbol(400, 850)?,
            one: symbol(800, 450)?,
        })
    }

    /// One pixel, green first, most significant bit first. The line idles
    /// low between frames, which latches the colour.
    fn frame(&self, rgb: Rgb) -> [Symbol; 24] {
        let mut frame = [self.zero; 24];
        for (i, byte) in [rgb.g, rgb.r, rgb.b].into_iter().enumerate() {
            for bit in 0..8 {
                if byte & (0x80 >> bit) != 0 {
                    frame[i * 8 + bit] = self.one;
                }
            }
        }
        frame
    }
}
