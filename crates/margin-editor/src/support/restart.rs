//! Detects when `cargo build` has replaced the running binary, and restarts into it.
//!
//! Builds of one binary can differ in features (with or without the `vault`), and
//! they all write the same `target/<profile>/<binary>`. Before swapping, a running
//! build asks the new binary what it was built with, and only restarts into an
//! identical build: a vault window never reopens as a build without the vault.

use crate::prelude::*;
use futures_util::StreamExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

const POLL_INTERVAL: Duration = Duration::from_millis(1500);

/// The argument a running build passes to a replaced binary to ask what it is.
/// A flag (not an environment variable) so that an older binary rejects it and
/// exits, instead of opening a window.
const PROBE: &str = "--build-identity";

/// What this build is, as its app declared it (`answer_build_probe`).
/// Restarts only swap into a build with the same identity.
static IDENTITY: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

/// Declare what this build is (the app and the features it was built with),
/// and answer a running build's probe (see `same_build`) by printing it and
/// exiting. Every app that shows the notebook calls this first in `main`,
/// before parsing its arguments.
pub fn answer_build_probe(identity: &'static str) {
    let _ = IDENTITY.set(identity);
    if std::env::args().nth(1).as_deref() == Some(PROBE) {
        println!("{identity}");
        std::process::exit(0);
    }
}

/// Whether the binary at `path` is a build with this build's features. A binary
/// that cannot answer within a few seconds (still being written, or too old to
/// know the probe) is not.
fn same_build(path: &Path) -> bool {
    let Some(identity) = IDENTITY.get() else {
        return false;
    };
    let Ok(mut child) = Command::new(path).arg(PROBE).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else {
        return false;
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = String::new();
                if let Some(mut stdout) = child.stdout.take() {
                    let _ = std::io::Read::read_to_string(&mut stdout, &mut out);
                }
                return status.success() && out.trim() == *identity;
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// The executable path and the modification time it had when this process started.
fn launch_stamp() -> Option<(PathBuf, SystemTime)> {
    let path = std::env::current_exe().ok()?;
    let modified = std::fs::metadata(&path).ok()?.modified().ok()?;
    Some((path, modified))
}

/// The binary's modification time, when it differs from `seen`.
fn changed_since(path: &Path, seen: SystemTime) -> Option<SystemTime> {
    let now = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    (now != seen).then_some(now)
}

/// The worktree this binary was built in, taken from `<worktree>/target/<profile>/margin`.
pub fn worktree_name() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let root = exe.parent()?.parent()?.parent()?;
    Some(root.file_name()?.to_str()?.to_string())
}

/// Shows the Restart control once a newer build of the running executable appears, and
/// restarts into it automatically. The control stays available if the automatic restart fails.
/// A newer build with another identity (see `answer_build_probe`) is ignored until the binary
/// changes again.
///
/// A plain thread polls the file and reports through a channel, so the hook does not
/// depend on any particular async timer. The thread exits once the component unmounts.
pub fn use_newer_build() -> Signal<bool> {
    let mut newer = use_signal(|| false);
    let stamp = use_hook(launch_stamp);
    use_future(move || {
        let stamp = stamp.clone();
        async move {
            let Some((path, mut seen)) = stamp else { return };
            let (tx, mut rx) = futures_channel::mpsc::unbounded::<()>();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(POLL_INTERVAL);
                    if tx.is_closed() {
                        return;
                    }
                    if let Some(modified) = changed_since(&path, seen) {
                        if same_build(&path) {
                            let _ = tx.unbounded_send(());
                            return;
                        }
                        seen = modified;
                    }
                }
            });
            if rx.next().await.is_some() {
                newer.set(true);
                restart_app();
            }
        }
    });
    newer
}

/// Starts a fresh process from the same executable and arguments, then exits this one.
/// If the executable is now another build, or the new process cannot start, the
/// current window stays open.
pub fn restart_app() {
    let Ok(exe) = std::env::current_exe() else { return };
    if !same_build(&exe) {
        return;
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if std::process::Command::new(exe).args(args).spawn().is_ok() {
        std::process::exit(0);
    }
}
