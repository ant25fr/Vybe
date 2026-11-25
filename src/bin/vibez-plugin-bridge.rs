//! See `vibez_plugin_host::bridge::helper` for what this binary is for.
//! It lives in the root package so a plain `cargo build` puts the
//! helper next to the `vibez` executable.

use std::process::ExitCode;

fn main() -> ExitCode {
    vibez_plugin_host::bridge::helper::run()
}
