"""Probe the platform's actual AppKit → winit → Dioxus input path."""
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import tempfile
import time
import uuid

def observe(keys, pid, reports, previous, evidence, gesture=None):
    """Wait for a new frame containing this key; never resend the edit."""
    known = set(reports.glob("*.json"))
    end = time.monotonic() + 15
    next_request = 0
    recent = None
    while time.monotonic() < end:
        if time.monotonic() >= next_request:
            subprocess.run([keys, "report", pid], check=True)
            next_request = time.monotonic() + 0.4
        for path in sorted(set(reports.glob("*.json")) - known, reverse=True):
            try:
                result = json.loads(path.read_text())
            except (OSError, json.JSONDecodeError):
                continue
            known.add(path)
            recent = result
            evidence.write_text(json.dumps(result, ensure_ascii=False, indent=2))
            if gesture is None:
                if result["current_state"]["focused"] == "note-body":
                    return result
            elif any(f["number"] > previous["frames_observed"] and any(f"Named({gesture})" in key for key in f["inputs"])
                     for f in result["recent_frames"]):
                return result
        time.sleep(0.04)
    detail = "No F9 report arrived." if recent is None else f"Last observed focus: {recent['current_state']['focused']}. See {evidence}."
    raise RuntimeError(f"No native input checkpoint for {gesture or 'startup'}. {detail}")

def expectation(gesture, previous):
    source = previous["current_markdown"]
    # This fixture contains ASCII only, so byte and Python string offsets agree.
    state = previous["current_state"]
    if state["focused"] != "note-body" or state["markdown_bytes"] is None:
        raise RuntimeError(f"Lost the native note body before {gesture}. Observed focus: {state['focused']}.")
    a, b = state["markdown_bytes"]
    if a != b:
        raise RuntimeError(f"Expected a single caret for {gesture}, received {a} → {b}")
    if gesture == "ArrowRight":
        at = b + 1
    elif gesture == "Enter":
        return source[:b] + "\n" + source[b:], b + 1
    elif gesture == "Backspace":
        return source[:b - 1] + source[b:], b - 1
    else:
        start = source.rfind("\n", 0, b) + 1
        following = source.find("\n", b)
        if following < 0:
            raise RuntimeError("The previous action left no next line to test")
        next_start = following + 1
        end = source.find("\n", next_start)
        at = min(next_start + b - start, len(source) if end < 0 else end)
    return source, at

# The first four keys exercise ordinary editing. The remaining physical keys
# select a 2x2 rectangle backwards and navigate it in both directions.
WINDOW_KEYS = [
    ("ArrowRight", "Right", "ArrowRight"), ("Enter", "Return", "Enter"),
    ("Backspace", "Backspace", "Backspace"), ("ArrowDown", "Down", "ArrowDown"),
    ("end-of-table", "Cmd+Down", "ArrowDown"),
    ("extend-left", "Alt+Shift+Left", "ArrowLeft"),
    ("extend-up", "Shift+Up", "ArrowUp"),
    ("previous-cell", "Shift+Tab", "Tab"), ("next-cell", "Tab", "Tab"),
    ("clear-cells", "Backspace", "Backspace"),
]

def window_expectation(name, previous):
    source = previous["current_markdown"]
    if name in {"ArrowRight", "ArrowDown", "Enter", "Backspace"}:
        text, at = expectation(name, previous)
        return text, [at, at], "note-body", None
    anchor = [2, 1]
    if name == "clear-cells":
        text = source.replace("| one | two |\n| three | four |", "|  |  |\n|  |  |")
        # GFM's canonical empty padded cell has its caret between the spaces.
        at = text.index("|  |  |") + 2
        return text, [at, at], "cell-0-1-0", [0, anchor, [1, 0]]
    word, field, corner = {
        "end-of-table": ("four", "cell-0-2-1", None),
        "extend-left": ("three", "cell-0-2-0", [2, 0]),
        "extend-up": ("one", "cell-0-1-0", [1, 0]),
        "previous-cell": ("four", "cell-0-2-1", [1, 0]),
        "next-cell": ("one", "cell-0-1-0", [1, 0]),
    }[name]
    start = source.index(word)
    end = start + len(word)
    return source, [end if corner is None else start, end], field, [0, anchor, corner] if corner else None

def run_window(root, run, env):
    orca = env.get("ORCA_CLI_COMMAND") or shutil.which("orca-dev" if env.get("ORCA_DEV_REPO_ROOT") else "orca")
    if not orca:
        installed = Path("/Applications/Orca.app/Contents/Resources/bin/orca")
        if not installed.is_file():
            raise RuntimeError("The background window driver needs Orca. The portable suite is available with --headless.")
        orca = str(installed)
    folder = run / "macos-window"
    folder.mkdir()
    evidence = folder / "observations"
    evidence.mkdir()
    pid = None
    with tempfile.TemporaryDirectory(prefix="margin-window-") as tmp:
        tmp = Path(tmp)
        reports = tmp / "reports"
        reports.mkdir()
        bundle = tmp / "Margin Input Check.app"
        contents = bundle / "Contents"
        (contents / "MacOS").mkdir(parents=True)
        identifier = "dev.margin.notes.test-" + uuid.uuid4().hex
        info = dict(CFBundleIdentifier=identifier, CFBundleName="Margin Input Check",
                    CFBundleExecutable="margin", CFBundlePackageType="APPL",
                    CFBundleVersion="1", NSHighResolutionCapable=True)
        (contents / "Info.plist").write_bytes(plistlib.dumps(info))
        subprocess.run(["cargo", "build", "--locked", "-p", "margin-editor", "--bin", "margin"], cwd=root, env=env, check=True)
        target = Path(env.get("CARGO_TARGET_DIR", root / "target"))
        exe = contents / "MacOS" / "margin"
        try:
            os.link(target / "debug/margin", exe)
        except OSError:
            shutil.copy2(target / "debug/margin", exe)
        keys = tmp / "keys"
        subprocess.run(["swiftc", root / "scripts/macos-test-keys.swift", "-o", keys], check=True)
        pid = subprocess.check_output([keys, "launch", bundle, reports], text=True, timeout=25).strip()
        failures = 0
        actions = []
        try:
            deadline = time.monotonic() + 15
            while subprocess.run([keys, "window", pid], capture_output=True).returncode:
                if time.monotonic() > deadline:
                    raise RuntimeError("The test process did not open a native window")
                time.sleep(0.1)
            observed = observe(keys, pid, reports, None, evidence / "startup.json")
            window_id = subprocess.check_output([keys, "window", pid], text=True).strip()
            for n, (gesture, os_name, key_name) in enumerate(WINDOW_KEYS, 1):
                prev = observed
                wanted, selection, expected_field, rectangle = window_expectation(gesture, prev)
                command = "hotkey" if "+" in os_name else "press-key"
                proc = subprocess.run([orca, "computer", command, "--app", f"pid:{pid}",
                    "--window-id", window_id, "--key", os_name, "--no-screenshot", "--json"],
                    capture_output=True, text=True, timeout=20)
                reply = json.loads(proc.stdout)
                if not reply.get("ok"):
                    raise RuntimeError(reply.get("error", {}).get("message", "Background keyboard input failed"))
                observed = observe(keys, pid, reports, prev, evidence / f"{n}-{gesture}.json", key_name)
                actions.append(gesture)
                state, actual = observed["current_state"], observed["current_markdown"]
                ok = (actual == wanted and state["markdown_bytes"] == selection and state["cell_selection"] == rectangle
                      and state["focused"] == expected_field and state["field_matches_source"] is True)
                error = None if ok else (
                    f"Expected {expected_field} at {selection} with cells {rectangle}, got {state['focused']} at {state['markdown_bytes']} with cells {state['cell_selection']}.\n"
                    f"Expected Markdown: {wanted!r}\nObserved Markdown: {actual!r}")
                failures += int(not ok)
                print(f"macOS window / {gesture}: {'PASS' if ok else 'FAIL'}", flush=True)
                record = {"case": "macos-window", "checkpoint": gesture, "state": state,
                          "actions": actions, "markdown": actual, "error": error,
                          "before": prev["current_state"], "before_markdown": prev["current_markdown"],
                          "expected_markdown": wanted, "expected_bytes": selection, "expected_cells": rectangle,
                          "recent_frames": observed["recent_frames"]}
                base = folder / f"{n:03}-{gesture}"
                base.with_suffix(".json").write_text(json.dumps(record, indent=2, ensure_ascii=False))
                window = subprocess.run([keys, "window", pid], capture_output=True, text=True)
                if not window.returncode and shutil.which("screencapture"):
                    subprocess.run(["screencapture", "-x", "-o", "-l", window.stdout.strip(), base.with_suffix(".png")],
                                   capture_output=True, timeout=8)
        finally:
            # Only this runner's uniquely named, disposable process can be closed.
            proc = subprocess.run(["ps", "-p", pid, "-o", "comm="], capture_output=True, text=True)
            if proc.returncode == 0 and Path(proc.stdout.strip()).resolve() == exe.resolve():
                subprocess.run(["kill", "-TERM", pid], capture_output=True)
        return failures == 0
