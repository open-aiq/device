use crate::config::DeviceConfig;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WifiStatus {
    pub connected: bool,
    pub ip: Option<String>,
    /// Signal strength of the connected network, in dBm.
    pub rssi: Option<i8>,
}

/// Signal strength as 0–4 bars, for the screen. The cutoffs match the app's
/// network list (`bars` in `app/lib/ble/protocol.dart`).
pub fn signal_bars(rssi: i8) -> u8 {
    match rssi {
        -55.. => 4,
        -65..=-56 => 3,
        -75..=-66 => 2,
        -85..=-76 => 1,
        _ => 0,
    }
}

/// One network found by a scan, as reported over BLE (`NET:<rssi>,<open|secured>,<ssid>`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WifiNetwork {
    pub ssid: String,
    pub rssi: i8,
    pub secured: bool,
}

/// Maximum networks returned by `SCAN`.
pub const SCAN_LIMIT: usize = 15;

impl WifiNetwork {
    pub fn ble_line(&self) -> String {
        format!(
            "NET:{},{},{}",
            self.rssi,
            if self.secured { "secured" } else { "open" },
            self.ssid
        )
    }
}

/// Drops hidden networks, keeps the strongest entry per SSID, sorts strongest first.
pub fn dedupe_networks(mut networks: Vec<WifiNetwork>) -> Vec<WifiNetwork> {
    networks.retain(|network| !network.ssid.is_empty());
    networks.sort_by_key(|network| std::cmp::Reverse(network.rssi));
    let mut unique: Vec<WifiNetwork> = Vec::new();
    for network in networks {
        if !unique.iter().any(|seen| seen.ssid == network.ssid) {
            unique.push(network);
        }
    }
    unique.truncate(SCAN_LIMIT);
    unique
}

pub trait WifiPort {
    type Error;
    fn connect(&mut self, config: &DeviceConfig) -> Result<bool, Self::Error>;
    fn status(&self) -> WifiStatus;
    fn scan(&mut self) -> Result<Vec<WifiNetwork>, Self::Error>;
}

#[cfg(target_os = "espidf")]
pub mod esp;

#[cfg(test)]
mod tests {
    use super::*;

    fn net(ssid: &str, rssi: i8) -> WifiNetwork {
        WifiNetwork {
            ssid: ssid.into(),
            rssi,
            secured: true,
        }
    }

    #[test]
    fn dedupes_sorts_and_limits() {
        let mut input = vec![
            net("home", -70),
            net("", -20),
            net("cafe", -40),
            net("home", -50),
        ];
        input.extend((0..20).map(|i| net(&format!("n{i}"), -90)));
        let result = dedupe_networks(input);
        assert_eq!(result.len(), SCAN_LIMIT);
        assert_eq!(result[0], net("cafe", -40));
        assert_eq!(result[1], net("home", -50));
        assert!(result.iter().all(|n| !n.ssid.is_empty()));
    }

    #[test]
    fn signal_bars_cover_the_range() {
        assert_eq!(signal_bars(-40), 4);
        assert_eq!(signal_bars(-55), 4);
        assert_eq!(signal_bars(-56), 3);
        assert_eq!(signal_bars(-65), 3);
        assert_eq!(signal_bars(-66), 2);
        assert_eq!(signal_bars(-75), 2);
        assert_eq!(signal_bars(-76), 1);
        assert_eq!(signal_bars(-85), 1);
        assert_eq!(signal_bars(-80), 1);
        assert_eq!(signal_bars(-86), 0);
        assert_eq!(signal_bars(i8::MIN), 0);
    }

    #[test]
    fn formats_ble_line_with_commas_in_ssid() {
        let network = WifiNetwork {
            ssid: "My, Wifi".into(),
            rssi: -61,
            secured: false,
        };
        assert_eq!(network.ble_line(), "NET:-61,open,My, Wifi");
    }
}
