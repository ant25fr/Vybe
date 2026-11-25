//! What a native Plucked String track is, for the device chain.

/// The track carries the built-in Plucked String and no plugin instrument.
pub fn is_native_plucked(track: &crate::state::UiTrack) -> bool {
    track.has_instrument
        && track.instrument_kind == Some(vibez_core::midi::InstrumentKind::PluckedString)
        && track.plugin_instrument_name.is_none()
        && track.plugin_instrument_ref.is_none()
}
