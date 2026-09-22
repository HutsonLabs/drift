//! Opt-in typing of the stored Linux password at the GDM greeter (tasks **M3-2**/**M7-3**). Pure.
//!
//! GDM 50.1 has no credential-injection API (plan §1.3), so a Remote Login reconnect always
//! stops at the greeter. When the user opted in (a `linux-login` secret is stored for the
//! profile, ADR M1-6), Drift types that password for them — but only into the **focused
//! password field**: the greeter first shows a user list, and the password field appears and
//! takes focus when the user clicks their tile. So the typist arms when the greeter appears,
//! waits for the user's first click, gives GDM [`TYPE_DELAY`] to show the field, then types
//! the password as Unicode key events (layout independent, verified at the GDM greeter) and
//! presses Enter. It fires at most once per greeter.

use std::time::{Duration, Instant};

use drift_core::InputEvent;

/// Delay between the user's click on their tile and typing (GDM animates the password field in).
pub const TYPE_DELAY: Duration = Duration::from_millis(1500);

/// The one-shot greeter typist (see the module docs).
#[derive(Debug, Default)]
pub struct GreeterTypist {
    armed: bool,
}

impl GreeterTypist {
    /// A disarmed typist.
    pub fn new() -> Self {
        Self::default()
    }

    /// The greeter appeared; arm if a password is stored.
    pub fn on_greeter(&mut self, has_password: bool) {
        todo!("{has_password} {}", self.armed)
    }

    /// The greeter is gone (user session, disconnect): disarm.
    pub fn disarm(&mut self) {
        todo!()
    }

    /// Observes user input sent to the greeter at `now`.
    pub fn on_input(&mut self, event: &InputEvent, now: Instant) {
        todo!("{event:?} {now:?}")
    }

    /// When [`Self::poll`] should run next.
    pub fn deadline(&self) -> Option<Instant> {
        todo!()
    }

    /// The key events to send now (`password` as Unicode down/up pairs, then Enter), once.
    pub fn poll(&mut self, now: Instant, password: &str) -> Option<Vec<InputEvent>> {
        todo!("{now:?} {}", password.len())
    }
}
