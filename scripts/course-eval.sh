#!/bin/zsh
# Course eval driver (docs/design/2026-10-20-course-eval.md §3.4, §6.1): every
# request of a course-eval set, in set order and one at a time, through one
# snapshotted lib test binary. Each request has its own run directory, and a
# second run of the same command resumes from what those directories hold.
#
# Usage: scripts/course-eval.sh run <recipe> <label> --binary <test-binary>
#            [--requests <set.json>] [--only <id>,<id>...] [--lessons m1.l1,...]
#            [--run-dir <dir>]
#
#   <recipe>     course-eval recipe JSON, e.g. evals/courses/recipes/course-eval-9b-local.json
#   <label>      run label; the run directory is artifacts/course-eval/<date>-<label>
#   --binary     lib test binary snapshotted from `cargo test --lib --no-run`
#                (e.g. artifacts/course-eval/bin/base-<sha>)
#   --requests   request set (default: the recipe's "requests")
#   --only       run only these item ids
#   --lessons    lesson selectors, overriding the recipe's "lessons"; "" = outline only
#   --run-dir    the run directory, to resume a run started on another day
#
# Writes artifacts/course-eval/<date>-<label>/<request-id>/... (section 6.2),
# <request-id>.log beside each, and <request-id>/exit.json when a request did
# not finish. A request with done.json is skipped; a missing optional request
# is skipped with a line in its run.json.
#
# Passed through to the test from the environment: LATTICE_COURSE_WEB_MODE
# (live | replay | replay-strict), LATTICE_COURSE_WEB_TRACE (traces root),
# and the server variables the recipe's url_env/auth_env name
# (LATTICE_COURSE_SERVER_URL, LATTICE_COURSE_SERVER_AUTH).
#
# One run at a time: the model server is single-slot, so this holds
# artifacts/course-eval/.lock and refuses to start beside another run. Do not
# run the chat eval at the same time. Nothing here has a time limit: stop a
# stuck request with Ctrl-C or kill, then run the same command to resume.
set -u

SCRIPT=${0:A}
REPO=${SCRIPT:h:h}
usage() {
  sed -n '2,/^set -u$/p' $SCRIPT | sed '$d' | sed 's/^# \{0,1\}//'
}

if [[ ${1:-} == (-h|--help|help) ]] || (( $# == 0 )); then
  usage
  exit 0
fi
if [[ $1 != run ]] || (( $# < 3 )); then
  usage >&2
  exit 2
fi
ARGS=("$@")
RECIPE=${2:A}
LABEL=$3
shift 3
BIN= REQUESTS= ONLY= LESSONS= LESSONS_SET=0 RUN_DIR=
while (( $# > 0 )); do
  case $1 in
    --binary) BIN=${2:A}; shift 2 ;;
    --requests) REQUESTS=${2:A}; shift 2 ;;
    --only) ONLY=$2; shift 2 ;;
    --lessons) LESSONS=$2; LESSONS_SET=1; shift 2 ;;
    --run-dir) RUN_DIR=${2:A}; shift 2 ;;
    *) print -u2 "Unknown option: $1"; usage >&2; exit 2 ;;
  esac
done

if [[ ! -f $RECIPE ]]; then
  print -u2 "No recipe: $RECIPE"
  exit 2
fi
if [[ -z $BIN || ! -x $BIN ]]; then
  print -u2 "--binary must name an executable test binary (got: ${BIN:-nothing})"
  exit 2
fi
if [[ $LABEL == */* || -z $LABEL ]]; then
  print -u2 "The label names a directory; it cannot be empty or contain /"
  exit 2
fi

# The recipe's set and results folder, and the set's items in order.
recipe_fields=$(python3 -I -c '
import json, sys
recipe = json.load(open(sys.argv[1]))
if recipe.get("schema") != "course-eval-recipe/1":
    sys.exit(sys.argv[1] + ": not a course-eval-recipe/1 recipe")
print(recipe["requests"] + "\t" + recipe["results_model"])
' "$RECIPE") || exit 2
IFS=$'\t' read -r RECIPE_REQUESTS RESULTS_MODEL <<< $recipe_fields
if [[ -z $REQUESTS ]]; then
  if [[ $RECIPE_REQUESTS == /* ]]; then REQUESTS=$RECIPE_REQUESTS; else REQUESTS=$REPO/$RECIPE_REQUESTS; fi
fi
listing=$(python3 -I -c '
import json, os, sys
path, only = sys.argv[1], sys.argv[2]
data = json.load(open(path))
if data.get("schema") != "course-eval-requests/1":
    sys.exit(path + ": not a course-eval-requests/1 set")
ids = [item["id"] for item in data["items"]]
wanted = [x.strip() for x in only.split(",") if x.strip()]
unknown = [x for x in wanted if x not in ids]
if unknown:
    sys.exit("--only names items not in the set: " + ", ".join(unknown))
base = os.path.dirname(os.path.abspath(path))
print(data["name"])
for item in data["items"]:
    if "/" in item["id"]:
        sys.exit("item ids cannot contain /: " + item["id"])
    if wanted and item["id"] not in wanted:
        continue
    request = os.path.join(base, item["request"])
    optional = bool(item.get("optional"))
    if not optional and not os.path.isfile(request):
        sys.exit("missing request for " + item["id"] + ": " + request)
    print("\t".join([item["id"], request, "1" if optional else "0"]))
' "$REQUESTS" "$ONLY") || exit 2
rows=("${(@f)listing}")
SET_NAME=${rows[1]}
items=("${(@)rows[2,-1]}")

mkdir -p $REPO/artifacts/course-eval
# Take the lock once, keep the Mac awake, and run the rest under both.
if [[ -z ${COURSE_EVAL_LOCKED:-} ]]; then
  export COURSE_EVAL_LOCKED=1
  exec lockf -t 0 $REPO/artifacts/course-eval/.lock caffeinate -i $SCRIPT "${ARGS[@]}"
fi

RUN=${RUN_DIR:-$REPO/artifacts/course-eval/$(date +%F)-$LABEL}
mkdir -p $RUN
day=${${RUN:t}[1,10]}
[[ $day == <->-<->-<-> ]] || day=$(date +%F)
print "COURSE EVAL $RUN"
print "BINARY $BIN sha256=$(shasum -a 256 $BIN | cut -d' ' -f1)"
print "RECIPE $RECIPE"
print "SET $REQUESTS (${#items} requests)"

cd $REPO/crates/lattice-desktop
for row in $items; do
  IFS=$'\t' read -r id request optional <<< $row
  dir=$RUN/$id
  if [[ -f $dir/done.json ]]; then
    print "SKIP $id: done"
    continue
  fi
  mkdir -p $dir
  if [[ ! -f $request ]]; then
    print -r -- "{\"schema\":\"course-eval-run/1\",\"request_id\":\"$id\",\"skipped\":\"optional request file is missing on this machine\"}" > $dir/run.json
    print "SKIP $id: optional request missing ($request)"
    continue
  fi
  rm -f $dir/exit.json
  lessons=()
  (( LESSONS_SET )) && lessons=(LATTICE_COURSE_LESSONS=$LESSONS)
  start=$(date +%s)
  print "START $id $(date '+%F %H:%M')"
  print "==== $(date '+%F %H:%M:%S') start" >> $RUN/$id.log
  env LATTICE_COURSE_RECIPE=$RECIPE \
      LATTICE_COURSE_REQUEST=$request \
      LATTICE_COURSE_REQUEST_ID=$id \
      LATTICE_COURSE_RUN=$dir \
      $lessons \
    $BIN course_eval::live_course_eval --ignored --nocapture --test-threads 1 >> $RUN/$id.log 2>&1
  code=$?
  if (( code != 0 )) || [[ ! -f $dir/done.json ]]; then
    reason="the test exited $code"
    (( code == 0 )) && reason="the test exited 0 without writing done.json; does the binary contain course_eval::live_course_eval?"
    print -r -- "{\"exit\": $code, \"finished_at\": \"$(date -u +%FT%TZ)\", \"reason\": \"$reason\"}" > $dir/exit.json
  fi
  print "END $id exit=$code minutes=$(( ($(date +%s) - start) / 60 )) $(tail -c 300 $RUN/$id.log | grep -a -E 'COURSE EVAL DONE|panicked at|Error' | tail -1 | cut -c1-200)"
done

print "ALL DONE $(date '+%F %H:%M')"
print "Collect with:"
print "  python3 scripts/course_eval.py collect ${RUN#$REPO/} --out evals/courses/results/$RESULTS_MODEL/$day-$SET_NAME-$LABEL"
