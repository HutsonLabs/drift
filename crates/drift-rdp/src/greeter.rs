//! Opt-in typing of the stored Linux password at the GDM greeter (tasks **M3-2**/**M7-3**). Pure.
//!
//! GDM 50.1 has no credential-injection API (plan §1.3), so a Remote Login reconnect always
//! stops at the greeter. When the user opted in (a `linux-login` secret is stored for the
//! profile, ADR M1-6), Drift types that password for them — but only into the **focused
//! password field**: the greeter shows a user list first, and the password field appears and
//! takes focus when the user clicks their tile. So the typist arms when the greeter appears,
//! waits for the user's first click, gives GDM [`TYPE_DELAY`] to show the field, then types the
//! password as Unicode key events (layout independent, verified at the GDM greeter, plan §1.5)
//! and presses Enter. It fires at most once per greeter.

use std::time::{Duration, Instant};

use drift_core::InputEvent;

/// Delay between the user's click on their tile and typing (GDM animates the password field in).
pub const TYPE_DELAY: Duration = Duration::from_millis(1500);

/// Set-1 scancode of Enter.
const ENTER: u8 = 0x1C;

/// The one-shot greeter typist (see the module docs).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GreeterTypist {
    armed: bool,
    due: Option<Instant>,
}

impl GreeterTypist {
    /// A disarmed typist.
    pub fn new() -> Self {
        Self::default()
    }

    /// The greeter appeared; arm if a password is stored.
    pub fn on_greeter(&mut self, has_password: bool) {
        self.armed = has_password;
        self.due = None;
    }

    /// The greeter is gone (user session, disconnect): disarm.
    pub fn disarm(&mut self) {
        self.armed = false;
        self.due = None;
    }

    /// Observes user input sent to the greeter at `now`: the release of a left click selects a
    /// user tile and focuses the password field.
    pub fn on_input(&mut self, event: &InputEvent, now: Instant) {
        let clicked = matches!(
            event,
            InputEvent::MouseButton { button: drift_core::MouseButton::Left, down: false, .. }
        );
        if self.armed && self.due.is_none() && clicked {
            self.due = Some(now + TYPE_DELAY);
        }
    }

    /// When [`Self::poll`] should run next.
    pub fn deadline(&self) -> Option<Instant> {
        self.due
    }

    /// The key events to send now (`password` as Unicode down/up pairs, then Enter), once.
    pub fn poll(&mut self, now: Instant, password: &str) -> Option<Vec<InputEvent>> {
        if !self.armed || self.due.is_none_or(|due| now < due) {
            return None;
        }
        self.disarm();
        let mut events: Vec<InputEvent> = password
            .encode_utf16()
            .flat_map(|ch| [InputEvent::Unicode { ch, down: true }, InputEvent::Unicode { ch, down: false }])
            .collect();
        events.push(InputEvent::Key { scancode: ENTER, extended: false, down: true });
        events.push(InputEvent::Key { scancode: ENTER, extended: false, down: false });
        Some(events)
    }
}

#[cfg(test)]
mod tests {
    use drift_core::MouseButton;

    use super::*;

    fn click(down: bool) -> InputEvent {
        InputEvent::MouseButton { button: MouseButton::Left, down, x: 10, y: 10 }
    }

    #[test]
    fn types_once_after_the_first_click() {
        let t0 = Instant::now();
        let mut t = GreeterTypist::new();
        t.on_greeter(true);
        assert_eq!(t.deadline(), None, "nothing until the user picks a tile");
        assert_eq!(t.poll(t0, "Fake9"), None);
        t.on_input(&click(true), t0);
        assert_eq!(t.deadline(), None, "the press alone does not arm the timer");
        t.on_input(&click(false), t0);
        assert_eq!(t.deadline(), Some(t0 + TYPE_DELAY));
        assert_eq!(t.poll(t0 + TYPE_DELAY - Duration::from_millis(1), "Fake9"), None);
        let events = t.poll(t0 + TYPE_DELAY, "hé").expect("types the password");
        assert_eq!(
            events,
            vec![
                InputEvent::Unicode { ch: u16::from(b'h'), down: true },
                InputEvent::Unicode { ch: u16::from(b'h'), down: false },
                InputEvent::Unicode { ch: 0xE9, down: true },
                InputEvent::Unicode { ch: 0xE9, down: false },
                InputEvent::Key { scancode: ENTER, extended: false, down: true },
                InputEvent::Key { scancode: ENTER, extended: false, down: false },
            ]
        );
        t.on_input(&click(false), t0 + TYPE_DELAY);
        assert_eq!(t.poll(t0 + TYPE_DELAY * 4, "hé"), None, "only once per greeter");
    }

    #[test]
    fn without_a_stored_password_nothing_is_typed() {
        let t0 = Instant::now();
        let mut t = GreeterTypist::new();
        t.on_greeter(false);
        t.on_input(&click(false), t0);
        assert_eq!(t.deadline(), None);
        assert_eq!(t.poll(t0 + TYPE_DELAY * 10, "Fake9"), None);
    }

    #[test]
    fn leaving_the_greeter_disarms() {
        let t0 = Instant::now();
        let mut t = GreeterTypist::new();
        t.on_greeter(true);
        t.on_input(&click(false), t0);
        t.disarm();
        assert_eq!(t.poll(t0 + TYPE_DELAY, "Fake9"), None);
    }
}
