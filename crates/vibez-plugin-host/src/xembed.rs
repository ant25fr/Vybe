//! Host side of the XEmbed protocol.
//!
//! Most X11 editors map their own child window inside the parent the host
//! provides. XEmbed clients (GTK's `GtkPlug`) do not: they set
//! `_XEMBED_INFO` and wait for the embedder to map and size them. Without
//! this the host window stays black.

use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto::{self, ConnectionExt as _};

const XEMBED_VERSION: u32 = 1;
const XEMBED_MAPPED: u32 = 1;
const XEMBED_EMBEDDED_NOTIFY: u32 = 0;
const XEMBED_WINDOW_ACTIVATE: u32 = 1;
const XEMBED_FOCUS_IN: u32 = 4;

/// Map the XEmbed clients of `parent` that ask to be shown and make them
/// fill it. Cheap and idempotent: call it periodically, since a client may
/// appear after the editor has attached.
pub fn sync_clients<C: Connection>(
    conn: &C,
    parent: xproto::Window,
    width: u32,
    height: u32,
) -> Result<(), ReplyError> {
    let info = conn.intern_atom(false, b"_XEMBED_INFO")?.reply()?.atom;
    let xembed = conn.intern_atom(false, b"_XEMBED")?.reply()?.atom;
    for child in conn.query_tree(parent)?.reply()?.children {
        let property = conn
            .get_property(false, child, info, xproto::AtomEnum::ANY, 0, 2)?
            .reply()?;
        let Some(flags) = property.value32().and_then(|mut v| v.nth(1)) else {
            continue;
        };
        if flags & XEMBED_MAPPED == 0 {
            continue;
        }
        let geometry = conn.get_geometry(child)?.reply()?;
        if (u32::from(geometry.width), u32::from(geometry.height)) != (width, height) {
            conn.configure_window(
                child,
                &xproto::ConfigureWindowAux::new()
                    .x(0)
                    .y(0)
                    .width(width)
                    .height(height),
            )?;
        }
        let attributes = conn.get_window_attributes(child)?.reply()?;
        if attributes.map_state != xproto::MapState::UNMAPPED {
            continue;
        }
        let send = |message: u32, data1: u32, data2: u32| {
            let event = xproto::ClientMessageEvent::new(
                32,
                child,
                xembed,
                [x11rb::CURRENT_TIME, message, 0, data1, data2],
            );
            conn.send_event(false, child, xproto::EventMask::NO_EVENT, event)
        };
        send(XEMBED_EMBEDDED_NOTIFY, parent, XEMBED_VERSION)?;
        conn.map_window(child)?;
        send(XEMBED_WINDOW_ACTIVATE, 0, 0)?;
        send(XEMBED_FOCUS_IN, 0, 0)?;
    }
    conn.flush()?;
    Ok(())
}
