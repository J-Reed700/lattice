#!/usr/bin/env python3
"""First launch of an installed Lattice, driven the way a person would do it.

Runs inside the `desktop` image from e2e/turnkey/Dockerfile, after apt has
installed the .deb. It starts Xvfb, opens the app, finds the "Set up AI"
dialog on screen, clicks Install, and then reads the app's own database until
the recommended chat and embedding models are downloaded and active. Last, it
opens Chat, asks one question and waits for a completed answer.

The screen is read with Tesseract and clicked with xdotool, so the test needs
no test-only build: it drives the same package a user installs. Every check
that decides pass or fail reads state the app wrote (SQLite rows, files,
processes); the screenshots in --out are there to explain a failure.

Usage: dbus-run-session -- python3 first_run.py --out /tmp/first-run
"""

from __future__ import annotations

import argparse
import json
import os
import sqlite3
import subprocess
import sys
import time
from pathlib import Path

DISPLAY = ":99"
SCREEN = (1440, 960)
APP_ID = "tech.lattice.app"
DATA_DIR = Path.home() / ".local/share" / APP_ID
DB_PATH = DATA_DIR / "lattice.db"
# FilesystemModelStorage keeps downloads here, outside the app data directory.
MODELS_DIR = Path.home() / ".cache/lattice/models"
QUESTION = "Reply with one word: what colour is a clear daytime sky?"

parser = argparse.ArgumentParser()
parser.add_argument("--out", type=Path, required=True)
parser.add_argument("--app", default=None, help="app executable (default: from dpkg)")
parser.add_argument("--download-timeout", type=int, default=90 * 60)
parser.add_argument("--answer-timeout", type=int, default=30 * 60)
args = parser.parse_args()
OUT: Path = args.out
OUT.mkdir(parents=True, exist_ok=True)
STARTED = time.monotonic()
SUMMARY: dict = {"steps": []}
shot_index = 0


def log(message: str) -> None:
    print(f"[{time.monotonic() - STARTED:7.1f}s] {message}", flush=True)


def record(step: str, ok: bool, **detail) -> None:
    SUMMARY["steps"].append({"step": step, "ok": ok, "seconds": round(time.monotonic() - STARTED), **detail})
    (OUT / "summary.json").write_text(json.dumps(SUMMARY, indent=2))


def fail(step: str, message: str, **detail) -> None:
    log(f"FAIL {step}: {message}")
    screenshot(f"fail-{step}")
    record(step, False, error=message, **detail)
    SUMMARY["passed"] = False
    (OUT / "summary.json").write_text(json.dumps(SUMMARY, indent=2))
    stop_app()
    sys.exit(1)


app: subprocess.Popen | None = None


def stop_app() -> None:
    """Close the app the way the window manager would, so it stops its sidecars."""
    if app is None or app.poll() is not None:
        return
    app.terminate()
    try:
        app.wait(timeout=30)
    except subprocess.TimeoutExpired:
        app.kill()


def run(*cmd: str, check: bool = True, **kw) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, check=check, text=True, capture_output=True, **kw)


def screenshot(name: str) -> Path:
    global shot_index
    shot_index += 1
    path = OUT / f"{shot_index:02d}-{name}.png"
    run("import", "-display", DISPLAY, "-window", "root", str(path), check=False)
    return path


def read_screen() -> list[dict]:
    """Words on screen with their centres, from Tesseract's TSV output.

    Each screenshot is read twice, as is and inverted: Tesseract reads dark
    text on a light background, so light labels on dark buttons ("Install",
    "New conversation") only show up in the inverted copy.
    """
    raw = OUT / "ocr-input.png"
    run("import", "-display", DISPLAY, "-window", "root", str(raw))
    words = []
    for variant, extra in (("plain", []), ("inverted", ["-negate"])):
        scaled = OUT / f"ocr-{variant}.png"
        # Twice the size in grey reads small UI text far more reliably.
        run("convert", str(raw), "-colorspace", "Gray", *extra, "-resize", "200%", str(scaled))
        tsv = run("tesseract", str(scaled), "-", "--psm", "11", "tsv", check=False).stdout
        for line in tsv.splitlines()[1:]:
            cols = line.split("\t")
            if len(cols) < 12 or not cols[11].strip():
                continue
            left, top, width, height = (int(c) for c in cols[6:10])
            words.append({
                "text": cols[11].strip(),
                "x": (left + width / 2) / 2,
                "y": (top + height / 2) / 2,
            })
    return words


def find_word(*candidates: str, timeout: float, exact: bool = False) -> dict | None:
    wanted = [c.lower() for c in candidates]
    deadline = time.monotonic() + timeout
    while True:
        for word in read_screen():
            text = word["text"].lower().strip(".,:;!?·…")
            if (text in wanted) if exact else any(text.startswith(w) for w in wanted):
                return word
        if time.monotonic() > deadline:
            return None
        time.sleep(3)


def find_phrase(*phrases: tuple[str, str], timeout: float) -> dict | None:
    """The second word of the first two-word phrase found on one line."""
    deadline = time.monotonic() + timeout
    while True:
        words = read_screen()
        for first, second in phrases:
            for a in words:
                if a["text"].lower().strip(".,:;…") != first:
                    continue
                for b in words:
                    if (b["text"].lower().strip(".,:;…") == second
                            and abs(a["y"] - b["y"]) < 8 and 0 < b["x"] - a["x"] < 200):
                        return b
        if time.monotonic() > deadline:
            return None
        time.sleep(3)


def screen_text() -> str:
    return " ".join(w["text"] for w in read_screen())


def click(word: dict) -> None:
    x, y = int(word["x"]), int(word["y"])
    run("xdotool", "mousemove", "--sync", str(x), str(y), "click", "1")
    log(f"clicked '{word['text']}' at {x},{y}")


def query(sql: str) -> list[tuple]:
    if not DB_PATH.exists():
        return []
    try:
        with sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True, timeout=5) as conn:
            return conn.execute(sql).fetchall()
    except sqlite3.Error as error:
        log(f"database read failed ({error}); retrying")
        return []


def app_executable() -> str:
    if args.app:
        return args.app
    package = run("dpkg-deb", "-f", "/tmp/lattice.deb", "Package").stdout.strip()
    for path in run("dpkg", "-L", package).stdout.splitlines():
        if path.startswith("/usr/bin/") and "llama-server" not in path and path.count("/") == 3:
            return path
    fail("locate-app", f"package {package} installs no executable in /usr/bin")
    return ""


def du(path: Path) -> int:
    return sum(f.stat().st_size for f in path.rglob("*") if f.is_file()) if path.exists() else 0


# ---------------------------------------------------------------------------
os.environ["DISPLAY"] = DISPLAY
xvfb = subprocess.Popen(
    ["Xvfb", DISPLAY, "-screen", "0", f"{SCREEN[0]}x{SCREEN[1]}x24", "-nolisten", "tcp"],
    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
)
time.sleep(2)

exe = app_executable()
log(f"starting {exe}")
app_log = open(OUT / "app.log", "w")
app = subprocess.Popen([exe], stdout=app_log, stderr=subprocess.STDOUT, env={**os.environ, "RUST_LOG": "info"})

# 1. The window appears.
deadline = time.monotonic() + 180
window = ""
while time.monotonic() < deadline and not window:
    if app.poll() is not None:
        fail("window", f"the app exited with code {app.returncode} before showing a window")
    window = run("xdotool", "search", "--onlyvisible", "--name", "Lattice", check=False).stdout.strip()
    time.sleep(2)
if not window:
    fail("window", "no Lattice window within 180 s")
window = window.splitlines()[0]
run("xdotool", "windowmove", window, "0", "0", check=False)
run("xdotool", "windowsize", window, str(SCREEN[0]), str(SCREEN[1]), check=False)
record("window", True)
log("window is up")

# 2. The first-run dialog offers the bundle.
# OCR reads the "I" in "AI" as "l" as often as not, so match "set up a".
# The sidebar's "Settings" also starts with "Set", so wait for the phrase.
deadline = time.monotonic() + 240
while "set up a" not in screen_text().lower():
    if app.poll() is not None:
        fail("dialog", f"the app exited with code {app.returncode} before the dialog appeared")
    if time.monotonic() > deadline:
        fail("dialog", "the 'Set up AI' dialog did not appear", screen=screen_text()[:500])
    time.sleep(3)
screenshot("setup-dialog")
# Install stays disabled while the recommendation loads ("Checking your hardware…").
deadline = time.monotonic() + 120
while "checking your hardware" in screen_text().lower() and time.monotonic() < deadline:
    time.sleep(2)
offer = screen_text()
record("dialog", True, offer=offer[:400])

# 3. One click.
install = find_word("Install", timeout=30, exact=True)
if not install:
    fail("install-button", "no Install button on screen", screen=offer[:500])
click(install)
time.sleep(5)
screenshot("after-install-click")
record("install-click", True)

# 4. Both models download and switch on without another click.
deadline = time.monotonic() + args.download_timeout
last_report = 0.0
rows: list[tuple] = []
while True:
    rows = query(
        "SELECT model_id, model_type, status, is_active_for_chat, is_active_for_embedding FROM models"
    )
    chat_ready = any(r[1] == "chat" and r[3] == 1 and r[2] in ("completed", "verified") for r in rows)
    embed_ready = any(r[1] == "embedding" and r[4] == 1 and r[2] in ("completed", "verified") for r in rows)
    if chat_ready and embed_ready:
        break
    if any(r[2] == "failed" for r in rows):
        fail("downloads", "a model download failed", models=rows)
    if app.poll() is not None:
        fail("downloads", f"the app exited with code {app.returncode} during the downloads", models=rows)
    if time.monotonic() > deadline:
        fail("downloads", "the models were not active in time", models=rows)
    if time.monotonic() - last_report > 60:
        last_report = time.monotonic()
        log(f"downloads: {du(MODELS_DIR) / 1e9:.2f} GB on disk; models={rows}")
    time.sleep(10)
screenshot("models-active")
record("downloads", True, models=rows, bytes_on_disk=du(MODELS_DIR))
log(f"models active: {rows}")

# 5. Ask one question in Chat.
chat_nav = find_word("Chat", timeout=30, exact=True)
if not chat_nav:
    fail("chat-open", "no Chat entry in the navigation", screen=screen_text()[:500])
click(chat_nav)
time.sleep(4)
screenshot("chat")
# An empty Chat page has no message box until a conversation is started.
# Mod+N (Ctrl+N here) starts one on the Chat page (src/components/RootLayout.tsx),
# once the page is listening; on a loaded machine the first press can come too
# early. The "New conversation" button under "No conversation open." is the
# other way in; its light-on-dark label is the one thing OCR cannot read, so
# it is found by the heading above it.
# The composer's placeholder: "Ask anything", "Search sources and answer" or
# "Follow up" (src/features/chat/hooks/useComposer.ts). Matching the pair keeps
# the sidebar's "Find anything" out.
PLACEHOLDERS = (("ask", "anything"), ("search", "sources"), ("follow", "up"))
composer = None
for attempt in range(5):
    run("xdotool", "key", "ctrl+n")
    composer = find_phrase(*PLACEHOLDERS, timeout=10)
    if composer:
        break
    heading = find_phrase(("conversation", "open"), timeout=5)
    if heading:
        click({"text": "New conversation", "x": heading["x"], "y": heading["y"] + 97})
        composer = find_phrase(*PLACEHOLDERS, timeout=10)
        if composer:
            break
    log(f"no message box yet (attempt {attempt + 1})")
screenshot("new-conversation")
if not composer:
    fail("chat-composer", "no message box after Ctrl+N or New conversation", screen=screen_text()[:500])
click(composer)
before = len(query("SELECT id FROM conversation_messages WHERE role = 'assistant'"))
run("xdotool", "type", "--delay", "40", QUESTION)
run("xdotool", "key", "Return")
time.sleep(3)
screenshot("question-sent")
# The turn is saved when it finishes, so until then only the screen shows it
# was sent ("Sending", "Generating…"). That grey text is hard to OCR on a busy
# machine, so it is noted, not required: the saved answer below decides.
record("question", True, seen_sending=bool(find_word("Sending", "Generating", "Answering", timeout=60)))

deadline = time.monotonic() + args.answer_timeout
answer: tuple | None = None
while True:
    rows = query(
        "SELECT status, content FROM conversation_messages WHERE role = 'assistant' ORDER BY rowid"
    )
    if len(rows) > before and rows[-1][0] == "completed" and rows[-1][1].strip():
        answer = rows[-1]
        break
    if len(rows) > before and rows[-1][0] in ("failed", "error"):
        fail("answer", "the answer failed", message=rows[-1])
    if app.poll() is not None:
        fail("answer", f"the app exited with code {app.returncode} while answering")
    if time.monotonic() > deadline:
        fail("answer", "no completed answer in time", messages=rows[-2:])
    time.sleep(10)
screenshot("answer")
sidecars = run("pgrep", "-a", "llama-server", check=False).stdout.strip().splitlines()
record("answer", True, answer=answer[1][:500], sidecars=sidecars)
log(f"answer: {answer[1][:200]!r}")

SUMMARY["passed"] = True
(OUT / "summary.json").write_text(json.dumps(SUMMARY, indent=2))
stop_app()
xvfb.terminate()
log("PASS")
