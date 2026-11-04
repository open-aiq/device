use core::convert::TryInto;

use embedded_svc::wifi::{AuthMethod, ClientConfiguration, Configuration};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::modem::Modem,
    nvs::EspDefaultNvsPartition,
    wifi::{BlockingWifi, EspWifi},
};

use crate::{
    config::DeviceConfig,
    wifi::{dedupe_networks, WifiNetwork, WifiPort, WifiStatus},
};

pub struct EspWifiPort<'d> {
    wifi: BlockingWifi<EspWifi<'d>>,
}

impl<'d> EspWifiPort<'d> {
    pub fn new(
        modem: Modem<'d>,
        event_loop: EspSystemEventLoop,
        nvs: EspDefaultNvsPartition,
    ) -> anyhow::Result<Self> {
        let wifi = EspWifi::new(modem, event_loop.clone(), Some(nvs))?;
        Ok(Self {
            wifi: BlockingWifi::wrap(wifi, event_loop)?,
        })
    }
}

impl WifiPort for EspWifiPort<'_> {
    type Error = anyhow::Error;

    fn connect(&mut self, config: &DeviceConfig) -> Result<bool, Self::Error> {
        if !config.has_wifi() {
            return Ok(false);
        }
        let configuration = Configuration::Client(ClientConfiguration {
            ssid: config
                .ssid
                .as_str()
                .try_into()
                .map_err(|_| anyhow::anyhow!("SSID is too long"))?,
            password: config
                .password
                .as_str()
                .try_into()
                .map_err(|_| anyhow::anyhow!("WiFi password is too long"))?,
            auth_method: if config.password.is_empty() {
                AuthMethod::None
            } else {
                AuthMethod::WPA2Personal
            },
            ..Default::default()
        });
        self.wifi.set_configuration(&configuration)?;
        if !self.wifi.is_started()? {
            self.wifi.start()?;
        }
        if self.wifi.is_connected()? {
            self.wifi.disconnect()?;
        }
        self.wifi.connect()?;
        self.wifi.wait_netif_up()?;
        Ok(true)
    }

    fn status(&self) -> WifiStatus {
        let connected = self.wifi.is_connected().unwrap_or(false);
        let ip = connected
            .then(|| {
                self.wifi
                    .wifi()
                    .sta_netif()
                    .get_ip_info()
                    .ok()
                    .map(|value| value.ip.to_string())
            })
            .flatten();
        WifiStatus { connected, ip }
    }

    fn scan(&mut self) -> Result<Vec<WifiNetwork>, Self::Error> {
        if !self.wifi.is_started()? {
            if !matches!(self.wifi.get_configuration()?, Configuration::Client(_)) {
                self.wifi
                    .set_configuration(&Configuration::Client(ClientConfiguration::default()))?;
            }
            self.wifi.start()?;
        }
        let found = self.wifi.scan()?;
        Ok(dedupe_networks(
            found
                .into_iter()
                .map(|ap| WifiNetwork {
                    ssid: ap.ssid.as_str().to_owned(),
                    rssi: ap.signal_strength,
                    secured: !matches!(ap.auth_method, None | Some(AuthMethod::None)),
                })
                .collect(),
        ))
    }
}
