//! Out-of-process plugin sandboxing.
//!
//! Each external plugin instance runs inside its own `vibez-plugin-bridge`
//! subprocess (see [`helper`]). The DAW talks to it over framed stdin/stdout
//! (see [`protocol`]) through [`BridgeChild`], and [`RemotePluginInstance`]
//! adapts that channel to the in-process [`crate::instance::PluginInstance`]
//! trait, so the engine needs no changes.
//!
//! A plugin that segfaults, aborts or hangs only kills/wedges its bridge
//! process: the audio call times out, the device is silenced and reported as
//! a local failure, and the DAW keeps running. Loading is sandboxed too — a
//! crashing plugin factory kills the bridge, not the app.
//!
//! If the helper binary is not next to the app executable, [`BridgeChild::spawn`]
//! returns [`BridgeError::HelperMissing`] and callers are expected to fall
//! back to in-process hosting (same policy as the sandboxed scanner).

pub mod child;
pub mod helper;
pub mod protocol;
pub mod remote_instance;
pub mod sandbox;

use std::sync::Arc;
use std::time::Duration;

pub use child::{BridgeChild, BridgeError};
pub use protocol::{
    AudioContextWire, AudioRequest, AudioRequestMut, AudioResponseRef, BlockNoteWire,
    BridgeRequest, BridgeResponse, ControlFrame, NoteEvent, ParamDescriptorWire,
};
pub use remote_instance::{LoadedInfo, RemotePluginInstance};

use crate::format::PluginFormat;
use crate::info::PluginInfo;

/// Default block size handed to the bridge at load time; process calls are
/// chunked to this size, so the sandbox adds at most one block of latency.
pub const BRIDGE_MAX_BUFFER: u32 = 4096;

const LOAD_TIMEOUT: Duration = Duration::from_secs(30);

/// Spawn a bridge subprocess and load `info` into it.
///
/// Returns the ready-to-use remote instance plus the child handle (the UI
/// keeps the handle for GUI commands and state capture).
pub fn load_remote(
    info: &PluginInfo,
    is_instrument: bool,
    sample_rate: f64,
    saved_state: Option<Vec<u8>>,
) -> Result<(RemotePluginInstance, Arc<BridgeChild>), BridgeError> {
    load_remote_with_buffer(
        info,
        is_instrument,
        sample_rate,
        BRIDGE_MAX_BUFFER,
        saved_state,
    )
}

/// Same as [`load_remote`] with an explicit max buffer size.
pub fn load_remote_with_buffer(
    info: &PluginInfo,
    is_instrument: bool,
    sample_rate: f64,
    max_buffer_size: u32,
    saved_state: Option<Vec<u8>>,
) -> Result<(RemotePluginInstance, Arc<BridgeChild>), BridgeError> {
    let child = BridgeChild::spawn()?;
    log::info!(
        "loading {:?} plugin '{}' in sandbox bridge",
        info.format,
        info.name
    );
    let format = match info.format {
        PluginFormat::Clap => "clap",
        PluginFormat::Vst3 => "vst3",
    };
    let response = child.request(
        BridgeRequest::Load {
            format: format.to_string(),
            path: info.path.to_string_lossy().into_owned(),
            uid: info.id.uid.clone(),
            is_instrument_hint: is_instrument,
            sample_rate,
            max_buffer_size,
            state: saved_state,
        },
        LOAD_TIMEOUT,
    );
    match response {
        Ok(BridgeResponse::Loaded {
            name,
            is_instrument,
            has_gui,
            latency,
            descriptors,
            external_inputs,
        }) => {
            let loaded = LoadedInfo {
                name,
                is_instrument,
                has_gui,
                latency,
                descriptors,
                external_inputs,
                sample_rate,
                max_buffer_size,
            };
            Ok((
                RemotePluginInstance::new(Arc::clone(&child), &loaded),
                child,
            ))
        }
        Ok(_) => Err(BridgeError::Remote("unexpected load response".to_string())),
        Err(e) => Err(e),
    }
}
