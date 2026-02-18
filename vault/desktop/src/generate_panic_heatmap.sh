#!/bin/bash
# Oracle Strategy E: Unwrap Heatmap Generator
# Identifies panic bombs across the codebase

echo "🔍 Generating Panic Density Heatmap..."
echo "File,Unwrap_Count,Expect_Count,Panic_Count,Total_Panic_Points" > docs/audit/panic_heatmap.csv

find src -name "*.rs" -type f | while read file; do
  unwraps=$(grep -o "\.unwrap()" "$file" | wc -l | tr -d ' ')
  expects=$(grep -o "\.expect(" "$file" | wc -l | tr -d ' ')
  panics=$(grep -o "panic!(" "$file" | wc -l | tr -d ' ')
  total=$((unwraps + expects + panics))
  
  if [ $total -gt 0 ]; then
    echo "$file,$unwraps,$expects,$panics,$total" >> docs/audit/panic_heatmap.csv
  fi
done

# Sort by total panic points (descending)
(head -1 docs/audit/panic_heatmap.csv && tail -n +2 docs/audit/panic_heatmap.csv | sort -t',' -k5 -rn) > docs/audit/panic_heatmap_sorted.csv

echo "✅ Heatmap generated: docs/audit/panic_heatmap_sorted.csv"
echo ""
echo "📊 TOP 20 PANIC HOTSPOTS:"
echo "================================"
head -21 docs/audit/panic_heatmap_sorted.csv | column -t -s','

# Generate summary statistics
total_files=$(tail -n +2 docs/audit/panic_heatmap_sorted.csv | wc -l)
total_unwraps=$(tail -n +2 docs/audit/panic_heatmap_sorted.csv | awk -F',' '{sum+=$2} END {print sum}')
total_expects=$(tail -n +2 docs/audit/panic_heatmap_sorted.csv | awk -F',' '{sum+=$3} END {print sum}')
total_panics=$(tail -n +2 docs/audit/panic_heatmap_sorted.csv | awk -F',' '{sum+=$4} END {print sum}')
total_bombs=$((total_unwraps + total_expects + total_panics))

echo ""
echo "📈 SUMMARY STATISTICS:"
echo "================================"
echo "Files with panic points: $total_files"
echo "Total unwrap() calls:    $total_unwraps"
echo "Total expect() calls:    $total_expects"
echo "Total panic!() calls:    $total_panics"
echo "TOTAL PANIC BOMBS:       $total_bombs"
echo ""
echo "🎯 Priority: Focus on TOP 20 files (likely 80% of risk)"
