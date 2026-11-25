//! Out-of-process plugin sandbox helper. See
//! `vibez_plugin_host::bridge` for the design.

use std::process::ExitCode;

fn main() -> ExitCode {
    vibez_plugin_host::bridge::helper::run()
}
