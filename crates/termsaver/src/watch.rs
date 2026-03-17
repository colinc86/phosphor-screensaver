use std::io;
use std::time::Duration;

use crate::config::Config;

/// Background idle watcher.
///
/// Monitors a heartbeat file (touched by PROMPT_COMMAND) to detect idle time.
/// When the shell has been idle for `timeout` seconds and is at the prompt,
/// injects `termsaver run` into the terminal input via TIOCSTI so bash
/// executes it as a normal foreground command. No process group manipulation.
pub fn run(shell_pid: u32, timeout: u64, _config: &Config) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    let tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")?;
    let tty_fd = tty.as_raw_fd();

    // Ignore terminal signals since we're background
    unsafe {
        libc::signal(libc::SIGTTOU, libc::SIG_IGN);
        libc::signal(libc::SIGTTIN, libc::SIG_IGN);
    }

    let heartbeat_path = format!("/tmp/.ts_heartbeat_{}", shell_pid);

    // Test that TIOCSTI works (may be disabled on Linux 6.2+)
    // We don't inject anything — just verify the ioctl is available
    // by checking the sysctl. If it fails on first real use, we exit.

    loop {
        let idle_secs = heartbeat_idle_secs(&heartbeat_path).unwrap_or(0);
        if idle_secs < timeout {
            let remaining = timeout - idle_secs;
            std::thread::sleep(Duration::from_secs(remaining.max(1)));
            continue;
        }

        // Check if shell is still alive
        if unsafe { libc::kill(shell_pid as libc::pid_t, 0) } != 0 {
            break;
        }

        // Check if shell is at the prompt (foreground pgid = shell's pgid)
        let shell_pgid = unsafe { libc::getpgid(shell_pid as libc::pid_t) };
        let fg_pgid = unsafe { libc::tcgetpgrp(tty_fd) };
        if fg_pgid != shell_pgid || shell_pgid <= 0 {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }

        // Inject "termsaver run" into terminal input
        // Leading space prevents bash from adding it to history
        if inject_input(tty_fd, b" termsaver run\n").is_err() {
            // TIOCSTI not available — exit gracefully
            #[cfg(target_os = "linux")]
            eprintln!("termsaver: TIOCSTI ioctl not available. \
                Enable with: sudo sysctl dev.tty.legacy_tiocsti=1");
            #[cfg(not(target_os = "linux"))]
            eprintln!("termsaver: TIOCSTI ioctl not available.");
            break;
        }

        // Wait for the screensaver to finish (shell becomes foreground again)
        // The screensaver runs as a foreground bash command, so the shell's
        // pgid won't be the foreground pgid while it's running.
        std::thread::sleep(Duration::from_secs(2));
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let fg = unsafe { libc::tcgetpgrp(tty_fd) };
            let spg = unsafe { libc::getpgid(shell_pid as libc::pid_t) };
            if fg == spg {
                break; // Shell is foreground again — screensaver exited
            }
            if unsafe { libc::kill(shell_pid as libc::pid_t, 0) } != 0 {
                return Ok(());
            }
        }

        // Reset heartbeat
        let _ = std::fs::File::create(&heartbeat_path);
    }

    let _ = std::fs::remove_file(&heartbeat_path);
    Ok(())
}

#[cfg(target_os = "linux")]
const TIOCSTI: libc::c_ulong = 0x5412;

#[cfg(target_os = "macos")]
const TIOCSTI: libc::c_ulong = 0x80017472;

fn inject_input(tty_fd: i32, input: &[u8]) -> io::Result<()> {
    for &byte in input {
        let ret = unsafe { libc::ioctl(tty_fd, TIOCSTI, &byte as *const u8) };
        if ret != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn heartbeat_idle_secs(path: &str) -> Option<u64> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    Some(modified.elapsed().ok()?.as_secs())
}
