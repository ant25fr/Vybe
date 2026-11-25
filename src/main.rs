/// A GUI launch must not leave a console window open on Windows. Console
/// output stays available for `--version`/`-V`/`--help` runs, where the
/// caller is a terminal.
#[cfg(windows)]
fn hide_console_window() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleWindow() -> *mut core::ffi::c_void;
        fn ShowWindow(hwnd: *mut core::ffi::c_void, n_cmd_show: i32) -> i32;
    }
    let wants_console_output = std::env::args()
        .skip(1)
        .any(|arg| matches!(arg.as_str(), "--version" | "-V" | "--help" | "-h"));
    if wants_console_output {
        return;
    }
    unsafe {
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
            const SW_HIDE: i32 = 0;
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}

#[cfg(not(windows))]
fn hide_console_window() {}

fn main() -> iced::Result {
    // Answer version queries before touching X11, audio, or the GUI, so a
    // packaged build can identify itself on a headless machine.
    if std::env::args()
        .skip(1)
        .any(|arg| arg == "--version" || arg == "-V")
    {
        println!("vibez {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    hide_console_window();

    // Call XInitThreads() before any X11/OpenGL operations.
    // JUCE-based plugins (Dexed, Surge, etc.) call XInitThreads internally
    // during gui.create(). If Xlib has already been used by wgpu/GLX (iced's
    // renderer) before XInitThreads is called, it causes undefined behavior
    // and hangs. Calling it first ensures thread-safe Xlib for all users.
    #[cfg(target_os = "linux")]
    unsafe {
        // dlopen libX11 without adding a build dependency
        if let Ok(lib) = libloading::Library::new("libX11.so.6") {
            if let Ok(init) = lib.get::<unsafe extern "C" fn() -> std::ffi::c_int>(b"XInitThreads")
            {
                init();
            }
            // Keep the library loaded for the process lifetime
            std::mem::forget(lib);
        }
    }

    vibez_ui::run()
}
