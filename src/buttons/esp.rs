use esp_idf_svc::hal::gpio::{Input, InputPin, PinDriver, Pull};

use crate::buttons::{ButtonEvent, ButtonSource, Debouncer};

pub struct EspButtons {
    right: PinDriver<'static, Input>,
    left: PinDriver<'static, Input>,
    pair: PinDriver<'static, Input>,
    debounce: Debouncer,
}

impl EspButtons {
    pub fn new(
        right: impl InputPin + 'static,
        left: impl InputPin + 'static,
        pair: impl InputPin + 'static,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            right: PinDriver::input(right, Pull::Up)?,
            left: PinDriver::input(left, Pull::Up)?,
            // Boards without a pair button leave the pin floating, and the
            // pull-up keeps it reading as not pressed.
            pair: PinDriver::input(pair, Pull::Up)?,
            debounce: Debouncer::default(),
        })
    }
}

impl ButtonSource for EspButtons {
    fn poll(&mut self, now_ms: u64) -> Option<ButtonEvent> {
        self.debounce.update(
            now_ms,
            self.right.is_low(),
            self.left.is_low(),
            self.pair.is_low(),
        )
    }
}
