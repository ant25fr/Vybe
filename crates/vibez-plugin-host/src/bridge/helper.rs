//! `vibez-plugin-bridge` subprocess entry point.
//!
//! The bridge hosts ONE plugin instance in its own process. Its main thread
//! is the plugin's "UI thread" (JUCE binds its MessageManager here — the
//! same rule the in-process loader follows) and runs a platform message loop
//! for the plugin window; a dedicated worker thread reads stdin frames and
//! acts as the plugin's processing thread:
//!
//! - control frames are forwarded to the main thread (params, state, GUI,
//!   lifecycle are main-thread class in CLAP/VST3),
//! - audio and note frames are executed inline on the worker thread, in
//!   wire order, so sample-accurate note scheduling survives the hop. Audio
//!   payloads are processed in place (zero-copy frame views).
//!
//! The plugin window is sized to the plugin's own preferred size (and follows
//! plugin-initiated resizes) on Windows, macOS and Linux.
//!
//! A crashing plugin kills this process; the parent detects it (EOF /
//! timeout / non-zero exit) and bypasses the device. The DAW is unaffected.

use std::cell::RefCell;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use vibez_core::routing::{ExternalInputBlock, ExternalInputId};

use super::protocol::*;
use crate::gui::PluginGuiHandle;
use crate::instance::PluginInstance;

thread_local! {
    /// The plugin GUI handle. Only ever touched from the bridge main thread
    /// (control handling + window procedure + runloop), like the in-process
    /// UI thread.
    static GUI: RefCell<Option<PluginGuiHandle>> = const { RefCell::new(None) };
}

fn with_gui<R>(f: impl FnOnce(&mut PluginGuiHandle) -> R) -> Option<R> {
    GUI.with(|g| g.borrow_mut().as_mut().map(f))
}

struct Shared {
    instance: Mutex<Option<Box<dyn PluginInstance>>>,
    stdout: Mutex<std::io::Stdout>,
}

impl Shared {
    fn with_inst<R>(&self, f: impl FnOnce(&mut Box<dyn PluginInstance>) -> R) -> Result<R, String> {
        // A plugin panic while holding the lock must not poison the bridge.
        let mut guard = self.instance.lock().unwrap_or_else(|e| e.into_inner());
        let inst = guard
            .as_mut()
            .ok_or_else(|| "no plugin loaded".to_string())?;
        Ok(f(inst))
    }

    fn write_frame(&self, frame: &[u8]) -> bool {
        let mut out = self.stdout.lock().unwrap_or_else(|e| e.into_inner());
        out.write_all(frame).is_ok() && out.flush().is_ok()
    }
}

/// Bridge binary entry point.
pub fn run() -> ExitCode {
    // Narrow what this process may do before any plugin code or thread exists.
    super::sandbox::harden_current_process();
    // This thread is the bridge's main thread: register it as the CLAP main
    // thread before any plugin code runs.
    crate::set_clap_main_thread();
    platform::init();

    let stdout = std::io::stdout();
    let shared = Arc::new(Shared {
        instance: Mutex::new(None),
        stdout: Mutex::new(stdout),
    });
    let (main_tx, main_rx) = mpsc::channel::<ControlFrame>();
    let quit = Arc::new(AtomicBool::new(false));

    // Worker thread: audio + notes + context, plus forwarding control
    // requests to the main thread.
    {
        let shared = Arc::clone(&shared);
        let quit = Arc::clone(&quit);
        std::thread::Builder::new()
            .name("vibez-bridge-audio".into())
            .spawn(move || {
                let mut stdin = std::io::stdin().lock();
                loop {
                    if quit.load(Ordering::SeqCst) {
                        break;
                    }
                    match read_frame(&mut stdin) {
                        Ok(Some(frame)) => match frame.kind() {
                            KIND_CONTROL => {
                                match serde_json::from_slice::<ControlFrame>(frame.payload()) {
                                    Ok(req @ ControlFrame::Request { .. }) => {
                                        if main_tx.send(req).is_err() {
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            KIND_AUDIO_REQ => {
                                let seq = frame.seq();
                                let mut frame = frame;
                                let resp = handle_audio(&shared, frame.payload_mut());
                                let out = encode_frame(
                                    KIND_AUDIO_RESP,
                                    seq,
                                    &encode_audio_response(resp.0, resp.1, &resp.2, resp.3, resp.4),
                                );
                                if !shared.write_frame(&out) {
                                    break;
                                }
                            }
                            KIND_NOTE => {
                                if let Some(ev) = decode_note_event(frame.payload()) {
                                    let _ = shared.with_inst(|inst| {
                                        if ev.on {
                                            inst.note_on_at(ev.pitch, ev.velocity, ev.frame);
                                        } else {
                                            inst.note_off_at(ev.pitch, ev.frame);
                                        }
                                    });
                                }
                            }
                            KIND_CONTEXT => {
                                if let Some(ctx) = decode_audio_context(frame.payload()) {
                                    let _ = shared.with_inst(|inst| {
                                        inst.set_audio_context(ctx.into());
                                    });
                                }
                            }
                            _ => {}
                        },
                        Ok(None) => break,
                        Err(_) => break,
                    }
                }
            })
            .expect("spawn bridge worker thread");
    }

    // Main thread: control requests + platform message loop.
    loop {
        match main_rx.recv_timeout(Duration::from_millis(2)) {
            Ok(ControlFrame::Request { seq, req }) => {
                let is_quit = matches!(req, BridgeRequest::Quit);
                let result = handle_control(&shared, req);
                let frame = encode_control(&ControlFrame::Response { seq, result });
                if !shared.write_frame(&frame) {
                    break;
                }
                if is_quit {
                    break;
                }
            }
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        crate::poll_clap_events();
        // VST3 Linux editors repaint and handle input only from the host run
        // loop: without this the window stays blank.
        let _ = with_gui(|g| g.service_runloop());
        // Follow plugin-initiated resizes and keep the window matched to the
        // plugin's size.
        if let Some((w, h)) = take_requested_resize() {
            platform::resize_to_content(w, h);
            let _ = with_gui(|g| g.set_size(w, h));
        }
        if platform::take_gui_closed() {
            close_gui();
            let frame = encode_control(&ControlFrame::Event(BridgeEvent::GuiClosed));
            let _ = shared.write_frame(&frame);
        }
        platform::pump_messages();
    }

    quit.store(true, Ordering::SeqCst);
    ExitCode::SUCCESS
}

/// Process one audio block in place. Returns
/// `(ok, buffer, notes, dropped, latency)`; `buffer` aliases the payload.
fn handle_audio<'a>(
    shared: &Shared,
    payload: &'a mut [u8],
) -> (bool, &'a [f32], Vec<BlockNoteWire>, u64, u32) {
    let Some(req) = decode_audio_request_mut(payload) else {
        return (false, &[], Vec::new(), 0, 0);
    };
    let channels = req.channels.max(1);
    let blocks: Vec<ExternalInputBlock<'_>> = req
        .inputs
        .iter()
        .enumerate()
        .map(|(i, (ch, samples))| ExternalInputBlock {
            id: ExternalInputId(i as u32),
            channels: *ch,
            samples,
            connected: true,
        })
        .collect();
    let buf: &'a mut [f32] = req.buffer;
    let mut guard = shared.instance.lock().unwrap_or_else(|e| e.into_inner());
    let Some(inst) = guard.as_mut() else {
        return (false, &[], Vec::new(), 0, 0);
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        inst.process_with_inputs(buf, channels, &blocks);
    }));
    let latency = inst.latency_samples();
    match result {
        Ok(()) => {
            let (notes, dropped) = match inst.note_output() {
                Some(n) => (
                    n.notes().iter().map(|b| BlockNoteWire::from(*b)).collect(),
                    n.dropped(),
                ),
                None => (Vec::new(), 0),
            };
            (true, buf, notes, dropped, latency)
        }
        Err(_) => {
            // The plugin panicked inside the bridge: report failure but keep
            // the bridge alive so the parent can still save state / quit.
            for s in buf.iter_mut() {
                *s = 0.0;
            }
            (false, buf, Vec::new(), 0, latency)
        }
    }
}

fn loaded_response(inst: &dyn PluginInstance, has_gui: bool) -> BridgeResponse {
    BridgeResponse::Loaded {
        name: inst.name().to_string(),
        is_instrument: inst.is_instrument(),
        has_gui,
        latency: inst.latency_samples(),
        descriptors: inst
            .param_descriptors_vec()
            .iter()
            .map(ParamDescriptorWire::from)
            .collect(),
        external_inputs: inst.external_inputs().to_vec(),
    }
}

fn handle_control(shared: &Shared, req: BridgeRequest) -> Result<BridgeResponse, String> {
    match req {
        BridgeRequest::Load {
            format,
            path,
            uid,
            is_instrument_hint,
            sample_rate,
            max_buffer_size,
            state,
        } => {
            let path = Path::new(&path);
            match format.as_str() {
                "clap" => {
                    let partial = crate::clap_host::instance::ClapPluginInstance::load_partial(
                        path,
                        &uid,
                        is_instrument_hint,
                    )?;
                    let inst =
                        crate::clap_host::instance::ClapPluginInstance::init_on_main_thread_with_state(
                            partial,
                            sample_rate,
                            max_buffer_size,
                            state.as_deref(),
                            false,
                        )?;
                    let gui = inst.extract_gui_handle().map(PluginGuiHandle::Clap);
                    let has_gui = gui.is_some();
                    let resp = loaded_response(&inst, has_gui);
                    *shared.instance.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(Box::new(inst));
                    GUI.with(|g| *g.borrow_mut() = gui);
                    Ok(resp)
                }
                "vst3" => {
                    let partial = crate::vst3_host::instance::Vst3PluginInstance::load_partial(
                        path,
                        &uid,
                        is_instrument_hint,
                    )?;
                    let inst =
                        crate::vst3_host::instance::Vst3PluginInstance::init_on_main_thread_with_state(
                            partial,
                            sample_rate,
                            max_buffer_size,
                            state.as_deref(),
                            false,
                        )?;
                    let gui = inst.extract_gui_handle().map(PluginGuiHandle::Vst3);
                    let has_gui = gui.is_some();
                    let resp = loaded_response(&inst, has_gui);
                    *shared.instance.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(Box::new(inst));
                    GUI.with(|g| *g.borrow_mut() = gui);
                    Ok(resp)
                }
                other => Err(format!("unknown plugin format '{other}'")),
            }
        }
        BridgeRequest::Prepare {
            sample_rate,
            max_buffer_size,
        } => shared.with_inst(|inst| {
            inst.prepare(sample_rate, max_buffer_size);
            BridgeResponse::Unit
        }),
        BridgeRequest::Activate => shared.with_inst(|inst| BridgeResponse::Bool(inst.activate())),
        BridgeRequest::Deactivate => shared.with_inst(|inst| {
            inst.deactivate();
            BridgeResponse::Unit
        }),
        BridgeRequest::StopProcessing => shared.with_inst(|inst| {
            inst.stop_processing();
            BridgeResponse::Unit
        }),
        BridgeRequest::Reset => shared.with_inst(|inst| {
            inst.reset();
            BridgeResponse::Unit
        }),
        BridgeRequest::SetParam { index, value } => shared.with_inst(|inst| {
            let _ = inst.set_param(index, value);
            BridgeResponse::Unit
        }),
        BridgeRequest::GetParam { index } => {
            shared.with_inst(|inst| BridgeResponse::Param(inst.get_param(index)))
        }
        BridgeRequest::SaveState => {
            shared.with_inst(|inst| BridgeResponse::State(inst.save_state()))
        }
        BridgeRequest::LoadState { data } => {
            shared.with_inst(|inst| BridgeResponse::Bool(inst.load_state(&data)))
        }
        BridgeRequest::NoteOutput => shared.with_inst(|inst| match inst.note_output() {
            Some(n) => BridgeResponse::NoteOutput {
                notes: n.notes().iter().map(|b| BlockNoteWire::from(*b)).collect(),
                dropped: n.dropped(),
            },
            None => BridgeResponse::NoteOutput {
                notes: Vec::new(),
                dropped: 0,
            },
        }),
        BridgeRequest::OpenGui { width, height } => open_gui(width, height),
        BridgeRequest::CloseGui => {
            close_gui();
            Ok(BridgeResponse::Unit)
        }
        BridgeRequest::Quit => Ok(BridgeResponse::Unit),
    }
}

/// Open the plugin's editor inside the bridge's own window, sized to the
/// plugin's preferred size (fallback: the requested size).
fn open_gui(width: u32, height: u32) -> Result<BridgeResponse, String> {
    if platform::is_visible() {
        platform::raise_window();
        return Ok(BridgeResponse::Bool(true));
    }
    let Some(gui) = GUI.with(|g| g.borrow_mut().take()) else {
        return Ok(BridgeResponse::Bool(false));
    };
    let mut gui = gui;
    // The editor may still be attached to a window that is no longer shown.
    if gui.is_open() {
        gui.hide();
        gui.destroy();
    }
    if !gui.create_gui() {
        GUI.with(|g| *g.borrow_mut() = Some(gui));
        return Ok(BridgeResponse::Bool(false));
    }
    // The plugin's own size wins over the requested one.
    let size = gui.get_size().unwrap_or((width.max(320), height.max(240)));
    if !platform::ensure_window(size.0, size.1, "vibez plugin (sandboxed)") {
        gui.destroy();
        GUI.with(|g| *g.borrow_mut() = Some(gui));
        return Err("cannot create the plugin window".to_string());
    }
    if !unsafe { gui.open_embedded(platform::parent_handle()) } {
        gui.destroy();
        GUI.with(|g| *g.borrow_mut() = Some(gui));
        platform::hide_window();
        return Ok(BridgeResponse::Bool(false));
    }
    // Re-read after embedding (some plugins resize on attach) and match the
    // window to the final plugin size.
    let size = gui.get_size().unwrap_or(size);
    platform::resize_to_content(size.0, size.1);
    let _ = gui.set_size(size.0, size.1);
    #[cfg(not(any(windows, target_os = "macos")))]
    platform::set_resizable(gui.can_resize(), size.0, size.1);
    platform::show_window();
    GUI.with(|g| *g.borrow_mut() = Some(gui));
    Ok(BridgeResponse::Bool(true))
}

/// A resize the plugin asked for: VST3 `resizeView` or CLAP `request_resize`.
fn take_requested_resize() -> Option<(u32, u32)> {
    let clap = crate::clap_host::host_impl::take_pending_gui_resizes();
    with_gui(|g| {
        g.take_pending_resize().or_else(|| {
            let ptr = g.clap_plugin_ptr()?;
            clap.iter()
                .find(|(p, _, _)| *p == ptr)
                .map(|&(_, w, h)| (w, h))
        })
    })
    .flatten()
}

/// Destroy the editor but keep its handle, so it can be opened again.
fn close_gui() {
    let _ = with_gui(|g| {
        if g.is_open() {
            g.hide();
            g.destroy();
        }
    });
    platform::hide_window();
}

// ── Platform window layer ──
//
// One module per platform. Each provides: init, ensure_window (create or
// resize to a client size), parent_handle (the GuiParent to embed into),
// resize_to_content, show/hide/raise/is_visible, take_gui_closed, and
// pump_messages (dispatch pending window-system events; forwards user
// resizes to the plugin via the GUI thread-local).

#[cfg(windows)]
mod platform {
    use std::cell::Cell;
    use std::sync::atomic::{AtomicBool, Ordering};

    use windows_sys::Win32::Foundation::{
        GetLastError, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, RECT, WPARAM,
    };
    use windows_sys::Win32::Graphics::Gdi::ValidateRect;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    use crate::gui::GuiParent;

    static mut BRIDGE_HWND: HWND = std::ptr::null_mut();
    static GUI_CLOSED: AtomicBool = AtomicBool::new(false);
    static REGISTERED: AtomicBool = AtomicBool::new(false);

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_CLOSE => {
                GUI_CLOSED.store(true, Ordering::SeqCst);
                ShowWindow(hwnd, SW_HIDE);
                0
            }
            WM_SIZE => {
                // Forward the new client size to the plugin editor.
                let w = (lparam & 0xFFFF) as u32;
                let h = (lparam >> 16) as u32;
                if w > 0 && h > 0 {
                    let _ = super::with_gui(|g| {
                        if let Some((w, h)) = g.resize_from_host(w, h) {
                            let _ = g.set_size(w, h);
                        }
                    });
                }
                0
            }
            WM_DESTROY => {
                BRIDGE_HWND = std::ptr::null_mut();
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    pub fn init() {}

    /// Create the bridge window if needed and set its client area to
    /// `width` x `height`.
    pub fn ensure_window(width: u32, height: u32, title: &str) -> bool {
        unsafe {
            if !REGISTERED.load(Ordering::SeqCst) {
                let hinstance = GetModuleHandleW(std::ptr::null());
                let class = wide("VibezPluginBridgeWnd");
                let mut wc: WNDCLASSEXW = std::mem::zeroed();
                wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
                wc.style = CS_HREDRAW | CS_VREDRAW;
                wc.lpfnWndProc = Some(wnd_proc);
                wc.hInstance = hinstance;
                wc.lpszClassName = class.as_ptr();
                wc.hbrBackground = std::ptr::null_mut();
                let atom = RegisterClassExW(&wc);
                if atom == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                    return false;
                }
                REGISTERED.store(true, Ordering::SeqCst);
            }
            if BRIDGE_HWND.is_null() {
                let hinstance = GetModuleHandleW(std::ptr::null());
                let class = wide("VibezPluginBridgeWnd");
                let title_w = wide(title);
                let hwnd = CreateWindowExW(
                    0,
                    class.as_ptr(),
                    title_w.as_ptr(),
                    WS_OVERLAPPEDWINDOW,
                    CW_USEDEFAULT,
                    CW_USEDEFAULT,
                    CW_USEDEFAULT,
                    CW_USEDEFAULT,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    hinstance,
                    std::ptr::null(),
                );
                if hwnd.is_null() {
                    return false;
                }
                BRIDGE_HWND = hwnd;
            }
            resize_to_content(width, height);
            true
        }
    }

    pub fn parent_handle() -> GuiParent {
        GuiParent::Win32(unsafe { BRIDGE_HWND } as *mut std::ffi::c_void)
    }

    /// Resize the window so its CLIENT area is width x height.
    pub fn resize_to_content(width: u32, height: u32) {
        unsafe {
            let hwnd = BRIDGE_HWND;
            if hwnd.is_null() {
                return;
            }
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            };
            AdjustWindowRectEx(&mut rect, style, 0, ex_style);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    pub fn show_window() {
        unsafe {
            if !BRIDGE_HWND.is_null() {
                ShowWindow(BRIDGE_HWND, SW_SHOW);
            }
        }
    }

    pub fn hide_window() {
        unsafe {
            if !BRIDGE_HWND.is_null() {
                ShowWindow(BRIDGE_HWND, SW_HIDE);
            }
        }
    }

    pub fn raise_window() {
        unsafe {
            if !BRIDGE_HWND.is_null() {
                SetForegroundWindow(BRIDGE_HWND);
                ShowWindow(BRIDGE_HWND, SW_RESTORE);
            }
        }
    }

    pub fn is_visible() -> bool {
        unsafe { !BRIDGE_HWND.is_null() && IsWindowVisible(BRIDGE_HWND) != 0 }
    }

    pub fn take_gui_closed() -> bool {
        GUI_CLOSED.swap(false, Ordering::SeqCst)
    }

    pub fn pump_messages() {
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            // Avoid a busy loop when no message is pending.
            if BRIDGE_HWND.is_null() {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }

    // Keep GDI import used (ValidateRect silences unused-import on some
    // feature sets; the class brush is null so no painting is needed).
    #[allow(dead_code)]
    unsafe fn _keep(_h: HWND) {
        let r = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let _ = ValidateRect(std::ptr::null_mut(), &r);
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::cell::{Cell, RefCell};

    use objc2::rc::Retained;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSView, NSWindow,
        NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

    use crate::gui::GuiParent;

    thread_local! {
        static STATE: RefCell<Option<(Retained<NSWindow>, Retained<NSView>)>> =
            const { RefCell::new(None) };
        static CLOSED: Cell<bool> = const { Cell::new(false) };
        static WAS_VISIBLE: Cell<bool> = const { Cell::new(false) };
    }

    fn ns_size((w, h): (u32, u32)) -> NSSize {
        NSSize::new(w as f64, h as f64)
    }

    pub fn init() {
        // JUCE/AppKit need a shared NSApplication on the main thread.
        if let Some(mtm) = MainThreadMarker::new() {
            let app = NSApplication::sharedApplication(mtm);
            unsafe {
                app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
            }
        }
    }

    pub fn ensure_window(width: u32, height: u32, title: &str) -> bool {
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        STATE.with(|s| {
            let mut slot = s.borrow_mut();
            if slot.is_none() {
                let style = NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::Miniaturizable
                    | NSWindowStyleMask::Resizable;
                let window = unsafe {
                    NSWindow::initWithContentRect_styleMask_backing_defer(
                        NSWindow::alloc(mtm),
                        NSRect::new(NSPoint::new(0.0, 0.0), ns_size((width, height))),
                        style,
                        NSBackingStoreType::Buffered,
                        false,
                    )
                };
                unsafe {
                    window.setReleasedWhenClosed(false);
                }
                window.setTitle(&NSString::from_str(title));
                let Some(view) = window.contentView() else {
                    return false;
                };
                *slot = Some((window, view));
            }
            true
        })
    }

    pub fn parent_handle() -> GuiParent {
        STATE.with(|s| match &*s.borrow() {
            Some((_, view)) => GuiParent::Cocoa(Retained::as_ptr(view) as *mut std::ffi::c_void),
            None => GuiParent::Cocoa(std::ptr::null_mut()),
        })
    }

    pub fn resize_to_content(width: u32, height: u32) {
        STATE.with(|s| {
            if let Some((window, _)) = &*s.borrow() {
                window.setContentSize(ns_size((width, height)));
            }
        });
    }

    pub fn show_window() {
        STATE.with(|s| {
            if let Some((window, _)) = &*s.borrow() {
                window.center();
                window.makeKeyAndOrderFront(None);
                WAS_VISIBLE.set(true);
            }
        });
    }

    pub fn hide_window() {
        STATE.with(|s| {
            if let Some((window, _)) = &*s.borrow() {
                window.orderOut(None);
            }
        });
    }

    pub fn raise_window() {
        show_window();
    }

    pub fn is_visible() -> bool {
        STATE.with(|s| {
            s.borrow()
                .as_ref()
                .is_some_and(|(window, _)| window.isVisible())
        })
    }

    pub fn take_gui_closed() -> bool {
        CLOSED.replace(false)
    }

    pub fn pump_messages() {
        // Run the run loop briefly so AppKit/JUCE deliver events, then check
        // whether the user closed the window.
        unsafe {
            core_foundation::runloop::CFRunLoopRunInMode(
                core_foundation::runloop::kCFRunLoopDefaultMode,
                0.0,
                1,
            );
        }
        STATE.with(|s| {
            if let Some((window, _)) = &*s.borrow() {
                let visible = window.isVisible();
                let was = WAS_VISIBLE.replace(visible);
                if was && !visible {
                    CLOSED.set(true);
                }
            }
        });
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    use std::cell::{Cell, RefCell};
    use std::time::{Duration, Instant};

    use x11rb::connection::Connection;
    use x11rb::properties::WmSizeHints;
    use x11rb::protocol::xproto::{self, ConnectionExt as _};
    use x11rb::protocol::Event;
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    use crate::gui::GuiParent;

    struct X11 {
        conn: RustConnection,
        win: u32,
        wm_delete: u32,
        mapped: Cell<bool>,
        /// Last size known to the plugin, to ignore moves and echoes.
        size: Cell<(u32, u32)>,
        resizable: Cell<bool>,
        last_embed: Cell<Instant>,
    }

    impl X11 {
        /// Map the window and wait for the server: the plugin parents its
        /// editor from its own connection, so the window must exist there.
        fn map(&self) {
            let _ = self.conn.map_window(self.win);
            let _ = self.conn.sync();
            self.mapped.set(true);
        }

        /// The window opens at a provisional size before the editor reports
        /// its own. Drop the notifications of those sizes, which would
        /// otherwise reach the editor as a resize by the user.
        fn discard_stale_sizes(&self) {
            // Give the window manager time to apply the final size.
            std::thread::sleep(Duration::from_millis(50));
            let _ = self.conn.sync();
            while let Ok(Some(event)) = self.conn.poll_for_event() {
                self.note_close_request(&event);
            }
        }

        fn note_close_request(&self, event: &Event) {
            if let Event::ClientMessage(cm) = event {
                if cm.format == 32 && cm.data.as_data32()[0] == self.wm_delete {
                    CLOSED.set(true);
                }
            }
        }

        /// A fixed-size editor locks the window with min == max hints.
        fn apply_size_hints(&self, width: u32, height: u32) {
            let lock = (!self.resizable.get()).then_some((width as i32, height as i32));
            let hints = WmSizeHints {
                min_size: lock,
                max_size: lock,
                ..Default::default()
            };
            let _ = hints.set_normal_hints(&self.conn, self.win);
        }
    }

    thread_local! {
        static STATE: RefCell<Option<X11>> = const { RefCell::new(None) };
        static CLOSED: Cell<bool> = const { Cell::new(false) };
    }

    fn intern_atom(conn: &RustConnection, name: &str) -> Option<u32> {
        conn.intern_atom(false, name.as_bytes())
            .ok()?
            .reply()
            .ok()
            .map(|r| r.atom)
    }

    pub fn init() {}

    pub fn ensure_window(width: u32, height: u32, title: &str) -> bool {
        STATE.with(|s| {
            let mut slot = s.borrow_mut();
            if slot.is_none() {
                let (conn, screen_num) = match RustConnection::connect(None) {
                    Ok(v) => v,
                    Err(_) => return false,
                };
                let screen = conn.setup().roots[screen_num].clone();
                let wm_protocols = intern_atom(&conn, "WM_PROTOCOLS").unwrap_or(0);
                let wm_delete = intern_atom(&conn, "WM_DELETE_WINDOW").unwrap_or(0);
                let win = match conn.generate_id() {
                    Ok(id) => id,
                    Err(_) => return false,
                };
                if conn
                    .create_window(
                        x11rb::COPY_DEPTH_FROM_PARENT,
                        win,
                        screen.root,
                        100,
                        100,
                        width.max(320) as u16,
                        height.max(240) as u16,
                        0,
                        xproto::WindowClass::INPUT_OUTPUT,
                        0,
                        &xproto::CreateWindowAux::new()
                            .event_mask(xproto::EventMask::STRUCTURE_NOTIFY),
                    )
                    .is_err()
                {
                    return false;
                }
                let _ = conn.change_property8(
                    xproto::PropMode::REPLACE,
                    win,
                    xproto::AtomEnum::WM_NAME,
                    xproto::AtomEnum::STRING,
                    title.as_bytes(),
                );
                if wm_protocols != 0 && wm_delete != 0 {
                    let _ = conn.change_property32(
                        xproto::PropMode::REPLACE,
                        win,
                        wm_protocols,
                        xproto::AtomEnum::ATOM,
                        &[wm_delete],
                    );
                }
                *slot = Some(X11 {
                    conn,
                    win,
                    wm_delete,
                    mapped: Cell::new(false),
                    size: Cell::new((width.max(320), height.max(240))),
                    resizable: Cell::new(true),
                    last_embed: Cell::new(Instant::now()),
                });
            }
            true
        }) && {
            resize_to_content(width, height);
            STATE.with(|s| {
                if let Some(x) = &*s.borrow() {
                    x.map();
                }
            });
            true
        }
    }

    pub fn set_resizable(resizable: bool, width: u32, height: u32) {
        STATE.with(|s| {
            if let Some(x) = &*s.borrow() {
                x.resizable.set(resizable);
                x.apply_size_hints(width, height);
                let _ = x.conn.flush();
            }
        });
    }

    pub fn parent_handle() -> GuiParent {
        STATE.with(|s| {
            s.borrow()
                .as_ref()
                .map(|x| GuiParent::X11(x.win))
                .unwrap_or(GuiParent::X11(0))
        })
    }

    pub fn resize_to_content(width: u32, height: u32) {
        STATE.with(|s| {
            if let Some(x) = &*s.borrow() {
                x.size.set((width.max(1), height.max(1)));
                // Move the lock first or the window manager refuses the size.
                x.apply_size_hints(width.max(1), height.max(1));
                let _ = x.conn.configure_window(
                    x.win,
                    &xproto::ConfigureWindowAux::new()
                        .width(width.max(1))
                        .height(height.max(1)),
                );
                let _ = x.conn.flush();
            }
        });
    }

    pub fn show_window() {
        STATE.with(|s| {
            if let Some(x) = &*s.borrow() {
                x.map();
                x.discard_stale_sizes();
            }
        });
    }

    pub fn hide_window() {
        STATE.with(|s| {
            if let Some(x) = &*s.borrow() {
                let _ = x.conn.unmap_window(x.win);
                let _ = x.conn.flush();
                x.mapped.set(false);
            }
        });
    }

    pub fn raise_window() {
        STATE.with(|s| {
            if let Some(x) = &*s.borrow() {
                let _ = x.conn.configure_window(
                    x.win,
                    &xproto::ConfigureWindowAux::new().stack_mode(xproto::StackMode::ABOVE),
                );
                x.map();
            }
        });
    }

    pub fn is_visible() -> bool {
        STATE.with(|s| s.borrow().as_ref().is_some_and(|x| x.mapped.get()))
    }

    pub fn take_gui_closed() -> bool {
        CLOSED.replace(false)
    }

    pub fn pump_messages() {
        STATE.with(|s| {
            let Some(x) = &*s.borrow() else {
                std::thread::sleep(std::time::Duration::from_millis(1));
                return;
            };
            loop {
                match x.conn.poll_for_event() {
                    Ok(Some(event)) => match event {
                        Event::ConfigureNotify(cfg) => {
                            let (w, h) = (cfg.width as u32, cfg.height as u32);
                            if w > 0 && h > 0 && (w, h) != x.size.get() {
                                x.size.set((w, h));
                                let _ = super::with_gui(|g| {
                                    if let Some((w, h)) = g.resize_from_host(w, h) {
                                        x.size.set((w, h));
                                        let _ = x.conn.configure_window(
                                            x.win,
                                            &xproto::ConfigureWindowAux::new().width(w).height(h),
                                        );
                                    }
                                });
                            }
                        }
                        other => x.note_close_request(&other),
                    },
                    _ => break,
                }
            }
            if x.mapped.get() && x.last_embed.get().elapsed() >= Duration::from_millis(100) {
                x.last_embed.set(Instant::now());
                let (w, h) = x.size.get();
                let _ = crate::xembed::sync_clients(&x.conn, x.win, w, h);
            }
            let _ = x.conn.flush();
        });
    }
}
