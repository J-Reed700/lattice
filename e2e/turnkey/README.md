# Fresh-machine setup test

This test checks the promise in the README: on a new machine, `./setup.sh`
installs everything, and one click in the app's first-run dialog finishes the
setup. It runs on clean Ubuntu 24.04 containers in Docker, so it covers the
Linux path only. macOS and Windows are not exercised here.

## Run it

```bash
bash e2e/turnkey/run.sh                    # both stages
bash e2e/turnkey/run.sh setup              # stage 1 only
bash e2e/turnkey/run.sh first-run <deb>    # stage 2 with a .deb from stage 1
bash e2e/turnkey/run.sh deps debian:12     # setup.sh --deps on another distribution
```

Docker must be running. The images are built for `linux/amd64`, because the
pinned `llama-server` release only has x86_64 Linux binaries. On Apple Silicon
Docker runs them under emulation, which makes the release build the slow part.

Compiling the app crate peaked at 6.7 GB under emulation, and the compiler was
killed each time in Docker Desktop's default VM (8 GB of memory, 1 GB of
swap), even with one cargo job. Before stage 1, `run.sh` checks that the VM
has at least 14 GB of memory plus swap. If it has less, `run.sh` prints a
command that adds 8 GB of swap to the running VM (gone after Docker
restarts). Raising the memory in Docker Desktop's settings also works.

Results go to `e2e-results/turnkey/<timestamp>/`: `run.log`, the
`source.tar.gz` that was tested, `setup.log`, `check.log`, the `.deb` under
`deb/`, `installed-files.txt`, `apt-install.log`, `first-run.log`, and
`first-run/` with the app's log, screenshots and `summary.json`. A `deps` run
writes `deps.log` and `check.log`.

## Stage 1: setup.sh on a bare machine

The `fresh` image in the `Dockerfile` is `ubuntu:24.04` with sudo for the
`ubuntu` user and nothing else: no compiler, git, curl, Node.js or Rust.
`run.sh` copies the working tree into it (tracked files plus untracked files
that are not ignored, so uncommitted changes are included) and runs
`./setup.sh --yes --no-launch`.

The stage passes when:

- `setup.sh` exits 0,
- the package it built is installed according to `dpkg`,
- the package installs `/usr/bin/llama-server` and the bundled Python
  (`learning-python/python.wasm`),
- `./setup.sh --check` afterwards reports nothing missing.

## Stage 2: first launch and one click

The `desktop` image is the same clean machine plus a virtual display (Xvfb),
a font, and the tools the test drives the app with: xdotool to click and
type, ImageMagick for screenshots, Tesseract to find words on them, and
sqlite3 and python3 to read the app's database. None of these are app
dependencies. `apt` installs the `.deb` from stage 1 and has to pull the
app's runtime libraries from the package's own `Depends`.

`first_run.py` then does what a person would do:

1. Starts the installed app and waits for its window.
2. Waits for the "Set up AI" dialog and clicks **Install**.
3. Reads the `models` table in `~/.local/share/tech.lattice.app/lattice.db`
   until one chat model and one embedding model are downloaded and active.
   Nothing else is clicked: the download saga activates them itself.
4. Opens Chat, starts a conversation with Ctrl+N, types a question, presses
   Enter, and waits for a completed assistant row in `conversation_messages`.
   On a host that is not x86_64 (Docker on Apple Silicon emulates the image)
   it waits only until the app log shows the llama-server sidecar ready and
   the model's first text, because CPU generation under emulation is too slow
   to finish: the first token took 18 minutes. `TURNKEY_FULL_ANSWER=1` waits
   for the whole answer anyway.

Pass and fail come from the database and the process list, not from the
screen. The screenshots are there to explain a failure.

## Settings

| Variable | Default | Effect |
| --- | --- | --- |
| `TURNKEY_CACHE` | off | `1` keeps the cargo registry, npm cache and target directory in Docker volumes between runs. Faster, but no longer a from-nothing run. Remove the `lattice-turnkey-cargo`, `-npm` and `-target` volumes to drop them. |
| `TURNKEY_FULL_LTO` | off | `1` builds with the shipping release profile. By default the run sets `codegen-units=16` and turns LTO off, because one codegen unit for the app crate takes hours under emulation and the profile is not what this test checks. |
| `TURNKEY_CPUS`, `TURNKEY_MEMORY` | `8`, `7g` | Container limits. |
| `TURNKEY_KEEP` | off | `1` leaves the containers running afterwards. |
| `TURNKEY_FULL_ANSWER` | off | `1` waits for the saved answer even under emulation (allow most of an hour). |

## Known limits

- Linux only, and only Ubuntu 24.04 end to end. The `deps` stage covers other
  distributions up to the build.
- Under emulation, Fedora 44's `tar` cannot unpack Node.js (`Cannot mkdir:
  Function not implemented`; the same archive unpacks on native arm64 Fedora),
  and pacman cannot start its download sandbox inside a container. Neither
  happens on real hardware, but it means the dnf and pacman paths have not
  passed this test end to end.
- The container has no GPU, so the app runs the CPU build of `llama-server`.
- Under emulation the app's 30-second stream stall limit fired several times,
  because generated tokens arrived more than 30 seconds apart. The limit only
  applies between generated chunks: before the first one, the app asks for
  `return_progress` and counts prompt-progress events as signs of life
  (`crates/lattice-desktop/src/features/llm/llama_cpp/retry.rs`), and
  streamed reasoning counts too. Real hardware emits many tokens a second.
