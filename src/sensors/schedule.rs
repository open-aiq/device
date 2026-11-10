//! When the PMS5003 runs, settles, counts and rests.
//!
//! The fan and laser wear out with run time, so the sensor sleeps between
//! short bursts. Every cycle wakes the sensor, waits for the fan to settle,
//! averages every frame for a short window, then sleeps until the next cycle.
//! The why, with sources, is on the docs site's particle sensor page.

use crate::UPLOAD_INTERVAL_MS;

/// One wake, warm-up, sample and sleep cycle: two per 10-minute upload.
pub const PMS_CYCLE_MS: u64 = 5 * 60_000;

/// Time after waking before frames count. The datasheet asks for at least
/// 30 s for the fan to settle (`docs/datasheets/`, page 10).
pub const PMS_WARM_UP_MS: u64 = 30_000;

/// How long frames are averaged once settled: about 13 frames at the
/// sensor's steady 2.3 s output rate, more when the air changes fast.
pub const PMS_SAMPLE_MS: u64 = 30_000;

// Checked at compile time, so a future edit can't quietly break them.
const _: () = assert!(PMS_WARM_UP_MS >= 30_000, "datasheet minimum warm-up");
const _: () = assert!(
    PMS_WARM_UP_MS + PMS_SAMPLE_MS < PMS_CYCLE_MS,
    "the sensor must get some rest every cycle"
);
const _: () = assert!(
    UPLOAD_INTERVAL_MS % PMS_CYCLE_MS == 0,
    "every upload should average the same number of bursts"
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmsPhase {
    /// Fan and laser on; frames are ignored while the fan settles.
    WarmingUp,
    /// Fan and laser on; every frame is averaged.
    Sampling,
    /// Fan and laser off.
    Asleep,
}

/// Where the sensor should be at `ms_since_boot`. Cycles start at boot, so
/// the first reading after power-on also waits out the warm-up.
pub fn pms_phase(ms_since_boot: u64) -> PmsPhase {
    let into_cycle = ms_since_boot % PMS_CYCLE_MS;
    if into_cycle < PMS_WARM_UP_MS {
        PmsPhase::WarmingUp
    } else if into_cycle < PMS_WARM_UP_MS + PMS_SAMPLE_MS {
        PmsPhase::Sampling
    } else {
        PmsPhase::Asleep
    }
}

/// What the main loop should do with the sensor on this tick.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmsAction {
    Wake,
    Sleep,
    Nothing,
}

/// Decides when to wake and sleep the PMS5003 and which frames count.
///
/// It follows the sensor's real state, not phase changes, so a long blocking
/// task (an upload, an OTA download) can't skip a wake or shorten the warm-up.
#[derive(Debug)]
pub struct PmsDutyCycle {
    /// When the sensor was last woken, or `None` while it sleeps.
    awake_since_ms: Option<u64>,
}

impl PmsDutyCycle {
    /// For a sensor that is running as of `now_ms`, as it is after power-on.
    pub fn new_awake(now_ms: u64) -> Self {
        Self {
            awake_since_ms: Some(now_ms),
        }
    }

    /// Call on every loop tick and carry out the returned action.
    pub fn tick(&mut self, now_ms: u64) -> PmsAction {
        match (pms_phase(now_ms), self.awake_since_ms) {
            (PmsPhase::Asleep, Some(_)) => {
                self.awake_since_ms = None;
                PmsAction::Sleep
            }
            (PmsPhase::WarmingUp | PmsPhase::Sampling, None) => {
                self.awake_since_ms = Some(now_ms);
                PmsAction::Wake
            }
            _ => PmsAction::Nothing,
        }
    }

    pub fn is_awake(&self) -> bool {
        self.awake_since_ms.is_some()
    }

    /// Whether a frame arriving at `now_ms` belongs in the average: the cycle
    /// is in its sampling window and the sensor has been awake for the whole
    /// warm-up.
    pub fn counts_frames(&self, now_ms: u64) -> bool {
        pms_phase(now_ms) == PmsPhase::Sampling
            && self
                .awake_since_ms
                .is_some_and(|since| now_ms.saturating_sub(since) >= PMS_WARM_UP_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the duty cycle from boot in 50 ms loop ticks and records each
    /// wake or sleep with its time.
    fn actions_until(end_ms: u64) -> Vec<(u64, PmsAction)> {
        let mut duty = PmsDutyCycle::new_awake(0);
        (0..end_ms)
            .step_by(50)
            .map(|ms| (ms, duty.tick(ms)))
            .filter(|(_, action)| *action != PmsAction::Nothing)
            .collect()
    }

    #[test]
    fn wakes_and_sleeps_once_per_cycle() {
        assert_eq!(
            actions_until(2 * PMS_CYCLE_MS),
            vec![
                (60_000, PmsAction::Sleep),
                (PMS_CYCLE_MS, PmsAction::Wake),
                (PMS_CYCLE_MS + 60_000, PmsAction::Sleep),
            ]
        );
    }

    #[test]
    fn counts_frames_only_after_the_warm_up() {
        let mut duty = PmsDutyCycle::new_awake(0);
        assert!(!duty.counts_frames(0));
        assert!(!duty.counts_frames(29_999));
        assert!(duty.counts_frames(30_000));
        assert!(duty.counts_frames(59_999));
        assert_eq!(duty.tick(60_000), PmsAction::Sleep);
        assert!(!duty.counts_frames(60_000));
        assert!(!duty.is_awake());
    }

    #[test]
    fn a_late_wake_still_gets_the_full_warm_up() {
        let mut duty = PmsDutyCycle::new_awake(0);
        assert_eq!(duty.tick(60_000), PmsAction::Sleep);
        // The loop was blocked through the next wake-up time and comes back
        // in the middle of the sampling window.
        let late = PMS_CYCLE_MS + 40_000;
        assert_eq!(duty.tick(late), PmsAction::Wake);
        assert!(!duty.counts_frames(late));
        assert!(!duty.counts_frames(PMS_CYCLE_MS + 59_999));
        // That burst is skipped rather than shortened; the next one is normal.
        assert_eq!(duty.tick(PMS_CYCLE_MS + 60_000), PmsAction::Sleep);
        assert_eq!(duty.tick(2 * PMS_CYCLE_MS), PmsAction::Wake);
        assert!(duty.counts_frames(2 * PMS_CYCLE_MS + PMS_WARM_UP_MS));
    }

    #[test]
    fn first_cycle_warms_up_samples_then_sleeps() {
        assert_eq!(pms_phase(0), PmsPhase::WarmingUp);
        assert_eq!(pms_phase(29_999), PmsPhase::WarmingUp);
        assert_eq!(pms_phase(30_000), PmsPhase::Sampling);
        assert_eq!(pms_phase(59_999), PmsPhase::Sampling);
        assert_eq!(pms_phase(60_000), PmsPhase::Asleep);
        assert_eq!(pms_phase(PMS_CYCLE_MS - 1), PmsPhase::Asleep);
    }

    #[test]
    fn cycles_repeat() {
        for cycle in 1..5 {
            let start = cycle * PMS_CYCLE_MS;
            assert_eq!(pms_phase(start), PmsPhase::WarmingUp);
            assert_eq!(pms_phase(start + PMS_WARM_UP_MS), PmsPhase::Sampling);
            assert_eq!(
                pms_phase(start + PMS_WARM_UP_MS + PMS_SAMPLE_MS),
                PmsPhase::Asleep
            );
        }
    }

    #[test]
    fn runs_a_fifth_of_the_time() {
        let awake = (0..PMS_CYCLE_MS)
            .step_by(1_000)
            .filter(|&ms| pms_phase(ms) != PmsPhase::Asleep)
            .count();
        assert_eq!(awake, 60, "60 s awake in every 300 s cycle");
    }
}
