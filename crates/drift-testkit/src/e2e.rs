//! Scripted remote input for the real-host e2e tests (plan §5.3), ported from the `probe2`
//! step runner.
//!
//! A [`Script`] is a list of [`Step`]s written in probe2's step language, for example the GDM
//! login used by `e2e_remote_login`:
//!
//! ```
//! # use drift_testkit::e2e::Script;
//! let script = Script::parse("wait:4|click:640,427|wait:2|text:@PW|key:Enter", &[("@PW", "…")]).unwrap();
//! assert_eq!(script.steps().len(), 5);
//! ```
//!
//! | Step | Meaning |
//! |---|---|
//! | `wait:<secs>` | pause (fractional seconds) |
//! | `click:<x>,<y>` | move + left press + release, desktop pixels |
//! | `move:<x>,<y>` | absolute pointer move |
//! | `text:<s>` | Unicode key down/up per UTF-16 unit (layout-independent, works in the GDM password field) |
//! | `key:<name>` | scancode press + release (`Enter`, `Tab`, `Esc`, `Down`, `Space`) |
//! | `ctrl:<k>`, `ctrlalt:<k>`, `super` | chords |
//! | `scroll:<units>` | vertical wheel, fractional-capable |
//!
//! Placeholders such as `@PW` are substituted from the caller's secrets **after** parsing, so a
//! [`Script`]'s `Debug` output never contains them.

use std::fmt;
use std::time::Duration;

use drift_core::{InputEvent, MouseButton};

/// One scripted step.
#[derive(Clone, PartialEq, Eq)]
pub enum Step {
    /// Pause.
    Wait(Duration),
    /// Left click at desktop pixels.
    Click {
        /// X.
        x: u16,
        /// Y.
        y: u16,
    },
    /// Absolute pointer move.
    Move {
        /// X.
        x: u16,
        /// Y.
        y: u16,
    },
    /// Unicode typing.
    Text(String),
    /// Scancode press + release.
    Key {
        /// Set-1 scancode.
        scancode: u8,
        /// Extended key.
        extended: bool,
    },
    /// Press all keys in order, release in reverse order.
    Chord(Vec<(u8, bool)>),
    /// Vertical wheel units (120 = one notch).
    Scroll(i16),
}

impl fmt::Debug for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wait(d) => write!(f, "Wait({d:?})"),
            Self::Click { x, y } => write!(f, "Click({x},{y})"),
            Self::Move { x, y } => write!(f, "Move({x},{y})"),
            // Typed text may be a password.
            Self::Text(t) => write!(f, "Text(<{} chars>)", t.chars().count()),
            Self::Key { scancode, extended } => write!(f, "Key({scancode:#04x}, ext={extended})"),
            Self::Chord(keys) => write!(f, "Chord({keys:?})"),
            Self::Scroll(u) => write!(f, "Scroll({u})"),
        }
    }
}

/// A script parse error (the offending step, never a substituted secret).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid script step `{0}`")]
pub struct ScriptError(pub String);

/// A parsed step list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Script {
    steps: Vec<Step>,
}

/// Desktop position (1280×800) of the "Drift e2e test user" tile in the GDM 50.1 greeter on
/// the homelab host, from the captured greeter screenshot (`/private/tmp/greeter.png`).
pub const GREETER_TEST_USER_TILE: (u16, u16) = (640, 427);

impl Script {
    /// Parses probe2's `|`-separated step language, replacing each placeholder in `text:`
    /// steps with its value.
    pub fn parse(src: &str, placeholders: &[(&str, &str)]) -> Result<Self, ScriptError> {
        let steps = src
            .split('|')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| parse_step(s, placeholders))
            .collect::<Result<_, _>>()?;
        Ok(Self { steps })
    }

    /// The GDM greeter login: wait for the greeter, click the test user's tile, type the
    /// password as Unicode events and press Enter (plan §5.3).
    pub fn gdm_login(password: &str) -> Self {
        let (x, y) = GREETER_TEST_USER_TILE;
        Self {
            steps: vec![
                Step::Wait(Duration::from_secs(4)),
                Step::Click { x, y },
                Step::Wait(Duration::from_secs(2)),
                Step::Text(password.to_owned()),
                Step::Wait(Duration::from_millis(300)),
                Step::Key { scancode: 0x1C, extended: false },
            ],
        }
    }

    /// The steps.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// Runs the script, sending each input event through `send` and sleeping for `Wait`s.
    pub async fn run(&self, mut send: impl FnMut(InputEvent)) {
        for step in &self.steps {
            match step {
                Step::Wait(d) => tokio::time::sleep(*d).await,
                other => {
                    for ev in other.input_events() {
                        send(ev);
                    }
                }
            }
        }
    }
}

impl Step {
    /// The wire-level input events of this step (`Wait` has none).
    pub fn input_events(&self) -> Vec<InputEvent> {
        match self {
            Self::Wait(_) => Vec::new(),
            Self::Click { x, y } => vec![
                InputEvent::MouseMove { x: *x, y: *y },
                InputEvent::MouseButton { button: MouseButton::Left, down: true, x: *x, y: *y },
                InputEvent::MouseButton { button: MouseButton::Left, down: false, x: *x, y: *y },
            ],
            Self::Move { x, y } => vec![InputEvent::MouseMove { x: *x, y: *y }],
            Self::Text(t) => t
                .encode_utf16()
                .flat_map(|ch| {
                    [InputEvent::Unicode { ch, down: true }, InputEvent::Unicode { ch, down: false }]
                })
                .collect(),
            Self::Key { scancode, extended } => vec![
                InputEvent::Key { scancode: *scancode, extended: *extended, down: true },
                InputEvent::Key { scancode: *scancode, extended: *extended, down: false },
            ],
            Self::Chord(keys) => keys
                .iter()
                .map(|&(scancode, extended)| InputEvent::Key { scancode, extended, down: true })
                .chain(keys.iter().rev().map(|&(scancode, extended)| InputEvent::Key {
                    scancode,
                    extended,
                    down: false,
                }))
                .collect(),
            Self::Scroll(units) => vec![InputEvent::Wheel { horizontal: false, units: *units }],
        }
    }
}

fn parse_step(step: &str, placeholders: &[(&str, &str)]) -> Result<Step, ScriptError> {
    let bad = || ScriptError(step.to_owned());
    let (kind, value) = step.split_once(':').unwrap_or((step, ""));
    let xy = |v: &str| -> Result<(u16, u16), ScriptError> {
        let (a, b) = v.split_once(',').ok_or_else(bad)?;
        Ok((a.trim().parse().map_err(|_| bad())?, b.trim().parse().map_err(|_| bad())?))
    };
    let letter = |v: &str| -> Result<u8, ScriptError> {
        Ok(match v {
            "a" => 0x1E,
            "c" => 0x2E,
            "l" => 0x26,
            "t" => 0x14,
            "v" => 0x2F,
            _ => return Err(bad()),
        })
    };
    Ok(match kind {
        "wait" => {
            let secs: f64 = value.parse().map_err(|_| bad())?;
            if !(0.0..=3600.0).contains(&secs) {
                return Err(bad());
            }
            Step::Wait(Duration::from_secs_f64(secs))
        }
        "click" => {
            let (x, y) = xy(value)?;
            Step::Click { x, y }
        }
        "move" => {
            let (x, y) = xy(value)?;
            Step::Move { x, y }
        }
        "text" => {
            let mut text = value.to_owned();
            for (name, secret) in placeholders {
                text = text.replace(name, secret);
            }
            Step::Text(text)
        }
        "key" => {
            let (scancode, extended) = match value {
                "Enter" => (0x1C, false),
                "Tab" => (0x0F, false),
                "Esc" => (0x01, false),
                "Down" => (0x50, true),
                "Space" => (0x39, false),
                _ => return Err(bad()),
            };
            Step::Key { scancode, extended }
        }
        "ctrl" => Step::Chord(vec![(0x1D, false), (letter(value)?, false)]),
        "ctrlalt" => Step::Chord(vec![(0x1D, false), (0x38, false), (letter(value)?, false)]),
        "ctrlshift" => Step::Chord(vec![(0x1D, false), (0x2A, false), (letter(value)?, false)]),
        "super" => Step::Chord(vec![(0x5B, true)]),
        "scroll" => Step::Scroll(value.parse::<i16>().map_err(|_| bad())?.clamp(-255, 255)),
        _ => return Err(bad()),
    })
}
