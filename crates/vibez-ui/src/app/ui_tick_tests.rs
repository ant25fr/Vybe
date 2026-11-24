//! The UI tick is the redraw rate: every message makes iced rebuild the
//! whole view, so the interval follows what actually has to animate.

use std::time::Duration;

use super::test_support::app;
use crate::state::PlaybackPhase;

#[test]
fn idle_tick_is_ten_hertz() {
    let app = app();
    assert_eq!(app.ui_tick_interval(), Duration::from_millis(100));
}

#[test]
fn playback_tick_is_thirty_hertz() {
    let mut app = app();
    app.state.transport.playback = PlaybackPhase::Playing;
    assert_eq!(app.ui_tick_interval(), Duration::from_millis(33));
    app.state.transport.playback = PlaybackPhase::Starting;
    assert_eq!(app.ui_tick_interval(), Duration::from_millis(33));
}

#[test]
fn interaction_tick_stays_at_ui_tick_rate() {
    let mut app = app();
    // An arrangement edit drag needs 60 fps edge auto-scroll.
    app.state.arrangement.edit_drag.mark_active();
    assert_eq!(
        app.ui_tick_interval(),
        Duration::from_millis(vibez_core::constants::UI_TICK_MS)
    );
}
