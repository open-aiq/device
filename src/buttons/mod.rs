#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonEvent {
    Right,
    Left,
    /// Both buttons held for [`PAIR_HOLD_MS`]: open the BLE pairing window.
    PairHold,
}

/// How long both buttons must be held to enter BLE pairing mode.
pub const PAIR_HOLD_MS: u64 = 3_000;

pub trait ButtonSource {
    fn poll(&mut self, now_ms: u64) -> Option<ButtonEvent>;
}

#[derive(Default)]
pub struct Debouncer {
    right_was_down: bool,
    left_was_down: bool,
    last_event_ms: u64,
    both_down_since: Option<u64>,
    hold_fired: bool,
}

#[cfg(target_os = "espidf")]
pub mod esp;
#[cfg(all(target_os = "espidf", feature = "serial-buttons"))]
pub mod serial;

impl Debouncer {
    pub fn update(
        &mut self,
        now_ms: u64,
        right_down: bool,
        left_down: bool,
    ) -> Option<ButtonEvent> {
        if right_down && left_down {
            let since = *self.both_down_since.get_or_insert(now_ms);
            self.right_was_down = true;
            self.left_was_down = true;
            if !self.hold_fired && now_ms.saturating_sub(since) >= PAIR_HOLD_MS {
                self.hold_fired = true;
                self.last_event_ms = now_ms;
                return Some(ButtonEvent::PairHold);
            }
            return None;
        }
        self.both_down_since = None;
        self.hold_fired = false;

        let event = if right_down && !self.right_was_down {
            Some(ButtonEvent::Right)
        } else if left_down && !self.left_was_down {
            Some(ButtonEvent::Left)
        } else {
            None
        };
        self.right_was_down = right_down;
        self.left_was_down = left_down;
        match event {
            Some(event) if now_ms.saturating_sub(self.last_event_ms) >= 150 => {
                self.last_event_ms = now_ms;
                Some(event)
            }
            _ => None,
        }
    }
}

/// A line typed on the serial console that stands in for a physical button.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialInput {
    Button(ButtonEvent),
    Help,
}

pub const SERIAL_HELP: &str = "Serial buttons: BTN:LEFT | BTN:RIGHT | BTN:PAIR | BTN:HELP";

/// Parses one console line (`BTN:LEFT`, `BTN:RIGHT`, `BTN:PAIR`, `BTN:HELP`), case-insensitively.
pub fn parse_serial_input(line: &str) -> Option<SerialInput> {
    let line = line.trim();
    let (prefix, value) = line.split_once(':')?;
    if !prefix.trim().eq_ignore_ascii_case("BTN") {
        return None;
    }
    match value.trim().to_ascii_uppercase().as_str() {
        "LEFT" | "L" => Some(SerialInput::Button(ButtonEvent::Left)),
        "RIGHT" | "R" => Some(SerialInput::Button(ButtonEvent::Right)),
        "PAIR" | "P" => Some(SerialInput::Button(ButtonEvent::PairHold)),
        "HELP" | "?" => Some(SerialInput::Help),
        _ => None,
    }
}

/// Polls several button sources and returns the first event any of them produced.
pub struct CombinedButtons<A, B> {
    pub first: A,
    pub second: B,
}

impl<A: ButtonSource, B: ButtonSource> ButtonSource for CombinedButtons<A, B> {
    fn poll(&mut self, now_ms: u64) -> Option<ButtonEvent> {
        self.first.poll(now_ms).or_else(|| self.second.poll(now_ms))
    }
}

impl<T: ButtonSource + ?Sized> ButtonSource for Box<T> {
    fn poll(&mut self, now_ms: u64) -> Option<ButtonEvent> {
        (**self).poll(now_ms)
    }
}

/// A source that never produces events; used when a source is compiled out.
pub struct NoButtons;

impl ButtonSource for NoButtons {
    fn poll(&mut self, _now_ms: u64) -> Option<ButtonEvent> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_presses_are_edge_triggered_and_debounced() {
        let mut d = Debouncer::default();
        assert_eq!(d.update(1_000, true, false), Some(ButtonEvent::Right));
        assert_eq!(d.update(1_050, true, false), None);
        assert_eq!(d.update(1_100, false, false), None);
        assert_eq!(d.update(1_120, false, true), None, "debounced");
        assert_eq!(d.update(1_400, false, false), None);
        assert_eq!(d.update(1_500, false, true), Some(ButtonEvent::Left));
    }

    #[test]
    fn holding_both_buttons_fires_pair_hold_once() {
        let mut d = Debouncer::default();
        assert_eq!(d.update(0, true, true), None);
        assert_eq!(d.update(2_999, true, true), None);
        assert_eq!(d.update(3_000, true, true), Some(ButtonEvent::PairHold));
        assert_eq!(d.update(6_000, true, true), None, "fires once per hold");
        assert_eq!(d.update(6_100, false, false), None);
        assert_eq!(d.update(7_000, true, true), None);
        assert_eq!(d.update(10_000, true, true), Some(ButtonEvent::PairHold));
    }

    #[test]
    fn releasing_one_button_cancels_the_hold() {
        let mut d = Debouncer::default();
        d.update(0, true, true);
        d.update(2_000, true, false);
        assert_eq!(d.update(2_500, true, true), None);
        assert_eq!(d.update(5_000, true, true), None);
        assert_eq!(d.update(5_500, true, true), Some(ButtonEvent::PairHold));
    }

    #[test]
    fn parses_serial_button_lines() {
        assert_eq!(
            parse_serial_input("btn:pair\r"),
            Some(SerialInput::Button(ButtonEvent::PairHold))
        );
        assert_eq!(
            parse_serial_input(" BTN: Left "),
            Some(SerialInput::Button(ButtonEvent::Left))
        );
        assert_eq!(
            parse_serial_input("BTN:RIGHT"),
            Some(SerialInput::Button(ButtonEvent::Right))
        );
        assert_eq!(parse_serial_input("BTN:HELP"), Some(SerialInput::Help));
        assert_eq!(parse_serial_input("BTN:JUMP"), None);
        assert_eq!(parse_serial_input("PAIR"), None);
        assert_eq!(parse_serial_input(""), None);
    }

    #[test]
    fn combined_sources_poll_both() {
        struct One(Option<ButtonEvent>);
        impl ButtonSource for One {
            fn poll(&mut self, _: u64) -> Option<ButtonEvent> {
                self.0.take()
            }
        }
        let mut c = CombinedButtons {
            first: One(None),
            second: One(Some(ButtonEvent::PairHold)),
        };
        assert_eq!(c.poll(0), Some(ButtonEvent::PairHold));
        assert_eq!(c.poll(0), None);
    }
}
