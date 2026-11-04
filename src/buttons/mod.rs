#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonEvent {
    Right,
    Left,
}

pub trait ButtonSource {
    fn poll(&mut self, now_ms: u64) -> Option<ButtonEvent>;
}

#[derive(Default)]
pub struct Debouncer {
    right_was_down: bool,
    left_was_down: bool,
    last_event_ms: u64,
}

#[cfg(target_os = "espidf")]
pub mod esp;

impl Debouncer {
    pub fn update(
        &mut self,
        now_ms: u64,
        right_down: bool,
        left_down: bool,
    ) -> Option<ButtonEvent> {
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
