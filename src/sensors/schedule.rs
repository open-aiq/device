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

#[cfg(test)]
mod tests {
    use super::*;

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
