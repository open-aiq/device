// Generated from docs/src/data/tokens.json by `make tokens:sync` in docs/. Don't edit.

/// One US EPA AQI band. `max` is `None` for the open-ended top band.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Band {
    pub max: Option<u16>,
    pub label: &'static str,
    pub rgb: (u8, u8, u8),
}

pub const BANDS: [Band; 6] = [
    Band {
        max: Some(50),
        label: "Good",
        rgb: (0xa8, 0xe0, 0x5f),
    },
    Band {
        max: Some(100),
        label: "Moderate",
        rgb: (0xfd, 0xd6, 0x4b),
    },
    Band {
        max: Some(150),
        label: "Slightly Unhealthy",
        rgb: (0xff, 0x9b, 0x57),
    },
    Band {
        max: Some(200),
        label: "Unhealthy",
        rgb: (0xfe, 0x6a, 0x69),
    },
    Band {
        max: Some(300),
        label: "Very Unhealthy",
        rgb: (0xa9, 0x7a, 0xbc),
    },
    Band {
        max: None,
        label: "Hazardous",
        rgb: (0xa8, 0x73, 0x83),
    },
];
