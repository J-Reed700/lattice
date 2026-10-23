#!/bin/zsh
# Baseline sweep (docs/development/eval-baseline-sweep.md): every eval suite's
# established baseline procedure, in order, on the current commit, with one set
# of binaries snapshotted at the start. Each step writes a done marker and its
# own log under artifacts/baseline/<date>/, so running the same command again
# resumes: done steps are skipped, a failed step is retried.
#
# Usage: scripts/baseline-sweep.sh [--only <step>,<step>...] [--date YYYY-MM-DD]
#        scripts/baseline-sweep.sh --list [--date YYYY-MM-DD]
#
#   --only   run only these steps, in sweep order; a step whose input step is
#            not done is reported as blocked. build runs first when a chosen
#            step needs a binary and the sweep has none yet
#   --date   the sweep directory artifacts/baseline/<date>, to resume a sweep
#            started on another day (default: today)
#   --list   print the steps, what each needs and its state, then exit
#
# Steps, in order: build, retrieval-v3, retrieval-terse, chat-answers,
# chat-score, memory, claims, course-run, course-collect, course-score.
#
# Environment. No committed file names a server; the auth values are never
# printed or logged.
#   LATTICE_JUDGE_URL     the remote judge's OpenAI-compatible base URL
#   LATTICE_JUDGE_MODEL   the model name judge requests carry
#   LATTICE_JUDGE_WINDOW  the judge's context window in tokens
#   LATTICE_JUDGE_AUTH    optional: one `Header-Name: value` line
#   LATTICE_JUDGE_TAG     optional: short judge name for result file names
#                         (default: the model's file name, lower case)
#   LATTICE_MEMORY_URL, LATTICE_MEMORY_MODEL, LATTICE_MEMORY_AUTH
#                         the remote chat model the memory release baseline
#                         runs on (default: the judge's URL, model and auth)
#   LATTICE_MEMORY_CONCURRENCY     default 2, as on 2026-09-26; at most the
#                                  server's free slots
#   LATTICE_MEMORY_CONTEXT_TOKENS  default 16384, as on 2026-09-26
#   LATTICE_APP_SETTINGS  the settings file the claim step copies and points at
#                         the judge (default: the app's own settings.json)
#
# The local Qwen3.5-9B server is started from each recipe's server.launch line
# by the step that needs it (chat-answers, course-run) and stopped when that
# step ends; nothing else may listen on its port. No model call has a time limit
# set by this script; the memory harness has its own (see the doc).
set -u

SCRIPT=${0:A}
REPO=${SCRIPT:h:h}
CRATE=$REPO/crates/lattice-desktop
BASE=$REPO/artifacts/baseline
CARGO_SLOT=$HOME/.local/bin/cargo-slot
usage() {
  sed -n '2,/^set -u$/p' $SCRIPT | sed '$d' | sed 's/^# \{0,1\}//'
}

STEPS=(build retrieval-v3 retrieval-terse chat-answers chat-score memory claims course-run course-collect course-score)
typeset -A ABOUT NEEDS AFTER BINARY
ABOUT=(
  build           "snapshot the lib test binary, the memory test binary and the two eval examples"
  retrieval-v3    "synthetic-library v3, production mode, scored at k=5"
  retrieval-terse "the v3 terse slice, compared with the v3 run"
  chat-answers    "shipped-path answers from the local 9B (desktop-path-v1)"
  chat-score      "shipped-path answers graded by the remote judge"
  memory          "conversation-memory release baseline"
  claims          "Lattice's lesson claim checker on lesson-claims-w1, remote judge"
  course-run      "course eval v1, outline and lesson m1.l1 per request, local 9B"
  course-collect  "collect the course run into its results directory"
  course-score    "score the course results with the remote judge, then summarize"
)
NEEDS=(
  build           "cargo"
  retrieval-v3    "embedding model"
  retrieval-terse "embedding model"
  chat-answers    "local 9B server (16K)"
  chat-score      "remote judge"
  memory          "remote chat model"
  claims          "remote judge + window"
  course-run      "local 9B server (32K)"
  course-collect  "-"
  course-score    "remote judge + window"
)
AFTER=(
  retrieval-terse retrieval-v3
  chat-score      chat-answers
  course-collect  course-run
  course-score    course-collect
)
BINARY=(
  retrieval-v3 1 retrieval-terse 1 chat-answers 1 memory 1 claims 1 course-run 1
)

ARGS=("$@")
ONLY= DATE= LIST=0
while (( $# > 0 )); do
  case $1 in
    -h|--help|help) usage; exit 0 ;;
    --only)
      (( $# >= 2 )) || { usage >&2; exit 2; }
      ONLY=$2
      shift 2
      ;;
    --date)
      (( $# >= 2 )) || { usage >&2; exit 2; }
      DATE=$2
      shift 2
      ;;
    --list) LIST=1; shift ;;
    *) print -u2 "Unknown option: $1"; usage >&2; exit 2 ;;
  esac
done

SELECTED=()
if [[ -n $ONLY ]]; then
  wanted=(${(s:,:)ONLY})
  for name in $wanted; do
    if (( ! ${STEPS[(Ie)$name]} )); then
      print -u2 "Unknown step: $name (steps: ${(j:, :)STEPS})"
      exit 2
    fi
  done
  for name in $STEPS; do
    (( ${wanted[(Ie)$name]} )) && SELECTED+=$name
  done
else
  SELECTED=($STEPS)
fi

HEAD_SHA=$(git -C $REPO rev-parse HEAD) || exit 2
SHORT=${HEAD_SHA[1,8]}

# Without --date, today's sweep; but an unfinished sweep of this commit from
# another day is resumed on purpose, never started over by accident.
if [[ -z $DATE ]]; then
  DATE=$(date +%F)
  if [[ ! -d $BASE/$DATE ]]; then
    for marker in $BASE/*/commit(N); do
      other=${marker:h}
      [[ -f $other/complete || $(<$marker) != $HEAD_SHA ]] && continue
      if (( LIST )); then
        DATE=${other:t}
        break
      fi
      print -u2 "An unfinished sweep of this commit started on ${other:t}."
      print -u2 "Resume it:          scripts/baseline-sweep.sh --date ${other:t}${ONLY:+ --only $ONLY}"
      print -u2 "Or start a new one: scripts/baseline-sweep.sh --date $DATE${ONLY:+ --only $ONLY}"
      exit 2
    done
  fi
fi
if [[ $DATE != <->-<->-<-> ]]; then
  print -u2 "--date takes YYYY-MM-DD (got: $DATE)"
  exit 2
fi
DIR=$BASE/$DATE

state() {
  if [[ -f $DIR/done/$1 ]]; then print done
  elif [[ -f $DIR/failed/$1 ]]; then print failed
  else print pending
  fi
}
minutes_of() {
  local file=$DIR/done/$1
  [[ -f $file ]] || file=$DIR/failed/$1
  [[ -f $file ]] || return 0
  local line=$(<$file)
  [[ $line == *minutes=* ]] && print -r -- "${${line##*minutes=}%% *} min"
}

if (( LIST )); then
  print "Sweep ${DIR#$REPO/}$([[ -f $DIR/commit ]] && print " on $(<$DIR/commit)")"
  for step in $STEPS; do
    printf '  %-16s %-8s %-8s %-24s %s\n' $step $(state $step) "$(minutes_of $step)" "${NEEDS[$step]}" "${ABOUT[$step]}"
  done
  exit 0
fi

# Paths in the tree this sweep writes; they may be dirty on a resume.
dirty_files() {
  local allowed=() line file entry ok
  [[ -f $DIR/outputs.txt ]] && allowed=("${(@f)$(<$DIR/outputs.txt)}")
  git -C $REPO status --porcelain=v1 -uall | while IFS= read -r line; do
    file=${line[4,-1]}
    ok=0
    for entry in $allowed; do
      if [[ -n $entry && ( $file == $entry || $file == $entry/* || $file == $entry.* ) ]]; then
        ok=1
        break
      fi
    done
    (( ok )) || print -r -- $file
  done
}

dirty=$(dirty_files)
if [[ -n $dirty ]]; then
  print -u2 "The tree has changes this sweep did not make. A baseline measures a commit, so commit or stash them first:"
  print -u2 -r -- $dirty
  exit 2
fi
if [[ -f $DIR/commit && $(<$DIR/commit) != $HEAD_SHA ]]; then
  print -u2 "The sweep in ${DIR#$REPO/} ran on $(<$DIR/commit), but HEAD is $HEAD_SHA."
  print -u2 "Check that commit out to resume it, or start another sweep with a different --date."
  exit 2
fi

mkdir -p $BASE
# One sweep at a time, and the Mac stays awake for the rest.
if [[ -z ${BASELINE_SWEEP_LOCKED:-} ]]; then
  export BASELINE_SWEEP_LOCKED=1
  exec lockf -t 0 $BASE/.lock caffeinate -i $SCRIPT "${ARGS[@]}"
fi

mkdir -p $DIR/done $DIR/failed $DIR/logs $DIR/bin
[[ -f $DIR/commit ]] || print -r -- $HEAD_SHA > $DIR/commit
touch $DIR/outputs.txt

LIB_BIN=$DIR/bin/lattice-$SHORT
MEM_BIN=$DIR/bin/conversation_memory_evals-$SHORT
SHIP_BIN=$DIR/bin/shipped_path_eval-$SHORT
RET_BIN=$DIR/bin/retrieval_eval-$SHORT
if [[ -f $DIR/done/build ]] && ! (cd $DIR/bin && shasum -a 256 -s -c SHA256SUMS); then
  print -u2 "The snapshotted binaries in ${DIR#$REPO/}/bin no longer match their SHA256SUMS; this sweep cannot be resumed."
  exit 2
fi

EMB=$HOME/.cache/lattice/models/qwen3-embedding-0.6b/Qwen3-Embedding-0.6B-f16.gguf
V3=evals/retrieval/synthetic-library-v3.json
TERSE=evals/retrieval/synthetic-library-v3-terse.json
CHAT_RECIPE=evals/retrieval/shipped-path-recipe-9b-local.json
CHAT_OUT=evals/retrieval/results/chat/qwen3.5-9b-local/$DATE-shipped-path-desktop-path-v1
COURSE_RECIPE=evals/courses/recipes/course-eval-9b-local.json
CLAIM_GOLD=evals/grounding/lesson-claims-w1/gold-w1.jsonl
RETRIEVAL_FLAGS=(--mode production --top-k 10 --compression none)

JUDGE_TAG=${LATTICE_JUDGE_TAG:-}
if [[ -z $JUDGE_TAG && -n ${LATTICE_JUDGE_MODEL:-} ]]; then
  JUDGE_TAG=${(L)${LATTICE_JUDGE_MODEL:t}%.gguf}
fi
JUDGE_TAG=${JUDGE_TAG//[^A-Za-z0-9._-]/-}

# ---- helpers --------------------------------------------------------------

say() { print -r -- "$@" }

claim_output() {
  local file
  for file in "$@"; do
    grep -qxF -- $file $DIR/outputs.txt || print -r -- $file >> $DIR/outputs.txt
  done
}

# A runner that refuses an existing output gets a clean path; what an earlier,
# interrupted attempt left is moved under the sweep directory, never deleted.
set_aside() {
  local file dest=$DIR/partial/$(date +%Y%m%d-%H%M%S)
  for file in "$@"; do
    [[ -e $file ]] || continue
    mkdir -p $dest
    mv $file $dest/
    say "SET ASIDE ${file#$REPO/} -> ${dest#$REPO/}/"
  done
}

recipe_field() {
  python3 -I -c '
import json, sys
value = json.load(open(sys.argv[1]))
for key in sys.argv[2].split("."):
    value = value[key]
print(value)
' $REPO/$1 $2
}

need_judge() {  # [window]
  local missing=()
  [[ -n ${LATTICE_JUDGE_URL:-} ]] || missing+=LATTICE_JUDGE_URL
  [[ -n ${LATTICE_JUDGE_MODEL:-} ]] || missing+=LATTICE_JUDGE_MODEL
  if [[ ${1:-} == window && ${LATTICE_JUDGE_WINDOW:-} != <-> ]]; then
    missing+="LATTICE_JUDGE_WINDOW (a whole number of tokens)"
  fi
  if (( ${#missing} )); then
    say "MISSING: this step needs ${(j:, :)missing}. The judge is named only through the environment."
    return 2
  fi
  say "JUDGE model $LATTICE_JUDGE_MODEL, tag $JUDGE_TAG, auth ${${LATTICE_JUDGE_AUTH:+set}:-not set}"
}

need_path() {  # path what
  [[ -e $1 ]] && return 0
  say "MISSING: $2 at $1"
  return 2
}

SERVER_PID=
start_server() {  # recipe log
  local launch url port program gguf
  launch=$(recipe_field $1 server.launch) || return 2
  url=$(recipe_field $1 server.url) || return 2
  gguf=$(recipe_field $1 server.gguf) || return 2
  gguf=${gguf/#\~/$HOME}
  program=${${(z)launch}[1]}
  need_path $REPO/$program "the bundled llama-server (copy crates/lattice-desktop/binaries/ from the main checkout)" || return 2
  need_path $gguf "the recipe's weights" || return 2
  port=${${url##*:}%%/*}
  if lsof -nP -iTCP:$port -sTCP:LISTEN >/dev/null 2>&1; then
    say "Something already listens on port $port. Stop it (the app's chat, another eval) and run the sweep again."
    return 2
  fi
  say "SERVER start ($1): $launch"
  (cd $REPO && exec zsh -fc "exec $launch") >> $2 2>&1 &
  SERVER_PID=$!
  say "SERVER pid $SERVER_PID, log ${2#$REPO/}; waiting for $url/health"
  until curl -sf -o /dev/null $url/health; do
    if ! kill -0 $SERVER_PID 2>/dev/null; then
      say "SERVER exited before it was ready; see ${2#$REPO/}"
      SERVER_PID=
      return 1
    fi
    sleep 2
  done
  say "SERVER ready $(date '+%F %H:%M:%S')"
}
stop_server() {
  [[ -z $SERVER_PID ]] && return 0
  kill $SERVER_PID 2>/dev/null
  wait $SERVER_PID 2>/dev/null
  say "SERVER stopped (pid $SERVER_PID)"
  SERVER_PID=
}
trap 'stop_server' EXIT
trap 'stop_server; exit 130' INT TERM

BUILD_OUT=
cargo_logged() {
  local code out=$DIR/logs/build.last-cargo.out
  say "+ (crates/lattice-desktop) ${*}"
  "$@" 2>&1 | tee $out
  code=${pipestatus[1]}
  BUILD_OUT=$(<$out)
  (( code == 0 )) || say "exited $code"
  return $code
}
executable() {  # the path cargo printed after "Executable <unit> (" in the last build
  print -r -- $BUILD_OUT | sed -n "s|.*Executable $1 (\(.*\))\$|\1|p" | tail -1
}

# ---- steps ----------------------------------------------------------------

step_build() {
  local lib mem target
  need_path $CARGO_SLOT "cargo-slot" || return 2
  cd $CRATE || return 1
  cargo_logged env CARGO_INCREMENTAL=0 $CARGO_SLOT cargo test --lib --no-run -j 6 || return 1
  lib=$(executable 'unittests src/lib.rs')
  cargo_logged env CARGO_INCREMENTAL=0 $CARGO_SLOT cargo test --test conversation_memory_evals --no-run -j 6 || return 1
  mem=$(executable 'tests/conversation_memory_evals.rs')
  cargo_logged env CARGO_INCREMENTAL=0 $CARGO_SLOT cargo build --example shipped_path_eval -j 6 || return 1
  cargo_logged env CARGO_INCREMENTAL=0 \
    CARGO_PROFILE_EVALOPT_INHERITS=dev CARGO_PROFILE_EVALOPT_OPT_LEVEL=3 \
    CARGO_PROFILE_EVALOPT_DEBUG=false CARGO_PROFILE_EVALOPT_DEBUG_ASSERTIONS=false \
    CARGO_PROFILE_EVALOPT_OVERFLOW_CHECKS=false SQLX_OFFLINE=true \
    $CARGO_SLOT cargo build --profile evalopt --example retrieval_eval -j 6 || return 1
  if [[ -z $lib || -z $mem ]]; then
    say "Could not read the test binaries' paths from cargo's output"
    return 1
  fi
  lib=${lib:A} mem=${mem:A}
  target=${lib:h:h:h}
  cp $lib $LIB_BIN && cp $mem $MEM_BIN \
    && cp $target/debug/examples/shipped_path_eval $SHIP_BIN \
    && cp $target/evalopt/examples/retrieval_eval $RET_BIN || return 1
  (cd $DIR/bin && shasum -a 256 ${LIB_BIN:t} ${MEM_BIN:t} ${SHIP_BIN:t} ${RET_BIN:t} > SHA256SUMS) || return 1
  cat $DIR/bin/SHA256SUMS
  dirty=$(dirty_files)
  if [[ -n $dirty ]]; then
    say "The build changed files in the tree; the binaries no longer describe a clean commit:"
    print -r -- $dirty
    return 1
  fi
}

step_retrieval_v3() {
  local work=$DIR/retrieval out=evals/retrieval/results/$DATE-v3-BASELINE
  need_path $EMB "the embedding model" || return 2
  mkdir -p $work $DIR/embed-cache
  claim_output $out-run.jsonl $out-metrics.json
  cd $CRATE || return 1
  say "+ (crates/lattice-desktop) LATTICE_EMBED_GGUF=$EMB RETRIEVAL_EVAL_EMBED_CACHE=${DIR#$REPO/}/embed-cache retrieval_eval ${RETRIEVAL_FLAGS[*]} ../../$V3"
  LATTICE_EMBED_GGUF=$EMB RETRIEVAL_EVAL_EMBED_CACHE=$DIR/embed-cache $RET_BIN $RETRIEVAL_FLAGS ../../$V3 > $work/v3-run.jsonl.tmp || return 1
  mv $work/v3-run.jsonl.tmp $work/v3-run.jsonl
  cd $REPO
  say "+ python3 scripts/rag_eval.py $V3 <run> --k 5"
  python3 scripts/rag_eval.py $V3 $work/v3-run.jsonl --k 5 > $work/v3-metrics.json || return 1
  cp $work/v3-run.jsonl $out-run.jsonl && cp $work/v3-metrics.json $out-metrics.json || return 1
  python3 -I -c '
import json, sys
m = json.load(open(sys.argv[1]))
print("v3: recall@5 %.4f  nDCG@5 %.4f  MRR@5 %.4f  passage recall@5 %s" % (
    m["recall_at_k"], m["ndcg_at_k"], m["mrr_at_k"], m.get("passage_recall_at_k")))
' $out-metrics.json
}

step_retrieval_terse() {
  local work=$DIR/retrieval out=evals/retrieval/results/$DATE-v3-BASELINE
  need_path $EMB "the embedding model" || return 2
  need_path $work/v3-run.jsonl "the v3 run from retrieval-v3" || return 2
  claim_output $out-terse-run.jsonl $out-terse-metrics.json
  cd $REPO
  say "+ python3 scripts/terse_slice.py check $TERSE"
  python3 scripts/terse_slice.py check $TERSE || return 1
  say "+ python3 scripts/terse_slice.py build $TERSE <work>/terse.json"
  python3 scripts/terse_slice.py build $TERSE $work/terse.json || return 1
  cd $CRATE || return 1
  say "+ (crates/lattice-desktop) LATTICE_EMBED_GGUF=$EMB RETRIEVAL_EVAL_EMBED_CACHE=${DIR#$REPO/}/embed-cache retrieval_eval ${RETRIEVAL_FLAGS[*]} <work>/terse.json"
  LATTICE_EMBED_GGUF=$EMB RETRIEVAL_EVAL_EMBED_CACHE=$DIR/embed-cache $RET_BIN $RETRIEVAL_FLAGS $work/terse.json > $work/terse-run.jsonl.tmp || return 1
  mv $work/terse-run.jsonl.tmp $work/terse-run.jsonl
  cd $REPO
  say "+ python3 scripts/terse_slice.py compare $TERSE <v3 run> <terse run>"
  python3 scripts/terse_slice.py compare $TERSE $work/v3-run.jsonl $work/terse-run.jsonl > $work/terse-metrics.json || return 1
  cp $work/terse-run.jsonl $out-terse-run.jsonl && cp $work/terse-metrics.json $out-terse-metrics.json || return 1
  python3 -I -c '
import json, sys
m = json.load(open(sys.argv[1]))
print("terse slice: queries", m.get("queries"), " p =", m.get("p_value"), " verdict:", m.get("verdict"))
' $out-terse-metrics.json
}

step_chat_answers() {
  claim_output $CHAT_OUT
  set_aside $REPO/$CHAT_OUT.jsonl(N) $REPO/$CHAT_OUT.jsonl.*(N)
  start_server $CHAT_RECIPE $DIR/logs/chat-answers.server.log || return $?
  cd $CRATE || return 1
  say "+ (crates/lattice-desktop) shipped_path_eval ../../$CHAT_RECIPE ../../$CHAT_OUT.jsonl"
  $SHIP_BIN ../../$CHAT_RECIPE ../../$CHAT_OUT.jsonl || return 1
  stop_server
  # The model-call record is committed gzipped, as the earlier runs are.
  if [[ -f $REPO/$CHAT_OUT.jsonl.calls.jsonl ]]; then
    say "+ gzip $CHAT_OUT.jsonl.calls.jsonl"
    gzip $REPO/$CHAT_OUT.jsonl.calls.jsonl || return 1
  fi
}

step_chat_score() {
  need_judge || return 2
  local scored=$CHAT_OUT.judge-$JUDGE_TAG.scored.jsonl
  set_aside $REPO/$scored(N) $REPO/$scored.*(N)
  cd $REPO
  say "+ python3 scripts/shipped_path_eval.py score evals/retrieval/desktop-path-v1.json $CHAT_OUT.jsonl $scored --endpoint \$LATTICE_JUDGE_URL --model $LATTICE_JUDGE_MODEL"
  python3 scripts/shipped_path_eval.py score evals/retrieval/desktop-path-v1.json $CHAT_OUT.jsonl $scored \
    --endpoint $LATTICE_JUDGE_URL --model $LATTICE_JUDGE_MODEL || return 1
  python3 -I -c '
import json, sys
o = json.load(open(sys.argv[1]))["overall"]
for key in ("mechanical", "judge", "both"):
    print("%-10s %d/%d" % (key, o[key]["passed"], o[key]["total"]))
' $scored.summary.json
}

step_memory() {
  setopt localoptions extendedglob
  local url model auth name= value= endpoint resume
  if [[ -n ${LATTICE_MEMORY_URL:-} ]]; then
    url=$LATTICE_MEMORY_URL auth=${LATTICE_MEMORY_AUTH:-}
  else
    url=${LATTICE_JUDGE_URL:-} auth=${LATTICE_JUDGE_AUTH:-}
  fi
  model=${LATTICE_MEMORY_MODEL:-${LATTICE_JUDGE_MODEL:-}}
  local concurrency=${LATTICE_MEMORY_CONCURRENCY:-2} context=${LATTICE_MEMORY_CONTEXT_TOKENS:-16384}
  if [[ -z $url || -z $model ]]; then
    say "MISSING: the remote chat model. Set LATTICE_MEMORY_URL and LATTICE_MEMORY_MODEL (and LATTICE_MEMORY_AUTH), or the judge's variables."
    return 2
  fi
  if [[ -n $auth ]]; then
    name=${${auth%%:*}//[[:space:]]/}
    value=${${${auth#*:}##[[:space:]]#}%%[[:space:]]#}
    if [[ $auth != *:* || -z $name || -z $value ]]; then
      say "The memory model's auth variable must read \`Header-Name: value\`"
      return 2
    fi
  fi
  endpoint=${url%/}
  [[ $endpoint == */v1 ]] || endpoint=$endpoint/v1
  mkdir -p $DIR/memory-traces
  # The harness resumes from one trace's completed cells; gather every earlier
  # attempt's, so a second interruption loses nothing.
  resume=$DIR/memory-resume.jsonl
  python3 -I - $DIR/memory-traces $resume <<'EOF'
import json, sys
from pathlib import Path
traces, out = Path(sys.argv[1]), Path(sys.argv[2])
kept = []
for path in sorted(traces.glob("lattice-conversation-memory-evals-*.jsonl")):
    for line in path.read_text().splitlines():
        try:
            record = json.loads(line)
        except ValueError:
            continue
        if record.get("event") == "run_complete":
            kept.append(line)
out.write_text("".join(line + "\n" for line in kept))
print(f"RESUME {len(kept)} completed cells from earlier attempts")
EOF
  [[ -s $resume ]] || resume=
  say "MEMORY model $model, auth ${${auth:+set}:-not set}, runs 3, concurrency $concurrency, context $context,"
  say "       request timeout and compaction deadline 900 s (the 2026-09-26 baseline's; the harness requires both)"
  cd $CRATE || return 1
  say "+ (crates/lattice-desktop) conversation_memory_evals release_baseline_covers_every_family_baseline_and_ablation --ignored --nocapture --test-threads=1"
  local started=$(date +%s)
  (
    export LATTICE_EVAL_PROVIDER=llamacpp
    export LATTICE_EVAL_UTILITY_MODEL=$model LATTICE_EVAL_CONTINUATION_MODEL=$model
    export LATTICE_EVAL_ENDPOINT=$endpoint
    export LATTICE_EVAL_AUTH_HEADER_NAME=$name LATTICE_EVAL_AUTH_HEADER_VALUE=$value
    export LATTICE_EVAL_PROVIDER_ATTEMPTS=3 LATTICE_EVAL_RETRY_BASE_DELAY_MS=1000
    export LATTICE_EVAL_RUNS=3 LATTICE_EVAL_CONCURRENCY=$concurrency
    export LATTICE_EVAL_CONTEXT_TOKENS=$context
    export LATTICE_EVAL_REQUEST_TIMEOUT_SECS=900 LATTICE_EVAL_COMPACTION_DEADLINE_SECS=900
    export LATTICE_EVAL_LOG_DIR=$DIR/memory-traces
    [[ -n $resume ]] && export LATTICE_EVAL_RESUME_TRACE=$resume
    exec $MEM_BIN release_baseline_covers_every_family_baseline_and_ablation --ignored --nocapture --test-threads=1
  )
  local code=$?
  # A run that reached the end failed only on its gates: that is the
  # baseline's result, not a step failure. Anything else is retried.
  python3 -I - $DIR/memory-traces $started <<'EOF'
import json, sys
from pathlib import Path
traces, started = Path(sys.argv[1]), float(sys.argv[2])
for path in sorted(traces.glob("lattice-conversation-memory-evals-*.jsonl"), key=lambda p: p.stat().st_mtime, reverse=True):
    if path.stat().st_mtime < started:
        break
    for line in path.read_text().splitlines():
        try:
            record = json.loads(line)
        except ValueError:
            continue
        if record.get("event") == "release_baseline_complete":
            failures = record.get("failures") or []
            print(f"MEMORY release baseline complete; {len(failures)} gate failure(s)")
            for failure in failures:
                print("  -", failure)
            sys.exit(0)
print("MEMORY the release baseline did not reach its end")
sys.exit(1)
EOF
  local complete=$?
  (( code == 0 || complete == 0 ))
}

step_claims() {
  need_judge window || return 2
  local source=${LATTICE_APP_SETTINGS:-$HOME/Library/Application Support/tech.lattice.app/settings.json}
  need_path $source "the settings file to copy (set LATTICE_APP_SETTINGS)" || return 2
  local checker=$JUDGE_TAG-lesson-judge
  local results=evals/grounding/results/$DATE-lesson-w1-$checker.jsonl
  local scores=evals/grounding/results/$DATE-lesson-w1-scores.json
  local settings=$DIR/claims-judge-settings.json code
  claim_output $results $scores
  # A settings copy pointing at the judge, as the testing doc describes; it
  # holds the auth header, so it is private and removed when the run ends.
  python3 -I - $source $settings <<'EOF' || return 2
import json, os, sys
source, out = sys.argv[1], sys.argv[2]
data = json.load(open(source))
llm = data["settings"]["llm"]
auth = os.environ.get("LATTICE_JUDGE_AUTH", "")
name, separator, value = auth.partition(":")
if auth and (not separator or not name.strip() or not value.strip()):
    sys.exit("LATTICE_JUDGE_AUTH must read `Header-Name: value`")
llm["llamaCpp"] = {
    "url": os.environ["LATTICE_JUDGE_URL"],
    "model": os.environ["LATTICE_JUDGE_MODEL"],
    "authHeaderName": name.strip(),
    "authHeaderValue": value.strip(),
}
llm["contextWindow"] = int(os.environ["LATTICE_JUDGE_WINDOW"])
fd = os.open(out, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
with os.fdopen(fd, "w") as handle:
    json.dump(data, handle)
EOF
  {
    cd $CRATE || return 1
    say "+ (crates/lattice-desktop) LATTICE_LLAMACPP_SETTINGS=<judge settings copy> LATTICE_CLAIM_PAIRS=$CLAIM_GOLD LATTICE_CLAIM_RESULTS=$results LATTICE_CLAIM_CHECKER=$checker lattice live_claim_pairs_eval --ignored --nocapture"
    (
      export LATTICE_LLAMACPP_SETTINGS=$settings
      export LATTICE_CLAIM_PAIRS=$REPO/$CLAIM_GOLD LATTICE_CLAIM_RESULTS=$REPO/$results
      export LATTICE_CLAIM_CHECKER=$checker
      exec $LIB_BIN live_claim_pairs_eval --ignored --nocapture
    )
    code=$?
  } always {
    rm -f $settings
  }
  (( code == 0 )) || return $code
  cd $REPO
  say "+ python3 scripts/claim_checker_bakeoff_score.py --dataset $CLAIM_GOLD --json-out $scores $results"
  python3 scripts/claim_checker_bakeoff_score.py --dataset $CLAIM_GOLD --json-out $scores $results
}

course_results_dir() {
  local model set
  model=$(recipe_field $COURSE_RECIPE results_model) || return 1
  set=$(python3 -I -c '
import json, sys
recipe = json.load(open(sys.argv[1]))
print(json.load(open(sys.argv[2] + "/" + recipe["requests"]))["name"])
' $REPO/$COURSE_RECIPE $REPO) || return 1
  print -r -- evals/courses/results/$model/$DATE-$set-base
}

step_course_run() {
  local run=$DIR/course-eval traces=$DIR/course-web-traces
  need_path $EMB "the embedding model" || return 2
  mkdir -p $traces
  start_server $COURSE_RECIPE $DIR/logs/course-run.server.log || return $?
  cd $REPO
  # A baseline is recorded by replaying against an empty trace
  # (evals/courses/README.md, Web replay); this sweep's own trace directory is.
  say "+ LATTICE_COURSE_WEB_TRACE=${traces#$REPO/} scripts/course-eval.sh run $COURSE_RECIPE base --binary ${LIB_BIN#$REPO/} --lessons m1.l1 --run-dir ${run#$REPO/}"
  LATTICE_COURSE_WEB_TRACE=$traces scripts/course-eval.sh run $COURSE_RECIPE base \
    --binary $LIB_BIN --lessons m1.l1 --run-dir $run || return 1
  stop_server
  python3 -I - $REPO/$COURSE_RECIPE $run $REPO <<'EOF'
import json, sys
from pathlib import Path
recipe, run, repo = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
items = json.loads((repo / json.loads(recipe.read_text())["requests"]).read_text())["items"]
unfinished = []
for item in items:
    directory = run / item["id"]
    if (directory / "done.json").exists():
        continue
    meta = directory / "run.json"
    if meta.exists() and "skipped" in json.loads(meta.read_text()):
        continue
    unfinished.append(item["id"])
if unfinished:
    sys.exit("COURSE requests without done.json: " + ", ".join(unfinished)
             + " (see their exit.json and .log); run the sweep again to resume them")
print(f"COURSE all {len(items)} requests finished or skipped")
EOF
}

step_course_collect() {
  local out
  out=$(course_results_dir) || return 1
  claim_output $out
  set_aside $REPO/$out(N)
  cd $REPO
  say "+ python3 scripts/course_eval.py collect ${DIR#$REPO/}/course-eval --out $out"
  python3 scripts/course_eval.py collect $DIR/course-eval --out $out
}

step_course_score() {
  need_judge window || return 2
  local out
  out=$(course_results_dir) || return 1
  cd $REPO
  say "+ python3 scripts/course_eval.py score $out --endpoint \$LATTICE_JUDGE_URL --model $LATTICE_JUDGE_MODEL --window $LATTICE_JUDGE_WINDOW"
  python3 scripts/course_eval.py score $out --endpoint $LATTICE_JUDGE_URL \
    --model $LATTICE_JUDGE_MODEL --window $LATTICE_JUDGE_WINDOW || return 1
  say "+ python3 scripts/course_eval.py summarize $out"
  python3 scripts/course_eval.py summarize $out
}

# ---- the sweep ------------------------------------------------------------

if [[ ! -f $DIR/done/build ]] && (( ! ${SELECTED[(Ie)build]} )); then
  for step in $SELECTED; do
    if [[ -n ${BINARY[$step]:-} ]]; then
      print "build is not done and $step needs its binary: building first"
      SELECTED=(build $SELECTED)
      break
    fi
  done
fi

print "BASELINE SWEEP ${DIR#$REPO/} on $HEAD_SHA"
print "STEPS ${(j: :)SELECTED}"
print "Resume or retry with the same command${DATE:+ (another day: --date $DATE)}."
FAILED=() BLOCKED=()
for step in $SELECTED; do
  log=$DIR/logs/$step.log
  if [[ -f $DIR/done/$step ]]; then
    print "SKIP $step: done ($(minutes_of $step))"
    continue
  fi
  dep=${AFTER[$step]:-}
  [[ -n ${BINARY[$step]:-} && ! -f $DIR/done/build ]] && dep=build
  if [[ -n $dep && ! -f $DIR/done/$dep ]]; then
    print "BLOCKED $step: needs $dep"
    BLOCKED+=$step
    continue
  fi
  # The scorers are read from the tree, so a checkout or an edit mid-sweep
  # would measure other code: stop rather than mix two commits.
  if [[ $(git -C $REPO rev-parse HEAD) != $HEAD_SHA ]]; then
    print "STOP: HEAD moved off $HEAD_SHA during the sweep. Check it out again and rerun to resume."
    exit 2
  fi
  dirty=$(dirty_files)
  if [[ -n $dirty ]]; then
    print "STOP: the tree changed during the sweep; restore these, then rerun to resume:"
    print -r -- $dirty
    exit 2
  fi
  rm -f $DIR/failed/$step
  started=$(date +%s)
  started_at=$(date -u +%FT%TZ)
  print "START $step $(date '+%F %H:%M')  log: ${log#$REPO/}"
  print "==== $(date '+%F %H:%M:%S') start $step on $HEAD_SHA" >> $log
  step_${step//-/_} >> $log 2>&1
  code=$?
  stop_server >> $log 2>&1
  minutes=$(( ($(date +%s) - started) / 60 ))
  if (( code == 0 )); then
    print -r -- "commit=$HEAD_SHA started=$started_at finished=$(date -u +%FT%TZ) minutes=$minutes" > $DIR/done/$step
    print "DONE $step minutes=$minutes"
  else
    print -r -- "commit=$HEAD_SHA exit=$code started=$started_at finished=$(date -u +%FT%TZ) minutes=$minutes" > $DIR/failed/$step
    print "FAILED $step exit=$code minutes=$minutes: $(grep -a -E 'MISSING|exited|panicked at|Error|error' $log | tail -1 | cut -c1-200)"
    FAILED+=$step
  fi
done

all_done=1
for step in $STEPS; do
  [[ -f $DIR/done/$step ]] || all_done=0
done
(( all_done )) && print -r -- "commit=$HEAD_SHA finished=$(date -u +%FT%TZ)" > $DIR/complete

print
print "STATE"
for step in $STEPS; do
  printf '  %-16s %-8s %s\n' $step $(state $step) "$(minutes_of $step)"
done

print
print "RESULT FILES TO COMMIT (written by this sweep so far)"
for entry in ${(f)"$(<$DIR/outputs.txt)"}; do
  [[ -n $entry ]] || continue
  for file in $REPO/$entry(N) $REPO/$entry.*(N); do
    if [[ -d $file ]]; then print "  ${file#$REPO/}/"; else print "  ${file#$REPO/}"; fi
  done
done
if [[ -f $DIR/done/memory ]]; then
  print "  evals/conversation-memory/results/<model folder>/$DATE-release-baseline.md: write it by hand from the"
  print "    printed summary in ${DIR#$REPO/}/logs/memory.log, headed like the 2026-09-26 files (never commit the traces)"
fi

print
print "HISTORY TO UPDATE"
print "  retrieval   evals/retrieval/README.md: a row in the production-mode table (\"| Run | recall@5 | ...\", section"
print "              \"Production mode now runs chat's search\"); the terse gap beside \"Terse slice of v3\""
print "  chat        no table: the run history is the folder evals/retrieval/results/chat/qwen3.5-9b-local/"
print "              (EVALUATION_PROTOCOL.md); compare with scripts/shipped_path_stats.py compare"
print "  memory      no table: one file per run in evals/conversation-memory/results/<model>/ (its README)"
print "  claims      evals/grounding/lesson-claims-w1/README.md \"Results\" table, and the table in"
print "              docs/development/course-generation-testing.md \"5. Results on lesson-claims-w1\""
print "  course      docs/development/course-generation-history.md \"Scored eval runs\" table"
print "  The course baseline's web trace is ${DIR#$REPO/}/course-web-traces; a candidate replays it with"
print "  LATTICE_COURSE_WEB_TRACE=${DIR#$REPO/}/course-web-traces."

if (( ${#FAILED} || ${#BLOCKED} )); then
  print
  (( ${#FAILED} )) && print "FAILED: ${(j:, :)FAILED} (logs in ${DIR#$REPO/}/logs/)"
  (( ${#BLOCKED} )) && print "BLOCKED: ${(j:, :)BLOCKED}"
  print "Fix what the log names and run the same command again; done steps are skipped."
  exit 1
fi
print "ALL SELECTED STEPS DONE $(date '+%F %H:%M')"
