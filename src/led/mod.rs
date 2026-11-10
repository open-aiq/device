//! What the status LED shows, and when.
//!
//! The WS2812 on GPIO 48 is a diagnostics light. It makes states you can't
//! otherwise see (sensor faults, the PMS5003 warm-up, sample and rest
//! phases, pairing and updates) visible without a serial cable or a working
//! screen. This module is pure logic: `esp.rs` redraws it in the background.

/// Brightest any channel ever gets, about 10%. A WS2812 at full power is
/// glaring next to a bed.
pub const MAX_LEVEL: u8 = 26;

/// How long the boot fade-in lasts.
pub const BOOT_FADE_MS: u64 = 1_500;
/// How long a pairing result flashes before the LED moves on.
pub const PAIR_RESULT_MS: u64 = 1_500;
/// How long a frame tick stays bright while sampling.
pub const FRAME_TICK_MS: u64 = 120;
/// How often the resting heartbeat blinks while the PMS5003 sleeps.
pub const HEARTBEAT_MS: u64 = 5_000;
/// One blink in a fault code: half on, half off.
pub const BLINK_MS: u64 = 400;
/// Gap between fault codes, so they can be counted.
pub const CODE_GAP_MS: u64 = 2_000;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const OFF: Self = Self::new(0, 0, 0);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Scales by `permille` (0–1000), then caps at [`MAX_LEVEL`].
    fn at(self, permille: u32) -> Self {
        let scale = |c: u8| {
            let c = c as u32 * permille.min(1_000) / 1_000;
            (c * MAX_LEVEL as u32 / 255) as u8
        };
        Self::new(scale(self.r), scale(self.g), scale(self.b))
    }
}

const WHITE: Rgb = Rgb::new(255, 255, 255);
const RED: Rgb = Rgb::new(255, 0, 0);
const GREEN: Rgb = Rgb::new(0, 255, 0);
const BLUE: Rgb = Rgb::new(0, 0, 255);
const AMBER: Rgb = Rgb::new(255, 110, 0);
const YELLOW: Rgb = Rgb::new(255, 200, 0);

/// Board faults, numbered by their blink code. The lowest number wins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Fault {
    /// A whole burst passed with no frames, or reads keep failing.
    PmsSilent = 1,
    /// The SHT30 wasn't found, or its last read failed.
    Sht30 = 2,
    /// The OLED wasn't found at boot.
    Oled = 3,
    /// WiFi is provisioned but not connected.
    Wifi = 4,
    /// The backend didn't accept the last upload.
    Upload = 5,
    /// A new OTA image hasn't been verified by an upload yet.
    OtaUnverified = 6,
}

impl Fault {
    pub const fn blinks(self) -> u8 {
        self as u8
    }
}

/// A set of [`Fault`]s.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Faults(u8);

impl Faults {
    pub fn set(&mut self, fault: Fault, present: bool) {
        let bit = 1 << fault.blinks();
        if present {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }

    pub fn contains(self, fault: Fault) -> bool {
        self.0 & (1 << fault.blinks()) != 0
    }

    /// The fault to show: the one with the fewest blinks.
    pub fn first(self) -> Option<Fault> {
        [
            Fault::PmsSilent,
            Fault::Sht30,
            Fault::Oled,
            Fault::Wifi,
            Fault::Upload,
            Fault::OtaUnverified,
        ]
        .into_iter()
        .find(|fault| self.contains(*fault))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Pairing {
    #[default]
    Closed,
    /// Advertising, waiting for a phone.
    Open,
    /// A phone is connected, or a passkey is on screen.
    Connected,
}

/// Where the PMS5003 is in its duty cycle.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PmsState {
    /// Fan on, frames ignored while it settles. It starts this way at boot.
    #[default]
    WarmingUp,
    /// Fan on, every frame averaged.
    Sampling,
    /// Fan and laser off.
    Asleep,
}

/// Everything the LED needs to know, copied in by the main loop.
/// Times are in the LED clock's milliseconds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LedStatus {
    pub ota: bool,
    pub pairing: Pairing,
    /// Whether the last pairing worked, and when it finished.
    pub pair_result: Option<(bool, u64)>,
    pub provisioned: bool,
    pub pms: PmsState,
    /// When the last counted PMS5003 frame arrived.
    pub last_frame_ms: Option<u64>,
    pub faults: Faults,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pattern {
    Boot,
    Ota,
    PairResult { ok: bool, at_ms: u64 },
    PairConnected,
    PairOpen,
    Fault(Fault),
    Unprovisioned,
    WarmingUp,
    Sampling,
    Asleep,
}

/// Picks the one pattern to show; earlier lines win.
pub fn choose(status: &LedStatus, now_ms: u64) -> Pattern {
    if now_ms < BOOT_FADE_MS {
        return Pattern::Boot;
    }
    if status.ota {
        return Pattern::Ota;
    }
    if let Some((ok, at_ms)) = status.pair_result {
        if now_ms.saturating_sub(at_ms) < PAIR_RESULT_MS {
            return Pattern::PairResult { ok, at_ms };
        }
    }
    match status.pairing {
        Pairing::Connected => return Pattern::PairConnected,
        Pairing::Open => return Pattern::PairOpen,
        Pairing::Closed => {}
    }
    if let Some(fault) = status.faults.first() {
        return Pattern::Fault(fault);
    }
    if !status.provisioned {
        return Pattern::Unprovisioned;
    }
    match status.pms {
        PmsState::WarmingUp => Pattern::WarmingUp,
        PmsState::Sampling => Pattern::Sampling,
        PmsState::Asleep => Pattern::Asleep,
    }
}

/// The colour for `status` at `now_ms`.
pub fn colour(status: &LedStatus, now_ms: u64) -> Rgb {
    match choose(status, now_ms) {
        Pattern::Boot => WHITE.at((now_ms * 1_000 / BOOT_FADE_MS) as u32),
        Pattern::Ota => AMBER.at(breathe(now_ms, 2_000)),
        Pattern::PairResult { ok, at_ms } => {
            // Two 200 ms flashes, then dark until the result expires.
            let t = now_ms.saturating_sub(at_ms);
            let on = t < 800 && (t / 200) % 2 == 0;
            match (on, ok) {
                (false, _) => Rgb::OFF,
                (true, true) => GREEN.at(1_000),
                (true, false) => RED.at(1_000),
            }
        }
        Pattern::PairConnected => BLUE.at(1_000),
        Pattern::PairOpen => BLUE.at(breathe(now_ms, 2_000)),
        Pattern::Fault(fault) => {
            if blink_code_on(fault.blinks(), now_ms) {
                RED.at(1_000)
            } else {
                Rgb::OFF
            }
        }
        Pattern::Unprovisioned => {
            if (now_ms / 1_000) % 2 == 0 {
                YELLOW.at(1_000)
            } else {
                Rgb::OFF
            }
        }
        Pattern::WarmingUp => WHITE.at(breathe(now_ms, 3_000) / 2),
        Pattern::Sampling => {
            let ticking = status
                .last_frame_ms
                .is_some_and(|at| now_ms >= at && now_ms - at < FRAME_TICK_MS);
            GREEN.at(if ticking { 1_000 } else { 300 })
        }
        Pattern::Asleep => {
            if now_ms % HEARTBEAT_MS < 80 {
                GREEN.at(500)
            } else {
                Rgb::OFF
            }
        }
    }
}

/// A triangle wave between 10% and 100%, in permille.
fn breathe(now_ms: u64, period_ms: u64) -> u32 {
    let half = period_ms / 2;
    let t = now_ms % period_ms;
    let rising = if t < half { t } else { period_ms - t };
    (100 + rising * 900 / half) as u32
}

/// `blinks` short blinks, then a gap, repeating.
fn blink_code_on(blinks: u8, now_ms: u64) -> bool {
    let blinks = blinks as u64;
    let period = blinks * BLINK_MS + CODE_GAP_MS;
    let t = now_ms % period;
    t < blinks * BLINK_MS && t % BLINK_MS < BLINK_MS / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    const AFTER_BOOT: u64 = 10_000;

    fn healthy() -> LedStatus {
        LedStatus {
            provisioned: true,
            ..LedStatus::default()
        }
    }

    #[test]
    fn boot_fade_comes_first() {
        let status = LedStatus {
            ota: true,
            ..healthy()
        };
        assert_eq!(choose(&status, 0), Pattern::Boot);
        assert_eq!(colour(&status, 0), Rgb::OFF);
        assert_eq!(choose(&status, BOOT_FADE_MS), Pattern::Ota);
    }

    #[test]
    fn priority_order() {
        let mut status = LedStatus {
            ota: true,
            pairing: Pairing::Connected,
            pair_result: Some((true, AFTER_BOOT)),
            provisioned: false,
            ..LedStatus::default()
        };
        status.faults.set(Fault::Wifi, true);
        assert_eq!(choose(&status, AFTER_BOOT), Pattern::Ota);
        status.ota = false;
        assert!(matches!(
            choose(&status, AFTER_BOOT),
            Pattern::PairResult { ok: true, .. }
        ));
        status.pair_result = None;
        assert_eq!(choose(&status, AFTER_BOOT), Pattern::PairConnected);
        status.pairing = Pairing::Open;
        assert_eq!(choose(&status, AFTER_BOOT), Pattern::PairOpen);
        status.pairing = Pairing::Closed;
        assert_eq!(choose(&status, AFTER_BOOT), Pattern::Fault(Fault::Wifi));
        status.faults = Faults::default();
        assert_eq!(choose(&status, AFTER_BOOT), Pattern::Unprovisioned);
        status.provisioned = true;
        assert_eq!(choose(&status, AFTER_BOOT), Pattern::WarmingUp);
        status.pms = PmsState::Sampling;
        assert_eq!(choose(&status, AFTER_BOOT), Pattern::Sampling);
        status.pms = PmsState::Asleep;
        assert_eq!(choose(&status, AFTER_BOOT), Pattern::Asleep);
    }

    #[test]
    fn lowest_fault_code_wins() {
        let mut faults = Faults::default();
        faults.set(Fault::Upload, true);
        faults.set(Fault::Sht30, true);
        assert_eq!(faults.first(), Some(Fault::Sht30));
        faults.set(Fault::Sht30, false);
        assert_eq!(faults.first(), Some(Fault::Upload));
        faults.set(Fault::Upload, false);
        assert_eq!(faults.first(), None);
    }

    #[test]
    fn fault_blinks_can_be_counted() {
        for fault in [Fault::PmsSilent, Fault::Oled, Fault::OtaUnverified] {
            let mut status = healthy();
            status.faults.set(fault, true);
            let period = fault.blinks() as u64 * BLINK_MS + CODE_GAP_MS;
            let start = AFTER_BOOT - AFTER_BOOT % period + period;
            let mut rising_edges = 0;
            let mut was_on = false;
            for t in (start..start + period).step_by(10) {
                let on = colour(&status, t) != Rgb::OFF;
                if on && !was_on {
                    rising_edges += 1;
                }
                was_on = on;
            }
            assert_eq!(rising_edges, fault.blinks(), "{fault:?}");
        }
    }

    #[test]
    fn sampling_ticks_on_each_frame() {
        let status = LedStatus {
            pms: PmsState::Sampling,
            last_frame_ms: Some(AFTER_BOOT),
            ..healthy()
        };
        let bright = colour(&status, AFTER_BOOT);
        let dim = colour(&status, AFTER_BOOT + FRAME_TICK_MS);
        assert!(bright.g > dim.g);
        assert!(dim.g > 0, "sampling stays lit between frames");
    }

    #[test]
    fn pair_result_expires() {
        let status = LedStatus {
            pair_result: Some((false, AFTER_BOOT)),
            ..healthy()
        };
        assert_eq!(colour(&status, AFTER_BOOT), RED.at(1_000));
        assert_eq!(colour(&status, AFTER_BOOT + 250), Rgb::OFF);
        assert_eq!(
            choose(&status, AFTER_BOOT + PAIR_RESULT_MS),
            Pattern::WarmingUp
        );
    }

    #[test]
    fn asleep_is_dark_apart_from_a_heartbeat() {
        let status = LedStatus {
            pms: PmsState::Asleep,
            ..healthy()
        };
        assert_ne!(colour(&status, 2 * HEARTBEAT_MS), Rgb::OFF);
        assert_eq!(colour(&status, 2 * HEARTBEAT_MS + 1_000), Rgb::OFF);
    }

    #[test]
    fn never_brighter_than_the_cap() {
        let mut status = healthy();
        status.pairing = Pairing::Connected;
        for t in (AFTER_BOOT..AFTER_BOOT + 5_000).step_by(7) {
            let c = colour(&status, t);
            assert!(c.r <= MAX_LEVEL && c.g <= MAX_LEVEL && c.b <= MAX_LEVEL);
        }
        assert_eq!(BLUE.at(1_000).b, MAX_LEVEL);
    }
}
