//! Detects when `cargo build` has replaced the running binary, and restarts into it.

use dioxus_native::prelude::*;
use futures_util::StreamExt;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const POLL_INTERVAL: Duration = Duration::from_millis(1500);

/// The executable path and the modification time it had when this process started.
fn launch_stamp() -> Option<(PathBuf, SystemTime)> {
    let path = std::env::current_exe().ok()?;
    let modified = std::fs::metadata(&path).ok()?.modified().ok()?;
    Some((path, modified))
}

fn binary_changed(path: &PathBuf, launched: SystemTime) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .is_ok_and(|now| now != launched)
}

/// The worktree this binary was built in, taken from `<worktree>/target/<profile>/margin`.
pub fn worktree_name() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let root = exe.parent()?.parent()?.parent()?;
    Some(root.file_name()?.to_str()?.to_string())
}

/// Shows the Restart control once a newer build of the running executable appears, and
/// restarts into it automatically. The control stays available if the automatic restart fails.
///
/// A plain thread polls the file and reports through a channel, so the hook does not
/// depend on any particular async timer. The thread exits once the component unmounts.
pub fn use_newer_build() -> Signal<bool> {
    let mut newer = use_signal(|| false);
    let stamp = use_hook(launch_stamp);
    use_future(move || {
        let stamp = stamp.clone();
        async move {
            let Some((path, launched)) = stamp else { return };
            let (tx, mut rx) = futures_channel::mpsc::unbounded::<()>();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(POLL_INTERVAL);
                    if tx.is_closed() {
                        return;
                    }
                    if binary_changed(&path, launched) {
                        let _ = tx.unbounded_send(());
                        return;
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
/// If the new process cannot start, the current window stays open.
pub fn restart_app() {
    let Ok(exe) = std::env::current_exe() else { return };
    let args: Vec<String> = std::env::args().skip(1).collect();
    if std::process::Command::new(exe).args(args).spawn().is_ok() {
        std::process::exit(0);
    }
}
