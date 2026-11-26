//! Process hardening for plugin hosts.
//!
//! The bridge already isolates a plugin's crashes in its own process. This
//! module narrows what that process may do and see:
//!
//! - [`sanitized_env`] strips the parent's environment down to what plugins
//!   and their GUIs need, so tokens and credentials in the DAW's environment
//!   never reach third-party code.
//! - [`harden_current_process`] (run first thing inside the helper, Linux)
//!   forbids privilege gain, disables core dumps and ptrace attachment, and
//!   installs a seccomp denylist that refuses system calls a plugin has no
//!   business making (ptrace, mount, module loading, bpf, reboot, ...).
//!   Everything is best-effort: a failure to harden is logged and the plugin
//!   still loads, because an unloadable plugin is worse than a softer sandbox.

use std::ffi::OsString;

/// Variables that survive into a plugin process, by exact name.
const ENV_ALLOW: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "LANG",
    "LANGUAGE",
    "TMPDIR",
    "TZ",
    "SHELL",
    "DISPLAY",
    "XAUTHORITY",
    "WAYLAND_DISPLAY",
    "DBUS_SESSION_BUS_ADDRESS",
    "LD_LIBRARY_PATH",
    "VST3_PATH",
    "CLAP_PATH",
    "RUST_LOG",
    "RUST_BACKTRACE",
    // Windows/macOS basics, harmless elsewhere.
    "SYSTEMROOT",
    "WINDIR",
    "TEMP",
    "TMP",
    "APPDATA",
    "LOCALAPPDATA",
    "USERPROFILE",
    "COMSPEC",
];

/// Variable prefixes that survive (locale, XDG dirs, audio/GUI toolkits).
const ENV_ALLOW_PREFIXES: &[&str] = &["LC_", "XDG_", "PULSE_", "PIPEWIRE_", "GDK_", "QT_", "GTK_"];

fn allowed(name: &str) -> bool {
    ENV_ALLOW.contains(&name) || ENV_ALLOW_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// The current environment reduced to the allowlist.
pub fn sanitized_env() -> Vec<(OsString, OsString)> {
    std::env::vars_os()
        .filter(|(name, _)| name.to_str().is_some_and(allowed))
        .collect()
}

/// Applies the allowlisted environment to a command, dropping the rest.
pub fn apply_env(command: &mut std::process::Command) {
    command.env_clear();
    command.envs(sanitized_env());
}

/// Hardens the calling process. Call before any plugin code or thread exists
/// so every thread inherits the restrictions.
pub fn harden_current_process() {
    #[cfg(target_os = "linux")]
    linux::harden();
}

#[cfg(target_os = "linux")]
mod linux {
    // Classic BPF opcodes.
    const BPF_LD_W_ABS: u16 = 0x20;
    const BPF_JEQ_K: u16 = 0x15;
    const BPF_RET_K: u16 = 0x06;
    const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
    const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
    const PR_SET_SECCOMP: libc::c_int = 22;
    const SECCOMP_MODE_FILTER: libc::c_ulong = 2;
    /// `struct seccomp_data` offsets.
    const OFFSET_NR: u32 = 0;
    const OFFSET_ARCH: u32 = 4;

    #[cfg(target_arch = "x86_64")]
    const AUDIT_ARCH: Option<u32> = Some(0xC000_003E);
    #[cfg(target_arch = "aarch64")]
    const AUDIT_ARCH: Option<u32> = Some(0xC000_00B7);
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    const AUDIT_ARCH: Option<u32> = None;

    /// System calls refused with EPERM: debugging other processes, changing
    /// the system's mounts/modules/boot state, and kernel attack surface that
    /// audio plugins never need.
    fn denied() -> Vec<u32> {
        vec![
            libc::SYS_ptrace,
            libc::SYS_process_vm_readv,
            libc::SYS_process_vm_writev,
            libc::SYS_mount,
            libc::SYS_umount2,
            libc::SYS_pivot_root,
            libc::SYS_chroot,
            libc::SYS_reboot,
            libc::SYS_kexec_load,
            libc::SYS_init_module,
            libc::SYS_finit_module,
            libc::SYS_delete_module,
            libc::SYS_bpf,
            libc::SYS_swapon,
            libc::SYS_swapoff,
            libc::SYS_acct,
            libc::SYS_keyctl,
            libc::SYS_add_key,
            libc::SYS_request_key,
            libc::SYS_perf_event_open,
            libc::SYS_settimeofday,
            libc::SYS_clock_settime,
        ]
        .into_iter()
        .map(|n| n as u32)
        .collect()
    }

    fn stmt(code: u16, k: u32) -> libc::sock_filter {
        libc::sock_filter {
            code,
            jt: 0,
            jf: 0,
            k,
        }
    }

    fn jump(code: u16, k: u32, jt: u8, jf: u8) -> libc::sock_filter {
        libc::sock_filter { code, jt, jf, k }
    }

    /// The filter program: unknown architectures are allowed through (the
    /// syscall numbers would not mean what we think), listed calls fail with
    /// EPERM, everything else is allowed.
    pub(super) fn program(arch: u32, denied: &[u32]) -> Vec<libc::sock_filter> {
        let n = denied.len();
        let mut prog = Vec::with_capacity(n + 5);
        prog.push(stmt(BPF_LD_W_ABS, OFFSET_ARCH));
        prog.push(jump(BPF_JEQ_K, arch, 0, (n + 1) as u8));
        prog.push(stmt(BPF_LD_W_ABS, OFFSET_NR));
        for (i, nr) in denied.iter().enumerate() {
            prog.push(jump(BPF_JEQ_K, *nr, (n - i) as u8, 0));
        }
        prog.push(stmt(BPF_RET_K, SECCOMP_RET_ALLOW));
        prog.push(stmt(BPF_RET_K, SECCOMP_RET_ERRNO | libc::EPERM as u32));
        prog
    }

    pub(super) fn harden() {
        // SAFETY: plain prctl/setrlimit calls on this process with valid
        // arguments; the seccomp program outlives the call.
        unsafe {
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                log::warn!("sandbox: PR_SET_NO_NEW_PRIVS failed");
                return;
            }
            // Other processes (including the same user's) cannot ptrace or
            // read the memory of a non-dumpable process.
            libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
            let no_core = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            libc::setrlimit(libc::RLIMIT_CORE, &no_core);

            let Some(arch) = AUDIT_ARCH else { return };
            let mut filter = program(arch, &denied());
            let fprog = libc::sock_fprog {
                len: filter.len() as u16,
                filter: filter.as_mut_ptr(),
            };
            if libc::prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &fprog as *const _) != 0 {
                log::warn!("sandbox: seccomp filter rejected");
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn filter_jumps_land_on_the_right_returns() {
            let denied = [10_u32, 20, 30];
            let prog = program(0xAA, &denied);
            let n = denied.len();
            assert_eq!(prog.len(), n + 5);
            let allow = n + 3;
            let deny = n + 4;
            assert_eq!(prog[allow].k, SECCOMP_RET_ALLOW);
            assert_eq!(prog[deny].k & 0xffff_0000, SECCOMP_RET_ERRNO);
            // A foreign architecture jumps straight to allow.
            assert_eq!(1 + 1 + prog[1].jf as usize, allow);
            // Every listed syscall jumps to deny.
            for i in 0..n {
                assert_eq!(3 + i + 1 + prog[3 + i].jt as usize, deny);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_allowlist_keeps_basics_and_drops_secrets() {
        assert!(allowed("PATH"));
        assert!(allowed("XDG_RUNTIME_DIR"));
        assert!(allowed("LC_ALL"));
        assert!(!allowed("AWS_SECRET_ACCESS_KEY"));
        assert!(!allowed("GITHUB_TOKEN"));
        assert!(!allowed("SSH_AUTH_SOCK"));
    }
}
