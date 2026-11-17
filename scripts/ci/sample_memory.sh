#!/usr/bin/env bash
# Usage: sample_memory.sh OUT          append a sample to OUT every 15 s until killed
#        sample_memory.sh --peaks OUT  print OUT's five samples with the most RAM plus swap in use
set -u

if [ "${1:-}" = --peaks ]; then
  [ -s "$2" ] || { echo "no memory samples in $2"; exit 0; }
  printf 'time (UTC)\tRAM+swap MB\tswap MB\tavailable MB\tlargest processes\n'
  sort -t$'\t' -k2,2nr "$2" | head -n 5
  exit 0
fi

out="$1"
while true; do
  read -r _ _ ram_used _ _ _ available < <(free -m | sed -n 2p)
  read -r _ _ swap_used _ < <(free -m | sed -n 3p)
  largest=$(ps -eo rss=,args= --sort=-rss | head -n 6 | awk '{
    name = $2; sub(".*/", "", name)
    for (i = 2; i < NF; i++) if ($i == "--crate-name") name = "rustc " $(i + 1)
    for (i = 2; i <= NF; i++) if ($i == "--test") name = name " (test)"
    printf "%s %d MB; ", name, $1 / 1024
  }')
  printf '%s\t%d\t%d\t%d\t%s\n' "$(date -u +%T)" $((ram_used + swap_used)) "$swap_used" "$available" "$largest" >> "$out"
  sleep 15
done
