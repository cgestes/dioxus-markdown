//! Detects when `cargo build` has replaced the running binary, and restarts into it.
//!
//! Builds of one binary can differ in features (with or without the `vault`), and
//! they all write the same `target/<profile>/<binary>`. Before swapping, a running
//! build asks the new binary what it was built with, and only restarts into an
//! identical build: a vault window never reopens as a build without the vault.

use margin_ui::prelude::*;
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

/// What the binary at a path says it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Probe {
    /// A build with this build's features.
    Same,
    /// Another build, or one too old to know the probe (it rejects the flag).
    Other,
    /// No answer in time: still being written, or slow to start the first
    /// time (a large debug build being checked). Ask again later.
    NoAnswer,
}

/// Ask the binary at `path` what it was built with. Blocks for up to a few
/// seconds: call it off the UI thread.
fn probe(path: &Path) -> Probe {
    let Some(identity) = IDENTITY.get() else {
        return Probe::Other;
    };
    let Ok(mut child) = Command::new(path).arg(PROBE).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else {
        return Probe::NoAnswer;
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = String::new();
                if let Some(mut stdout) = child.stdout.take() {
                    let _ = std::io::Read::read_to_string(&mut stdout, &mut out);
                }
                return if status.success() && out.trim() == *identity { Probe::Same } else { Probe::Other };
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Probe::NoAnswer;
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

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// When to ask the binary on disk what it is, from its modification time at
/// each poll.
struct Watch {
    /// The time of the binary this one is, or of the last other build.
    seen: SystemTime,
    /// The time at the previous poll.
    last: SystemTime,
}

impl Watch {
    fn new(launched: SystemTime) -> Self {
        Self { seen: launched, last: launched }
    }

    /// Whether to ask now: the binary changed, and its time held since the
    /// previous poll (a build still being written changes it again).
    fn ready(&mut self, now: SystemTime) -> bool {
        let settled = now == self.last;
        self.last = now;
        settled && now != self.seen
    }

    /// Record the answer of the binary at time `now`. True when it is a newer
    /// build of this one. Another build is not asked again until it changes;
    /// one that did not answer is asked again at the next poll.
    fn answered(&mut self, now: SystemTime, probe: Probe) -> bool {
        match probe {
            Probe::Same => true,
            Probe::Other => {
                self.seen = now;
                false
            }
            Probe::NoAnswer => false,
        }
    }
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
/// changes again. The new binary is asked only once its file has stopped changing for one
/// poll, and asked again on the next poll when it does not answer in time.
///
/// A plain thread polls the file, asks the binary, and reports through a channel, so the hook
/// does not depend on any particular async timer and the UI never waits on the probe. The
/// thread exits once the component unmounts.
pub fn use_newer_build() -> Signal<bool> {
    let mut newer = use_signal(|| false);
    let stamp = use_hook(launch_stamp);
    use_future(move || {
        let stamp = stamp.clone();
        async move {
            let Some((path, seen)) = stamp else { return };
            let (tx, mut rx) = futures_channel::mpsc::unbounded::<()>();
            std::thread::spawn(move || {
                let mut watch = Watch::new(seen);
                loop {
                    std::thread::sleep(POLL_INTERVAL);
                    if tx.is_closed() {
                        return;
                    }
                    let Some(now) = modified(&path) else { continue };
                    if watch.ready(now) && watch.answered(now, probe(&path)) {
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
/// If the executable is now another build, or the new process cannot start, the
/// current window stays open. The new binary is asked on another thread, so the
/// window keeps responding meanwhile.
pub fn restart_app() {
    let Ok(exe) = std::env::current_exe() else { return };
    std::thread::spawn(move || {
        if probe(&exe) != Probe::Same {
            return;
        }
        let args: Vec<String> = std::env::args().skip(1).collect();
        if std::process::Command::new(exe).args(args).spawn().is_ok() {
            std::process::exit(0);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_build_is_asked_once_settled_and_again_when_it_did_not_answer() {
        let at = |s: u64| SystemTime::UNIX_EPOCH + Duration::from_secs(s);
        let mut watch = Watch::new(at(1));
        assert!(!watch.ready(at(1)), "the running binary");
        assert!(!watch.ready(at(2)), "just written: wait one poll");
        assert!(!watch.ready(at(3)), "still being written");
        assert!(watch.ready(at(3)), "settled");
        assert!(!watch.answered(at(3), Probe::NoAnswer));
        assert!(watch.ready(at(3)), "asked again after no answer");
        assert!(!watch.answered(at(3), Probe::Other));
        assert!(!watch.ready(at(3)), "another build is not asked again");
        assert!(!watch.ready(at(4)));
        assert!(watch.ready(at(4)));
        assert!(watch.answered(at(4), Probe::Same));
    }
}
