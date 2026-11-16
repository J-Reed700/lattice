#!/usr/bin/env bash
# Whole-module live course run, as the app runs it: every request JSON in <requests-dir>
# is one course of ONE process of the snapshotted test binary (one container, one
# database, one set of pacers and schedulers), all courses started side by side.
# Model and web calls are recorded (or replayed).
#
# Usage: scripts/live-course-runs.sh <binary> <label> <requests-dir> [artifacts-root]
#
# The run is <artifacts-root>/<label>, with one directory per course inside it.
#
# Environment:
#   LATTICE_COURSE_SETTINGS_FILE      settings.json copied once per run (default: the courseprep
#                                     test app's file under %APPDATA%)
#   LATTICE_LEARNING_PYTHON           the Python bundle (default: the repository's)
#   LATTICE_COURSE_MANUAL_REPAIRS     Repair presses allowed per course after generation (default 0)
#   LATTICE_COURSE_RETRY_FAILED       set to 1 with LATTICE_COURSE_CONTINUE: press "Retry lesson
#                                     preparation" once on each lesson whose preparation failed
#   RUST_MIN_STACK                    the test thread's stack (default 32 MiB; a debug build's
#                                     futures overflow the 2 MiB default)
#   LATTICE_COURSE_REPLAY_FROM_ROOT   an earlier artifacts root; the run replays the calls of
#                                     <root>/<earlier label>
#   LATTICE_COURSE_REPLAY_FROM_LABEL  the earlier run's label (default: <label>)
#   LATTICE_COURSE_REPLAY_STRICT      set: a call the earlier run has no answer for fails
#                                     instead of going live (a free determinism check)
#   LATTICE_COURSE_CONTINUE           set: continue the run <artifacts-root>/<label> that a stop
#                                     request left unfinished, in the same directory and database
#
# A run ends when its jobs end. To end it in order, create the file <artifacts-root>/<label>/stop;
# unfinished jobs are suspended, and the run continues later with LATTICE_COURSE_CONTINUE=1.
set -u

if (( $# < 3 || $# > 4 )); then
  echo "Usage: $0 <binary> <label> <requests-dir> [artifacts-root]" >&2
  exit 2
fi

REPO=$(cd "$(dirname "$0")/.." && pwd)
BIN=$(realpath "$1")
LABEL=$2
REQUESTS=$(realpath "$3")
ROOT=${4:-$REPO/artifacts/course-runs-$(date +%F)}
mkdir -p "$ROOT" && ROOT=$(realpath "$ROOT")
RUN=$ROOT/$LABEL
REPLAY_ROOT=${LATTICE_COURSE_REPLAY_FROM_ROOT:-}
REPLAY_LABEL=${LATTICE_COURSE_REPLAY_FROM_LABEL:-$LABEL}

if [[ ! -x $BIN ]]; then echo "Not an executable test binary: $BIN" >&2; exit 2; fi
if [[ ! -d $REQUESTS ]]; then echo "No requests directory: $REQUESTS" >&2; exit 2; fi
CONTINUE=${LATTICE_COURSE_CONTINUE:-}
SETTINGS_SRC=${LATTICE_COURSE_SETTINGS_FILE:-$APPDATA/tech.lattice.compatibility.courseprep/settings.json}
if [[ -z $CONTINUE && ! -f $SETTINGS_SRC ]]; then echo "No settings file: $SETTINGS_SRC" >&2; exit 2; fi
PYTHON_BUNDLE=${LATTICE_LEARNING_PYTHON:-$REPO/crates/lattice-desktop/resources/learning-python}
if [[ ! -d $PYTHON_BUNDLE ]]; then echo "No Python bundle: $PYTHON_BUNDLE" >&2; exit 2; fi
shopt -s nullglob
requests=("$REQUESTS"/*.json)
if (( ${#requests[@]} == 0 )); then echo "No request JSON files in $REQUESTS" >&2; exit 2; fi
if [[ -n $CONTINUE ]]; then
  if [[ ! -f $RUN/app-data/lattice.db ]]; then echo "No run to continue: $RUN" >&2; exit 2; fi
  if [[ -n $REPLAY_ROOT ]]; then echo "A continued run does not replay." >&2; exit 2; fi
  if [[ ! -f $ROOT/settings/settings.json ]]; then
    echo "No settings of the run to continue: $ROOT/settings/settings.json" >&2; exit 2
  fi
elif [[ -e $RUN ]]; then
  echo "Run directory already exists: $RUN. Choose a new label." >&2; exit 2
fi
replay=()
if [[ -n $REPLAY_ROOT ]]; then
  if [[ ! -d $REPLAY_ROOT/$REPLAY_LABEL/calls ]]; then
    echo "No calls to replay in $REPLAY_ROOT/$REPLAY_LABEL" >&2; exit 2
  fi
  replay=(LATTICE_COURSE_REPLAY_FROM="$(realpath "$REPLAY_ROOT")/$REPLAY_LABEL")
fi

if [[ -n $CONTINUE ]]; then
  TEST=live_courses_an_earlier_run_left_unfinished_finish_preparing
  VERB=CONTINUE
  exec 3>>"$RUN.log"
else
  TEST=live_topic_courses_pass_review_and_prepare_their_first_modules
  VERB=START
  mkdir -p "$RUN" "$ROOT/settings" && cp "$SETTINGS_SRC" "$ROOT/settings/settings.json"
  exec 3>"$RUN.log"
fi
COMMIT=$(git -C "$REPO" rev-parse HEAD 2>/dev/null || true)
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(cygpath -w "$REPO/scripts/keep-awake.ps1")" &
AWAKE=$!
trap 'kill "$AWAKE" 2>/dev/null' EXIT

cd "$REPO/crates/lattice-desktop" || exit 2
echo "$VERB $LABEL ${#requests[@]} courses $(date '+%F %H:%M')"
env "${replay[@]}" \
  LATTICE_COURSE_REQUESTS="$REQUESTS" \
  LATTICE_COURSE_SETTINGS="$ROOT/settings" \
  LATTICE_LEARNING_PYTHON="$PYTHON_BUNDLE" \
  LATTICE_COURSE_ARTIFACTS="$RUN" \
  LATTICE_COURSE_MANUAL_REPAIRS="${LATTICE_COURSE_MANUAL_REPAIRS:-0}" \
  LATTICE_COURSE_RETRY_FAILED="${LATTICE_COURSE_RETRY_FAILED:-0}" \
  LATTICE_COURSE_COMMIT="${LATTICE_COURSE_COMMIT:-$COMMIT}" \
  RUST_MIN_STACK="${RUST_MIN_STACK:-33554432}" \
  "$BIN" "$TEST" --ignored --nocapture --test-threads=1 >&3 2>&3
code=$?
echo "END $LABEL exit=$code"
for request in "${requests[@]}"; do
  name=$(basename "$request" .json)
  echo "  course $name"
  if [[ -f $RUN/$name/lessons.json ]]; then
    python -c 'import json,sys; [print("    lesson", l["title"], "->", l["endState"]) for l in json.load(open(sys.argv[1], encoding="utf-8"))]' "$(cygpath -m "$RUN/$name/lessons.json")"
  else
    echo "    no lessons.json"
  fi
done
if [[ -f $RUN/run.json ]]; then
  python -c 'import json,sys; print("  calls", json.dumps(json.load(open(sys.argv[1], encoding="utf-8")).get("calls")))' "$(cygpath -m "$RUN/run.json")"
else
  echo "  no run.json"
fi
echo "ALL DONE $(date '+%F %H:%M')"
