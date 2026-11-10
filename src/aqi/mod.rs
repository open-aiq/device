#[path = "bands.g.rs"]
mod bands;

pub use bands::{Band, BANDS};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AqiCategory {
    Good,
    Moderate,
    SlightlyUnhealthy,
    Unhealthy,
    VeryUnhealthy,
    Hazardous,
}

impl AqiCategory {
    pub const ALL: [Self; 6] = [
        Self::Good,
        Self::Moderate,
        Self::SlightlyUnhealthy,
        Self::Unhealthy,
        Self::VeryUnhealthy,
        Self::Hazardous,
    ];

    /// Position in [`BANDS`], from 0 (Good) to 5 (Hazardous).
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The band's name, shared with the app, dashboard and docs.
    pub const fn label(self) -> &'static str {
        BANDS[self.index()].label
    }

    /// The band's colour as `(r, g, b)`.
    pub const fn colour(self) -> (u8, u8, u8) {
        BANDS[self.index()].rgb
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pollutant {
    Pm25,
    Pm10,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AqiResult {
    pub aqi: u16,
    pub category: AqiCategory,
    pub dominant: Pollutant,
}

#[derive(Clone, Copy)]
struct Breakpoint {
    low: f32,
    high: f32,
    i_low: u16,
    i_high: u16,
    category: AqiCategory,
}

const PM25: [Breakpoint; 6] = [
    Breakpoint {
        low: 0.0,
        high: 9.0,
        i_low: 0,
        i_high: 50,
        category: AqiCategory::Good,
    },
    Breakpoint {
        low: 9.1,
        high: 35.4,
        i_low: 51,
        i_high: 100,
        category: AqiCategory::Moderate,
    },
    Breakpoint {
        low: 35.5,
        high: 55.4,
        i_low: 101,
        i_high: 150,
        category: AqiCategory::SlightlyUnhealthy,
    },
    Breakpoint {
        low: 55.5,
        high: 125.4,
        i_low: 151,
        i_high: 200,
        category: AqiCategory::Unhealthy,
    },
    Breakpoint {
        low: 125.5,
        high: 225.4,
        i_low: 201,
        i_high: 300,
        category: AqiCategory::VeryUnhealthy,
    },
    Breakpoint {
        low: 225.5,
        high: 325.4,
        i_low: 301,
        i_high: 500,
        category: AqiCategory::Hazardous,
    },
];

const PM10: [Breakpoint; 6] = [
    Breakpoint {
        low: 0.0,
        high: 54.0,
        i_low: 0,
        i_high: 50,
        category: AqiCategory::Good,
    },
    Breakpoint {
        low: 55.0,
        high: 154.0,
        i_low: 51,
        i_high: 100,
        category: AqiCategory::Moderate,
    },
    Breakpoint {
        low: 155.0,
        high: 254.0,
        i_low: 101,
        i_high: 150,
        category: AqiCategory::SlightlyUnhealthy,
    },
    Breakpoint {
        low: 255.0,
        high: 354.0,
        i_low: 151,
        i_high: 200,
        category: AqiCategory::Unhealthy,
    },
    Breakpoint {
        low: 355.0,
        high: 424.0,
        i_low: 201,
        i_high: 300,
        category: AqiCategory::VeryUnhealthy,
    },
    Breakpoint {
        low: 425.0,
        high: 604.0,
        i_low: 301,
        i_high: 500,
        category: AqiCategory::Hazardous,
    },
];

fn sub_index(value: f32, table: &[Breakpoint]) -> (u16, AqiCategory) {
    if value > table.last().expect("AQI table is not empty").high {
        return (500, AqiCategory::Hazardous);
    }
    let bp = table
        .iter()
        .find(|bp| value <= bp.high)
        .unwrap_or(&table[0]);
    let index = ((bp.i_high - bp.i_low) as f32 / (bp.high - bp.low) * (value - bp.low)
        + bp.i_low as f32)
        .round()
        .max(0.0) as u16;
    (index, bp.category)
}

pub fn compute_us_aqi(pm25: u16, pm10: u16) -> AqiResult {
    let (i25, c25) = sub_index(pm25 as f32, &PM25);
    let (i10, c10) = sub_index(pm10 as f32, &PM10);
    if i25 >= i10 {
        AqiResult {
            aqi: i25,
            category: c25,
            dominant: Pollutant::Pm25,
        }
    } else {
        AqiResult {
            aqi: i10,
            category: c10,
            dominant: Pollutant::Pm10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_breakpoints_and_caps() {
        assert_eq!(compute_us_aqi(0, 0).aqi, 0);
        assert_eq!(compute_us_aqi(9, 0).aqi, 50);
        assert_eq!(
            compute_us_aqi(36, 0).category,
            AqiCategory::SlightlyUnhealthy
        );
        assert_eq!(compute_us_aqi(400, 900).aqi, 500);
    }

    #[test]
    fn breakpoints_line_up_with_the_token_bands() {
        for table in [&PM25, &PM10] {
            for (bp, band) in table.iter().zip(BANDS) {
                assert_eq!(BANDS[bp.category.index()], band);
                // The top band is open-ended in the tokens and capped at 500 here.
                assert_eq!(band.max.unwrap_or(500), bp.i_high);
            }
        }
        assert_eq!(AqiCategory::SlightlyUnhealthy.label(), "Slightly Unhealthy");
        assert_eq!(AqiCategory::ALL.len(), BANDS.len());
    }

    #[test]
    fn ties_go_to_pm25() {
        assert_eq!(compute_us_aqi(0, 0).dominant, Pollutant::Pm25);
    }
}
