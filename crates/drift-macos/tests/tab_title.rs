//! M6-2: tab title formatter (profile name + state glyph).
#![allow(missing_docs)]

use std::time::Duration;

use drift_core::{ConnectStage, DisconnectReason, SessionState, Size};
use drift_macos::tabs::{display_name, tab_title};

#[test]
fn title_per_state() {
    let cases = [
        (SessionState::Idle, "○ Homelab"),
        (SessionState::Connecting { leg: 1, stage: ConnectStage::Tcp }, "◌ Homelab"),
        (SessionState::AwaitingGreeterLogin, "◐ Homelab"),
        (SessionState::Connected { desktop: Size::new(1280, 800), scale: 100 }, "● Homelab"),
        (
            SessionState::Reconnecting {
                attempt: 2,
                next_in: Duration::from_secs(3),
                reason: DisconnectReason::Network,
            },
            "↻ Homelab",
        ),
        (SessionState::Disconnected { reason: DisconnectReason::UserClosed }, "○ Homelab"),
        (SessionState::Failed { reason: DisconnectReason::AuthFailed }, "⚠ Homelab"),
    ];
    for (state, want) in cases {
        assert_eq!(tab_title("Homelab", &state), want, "{state:?}");
    }
}

#[test]
fn blank_or_padded_names_fall_back_and_trim() {
    let connected = SessionState::Connected { desktop: Size::new(1280, 800), scale: 100 };
    assert_eq!(tab_title("  Work box  ", &connected), "● Work box");
    assert_eq!(tab_title("   ", &connected), "● Untitled");
    assert_eq!(tab_title("", &SessionState::Idle), "○ Untitled");
}

#[test]
fn control_characters_are_stripped() {
    assert_eq!(tab_title("a\nb\tc", &SessionState::Idle), "○ a b c");
}

/// UI-tabs: the HTML tab strip shows the same cleaned name as the window title.
#[test]
fn display_names_are_cleaned_like_titles() {
    assert_eq!(display_name("  Work box  "), "Work box");
    assert_eq!(display_name(" \t "), "Untitled");
    assert_eq!(display_name("a\nb\tc"), "a b c");
}
