#!/bin/zsh
# Live end-to-end course runs: every request JSON in <requests-dir>, one after
# another, through one snapshotted test binary. Each run gets a new artifact
# directory and a fresh throwaway SQLite database (course.sqlite). The runs use
# the real model server named in the app settings and the real web.
#
# Usage: scripts/live-course-e2e.sh <binary> <label> <requests-dir> [artifacts-root]
#
#   <binary>         lib test binary snapshotted from `cargo test --lib --no-run`
#   <label>          run label; run directories are <label>-<request name>
#   <requests-dir>   directory of GenerateLearningProgramRequestDto JSON files
#                    (topic-only: documentIds and sourceUrls empty)
#   [artifacts-root] default: <repo>/artifacts/course-e2e-<today>
#
# Environment:
#   LATTICE_LLAMACPP_SETTINGS      settings.json whose settings.llm is used
#                                  (default: the app's own settings file)
#   LATTICE_COURSE_MANUAL_REPAIRS  Repair presses allowed after generation (default 0)
#
# See docs/development/course-generation-testing.md before running: the model
# server is single-slot, so the app's chat queues behind these runs.
set -u

if (( $# < 3 || $# > 4 )); then
  print -u2 "Usage: $0 <binary> <label> <requests-dir> [artifacts-root]"
  exit 2
fi

REPO=${0:A:h:h}
BIN=${1:A}
LABEL=$2
REQUESTS=${3:A}
ROOT=${4:-$REPO/artifacts/course-e2e-$(date +%F)}
ROOT=${ROOT:A}
SETTINGS=${LATTICE_LLAMACPP_SETTINGS:-"$HOME/Library/Application Support/tech.lattice.app/settings.json"}

if [[ ! -x $BIN ]]; then
  print -u2 "Not an executable test binary: $BIN"
  exit 2
fi
if [[ ! -d $REQUESTS ]]; then
  print -u2 "No requests directory: $REQUESTS"
  exit 2
fi
if [[ ! -f $SETTINGS ]]; then
  print -u2 "No settings file: $SETTINGS (set LATTICE_LLAMACPP_SETTINGS)"
  exit 2
fi
requests=($REQUESTS/*.json(N))
if (( ${#requests} == 0 )); then
  print -u2 "No request JSON files in $REQUESTS"
  exit 2
fi
for request in $requests; do
  if [[ -e $ROOT/$LABEL-${request:t:r} ]]; then
    print -u2 "Run directory already exists: $ROOT/$LABEL-${request:t:r}. Choose a new label."
    exit 2
  fi
done

mkdir -p $ROOT
SUMMARY=$ROOT/$LABEL-summary.txt
: > $SUMMARY
print "BINARY $BIN sha256=$(shasum -a 256 $BIN | cut -d' ' -f1)" >> $SUMMARY
print "COMMIT $(git -C $REPO rev-parse --short HEAD 2>/dev/null || print unknown)" >> $SUMMARY

cd $REPO/crates/lattice-desktop
for request in $requests; do
  name=${request:t:r}
  dir=$ROOT/$LABEL-$name
  start=$(date +%s)
  print "START $name $(date '+%F %H:%M')" >> $SUMMARY
  LATTICE_LLAMACPP_SETTINGS="$SETTINGS" \
  LATTICE_COURSE_REQUEST=$request \
  LATTICE_COURSE_ARTIFACTS=$dir \
  LATTICE_COURSE_MANUAL_REPAIRS=${LATTICE_COURSE_MANUAL_REPAIRS:-0} \
    $BIN live_topic_course_passes_review --ignored --nocapture > $dir.log 2>&1
  code=$?
  minutes=$(( ($(date +%s) - start) / 60 ))
  result=$(grep -a -E "LIVE COURSE CONFIRMED|LIVE COURSE generated|panicked at" $dir.log | tail -2 | tr '\n' ' ' | cut -c1-400)
  print "END $name exit=$code minutes=$minutes $result" >> $SUMMARY
done
print "ALL DONE $(date '+%F %H:%M')" >> $SUMMARY
