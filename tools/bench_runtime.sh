#!/usr/bin/env bash
set -euo pipefail

BENCH_KEEP_WORKTREE=0
BENCH_BASELINE_WT=""
BENCH_REPO_ROOT=""
DOC_WIDGETS_MANIFEST="docs/widgets/Cargo.toml"

usage() {
    cat <<'EOF'
Usage: tools/bench_runtime.sh --baseline <commit-ish> [options]

Options:
  --baseline <commit>   Baseline commit/tag/branch to compare against (required)
  --runs <n>            Runs per scenario per revision (default: 5)
  --timeout <seconds>   Timeout per run (default: 8)
  --out-dir <path>      Output directory (default: /tmp/textual-bench-<ts>)
  --keep-worktree       Keep temporary baseline worktree under /tmp
  --help                Show this help

Scenarios:
  - tabbed_content: command palette navigation + search stress
  - tick: sustained typing while ticks are firing

Notes:
  - This script requires PTY allocation (`script` command).
  - It emits machine-readable TSV and a human-readable markdown summary.
EOF
}

require_cmd() {
    local cmd="$1"
    command -v "$cmd" >/dev/null 2>&1 || {
        echo "missing required command: $cmd" >&2
        exit 1
    }
}

calc_stats() {
    # Prints: count p50 p95 max
    local values_file="$1"
    local sorted_file="$2"
    if [[ ! -s "$values_file" ]]; then
        echo "0 - - -"
        return
    fi
    sort -n "$values_file" >"$sorted_file"
    local count
    count=$(wc -l <"$sorted_file")
    local p50_i p95_i p50 p95 max
    p50_i=$(( (count * 50 + 99) / 100 ))
    p95_i=$(( (count * 95 + 99) / 100 ))
    p50=$(sed -n "${p50_i}p" "$sorted_file")
    p95=$(sed -n "${p95_i}p" "$sorted_file")
    max=$(tail -n 1 "$sorted_file")
    echo "$count $p50 $p95 $max"
}

extract_loop_metrics() {
    # Args: timing_log key_total_out key_dispatch_out render_nonzero_out tick_loops_out
    local timing_log="$1"
    local total_out="$2"
    local input_out="$3"
    local render_out="$4"
    local tick_out="$5"
    awk '
      /\[timing\] loop / {
        is_input = ($0 !~ / input=none /);
        if (is_input && match($0, /total_us=([0-9]+)/, m1)) print m1[1] >> TOTAL;
        if (is_input && match($0, /input_dispatch_us=([0-9]+)/, m2)) print m2[1] >> INPUT;
        if (match($0, /render_us\(immediate=([0-9]+) normal=([0-9]+) tick=([0-9]+)\)/, m3)) {
          sum = m3[1] + m3[2] + m3[3];
          if (sum > 0) print sum >> RENDER;
          if (m3[3] > 0) print 1 >> TICK;
        }
      }
    ' TOTAL="$total_out" INPUT="$input_out" RENDER="$render_out" TICK="$tick_out" "$timing_log"
}

main() {
    require_cmd git
    require_cmd cargo
    require_cmd script
    require_cmd timeout
    require_cmd awk

    local baseline=""
    local runs=5
    local timeout_s=8
    local out_dir=""
    local keep_worktree=0

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --baseline)
                baseline="${2:-}"
                shift 2
                ;;
            --runs)
                runs="${2:-}"
                shift 2
                ;;
            --timeout)
                timeout_s="${2:-}"
                shift 2
                ;;
            --out-dir)
                out_dir="${2:-}"
                shift 2
                ;;
            --keep-worktree)
                keep_worktree=1
                shift
                ;;
            --help)
                usage
                exit 0
                ;;
            *)
                echo "unknown option: $1" >&2
                usage
                exit 1
                ;;
        esac
    done

    if [[ -z "$baseline" ]]; then
        echo "--baseline is required" >&2
        usage
        exit 1
    fi

    local repo_root
    repo_root=$(git rev-parse --show-toplevel)
    BENCH_REPO_ROOT="$repo_root"
    local head_sha base_sha ts
    head_sha=$(git -C "$repo_root" rev-parse --short HEAD)
    base_sha=$(git -C "$repo_root" rev-parse --short "$baseline")
    ts=$(date +%Y%m%d-%H%M%S)

    if [[ -z "$out_dir" ]]; then
        out_dir="/tmp/textual-bench-${base_sha}-vs-${head_sha}-${ts}"
    fi
    mkdir -p "$out_dir"
    mkdir -p "$out_dir/inputs"

    local baseline_wt
    baseline_wt="/tmp/textual-rs-bench-${base_sha}-${ts}"
    git -C "$repo_root" worktree add --detach "$baseline_wt" "$baseline" >/dev/null
    BENCH_BASELINE_WT="$baseline_wt"
    BENCH_KEEP_WORKTREE="$keep_worktree"

    cleanup() {
        if [[ "$BENCH_KEEP_WORKTREE" -eq 0 && -n "$BENCH_BASELINE_WT" && -n "$BENCH_REPO_ROOT" ]]; then
            git -C "$BENCH_REPO_ROOT" worktree remove --force "$BENCH_BASELINE_WT" >/dev/null 2>&1 || true
        fi
    }
    trap cleanup EXIT

    local tabbed_input tick_input
    tabbed_input="$out_dir/inputs/tabbed_content.keys"
    tick_input="$out_dir/inputs/tick.keys"

    # Ctrl+P, down x20, up x20, type "theme", enter, esc, Ctrl+Q.
    {
        printf '\020'
        for _ in $(seq 1 20); do printf '\033[B'; done
        for _ in $(seq 1 20); do printf '\033[A'; done
        printf 'theme'
        printf '\r'
        printf '\033'
        printf '\021'
    } >"$tabbed_input"

    # Sustained typing + backspace pressure, then Ctrl+Q.
    {
        for _ in $(seq 1 600); do printf 'a'; done
        for _ in $(seq 1 600); do printf '\177'; done
        printf '\021'
    } >"$tick_input"

    echo "Output dir: $out_dir"
    echo "Baseline worktree: $baseline_wt"
    echo "Building examples on both revisions..."
    (cd "$repo_root" && cargo build --offline --quiet --manifest-path "$DOC_WIDGETS_MANIFEST" --example tabbed_content --example tick)
    (cd "$baseline_wt" && cargo build --offline --quiet --manifest-path "$DOC_WIDGETS_MANIFEST" --example tabbed_content --example tick)

    run_case() {
        # Args: label repo_path scenario example input_file run_idx
        local label="$1"
        local repo_path="$2"
        local scenario="$3"
        local example="$4"
        local input_file="$5"
        local run_idx="$6"

        local run_dir="$out_dir/raw/$label/$scenario/run${run_idx}"
        mkdir -p "$run_dir"
        local timing_log="$run_dir/timing.log"
        local input_log="$run_dir/input.log"
        local render_log="$run_dir/render.log"
        local transcript="$run_dir/transcript.log"

        (
            {
                sleep 1
                cat "$input_file"
                sleep 0.25
                cat "$input_file"
                sleep 0.25
                cat "$input_file"
            } |
                timeout "${timeout_s}s" script -q -c \
                    "cd '$repo_path' && TEXTUAL_DEBUG_TIMING_FILE='$timing_log' TEXTUAL_DEBUG_INPUT_FILE='$input_log' TEXTUAL_DEBUG_RENDER_FILE='$render_log' cargo run --offline --quiet --manifest-path '$DOC_WIDGETS_MANIFEST' --example '$example'" \
                    "$transcript"
        ) >/dev/null 2>&1 || true

        if [[ ! -s "$timing_log" ]]; then
            echo "warning: missing timing log for $label/$scenario run $run_idx" >&2
        fi
    }

    for label in baseline head; do
        local repo
        if [[ "$label" == "baseline" ]]; then
            repo="$baseline_wt"
        else
            repo="$repo_root"
        fi
        for i in $(seq 1 "$runs"); do
            echo "Running $label tabbed_content ($i/$runs)..."
            run_case "$label" "$repo" "tabbed_content" "tabbed_content" "$tabbed_input" "$i"
            echo "Running $label tick ($i/$runs)..."
            run_case "$label" "$repo" "tick" "tick" "$tick_input" "$i"
        done
    done

    local summary_tsv="$out_dir/summary.tsv"
    echo -e "label\tscenario\tinput_loops\tp50_input_loop_total_us\tp95_input_loop_total_us\tmax_input_loop_total_us\tp50_input_dispatch_us\tp95_input_dispatch_us\tmax_input_dispatch_us\tp50_nonzero_render_sum_us\tp95_nonzero_render_sum_us\tmax_nonzero_render_sum_us\twait_dirty_count\ttick_render_loops" >"$summary_tsv"

    summarize_case() {
        # Args: label scenario
        local label="$1"
        local scenario="$2"
        local agg_dir="$out_dir/agg/$label/$scenario"
        mkdir -p "$agg_dir"
        local total_vals="$agg_dir/total_us.values"
        local input_vals="$agg_dir/input_dispatch_us.values"
        local render_vals="$agg_dir/render_sum_us.values"
        local tick_vals="$agg_dir/tick_render.values"
        : >"$total_vals"
        : >"$input_vals"
        : >"$render_vals"
        : >"$tick_vals"

        local wait_dirty_count=0
        local run
        for run in "$out_dir/raw/$label/$scenario"/run*/timing.log; do
            [[ -f "$run" ]] || continue
            extract_loop_metrics "$run" "$total_vals" "$input_vals" "$render_vals" "$tick_vals"
            local waits
            waits=$(grep -c "wait_for_input .*dirty=true" "$run" || true)
            wait_dirty_count=$((wait_dirty_count + waits))
        done

        local input_loops p50_total p95_total max_total
        local p50_input p95_input max_input
        local p50_render p95_render max_render
        read -r input_loops p50_total p95_total max_total < <(calc_stats "$total_vals" "$agg_dir/total.sorted")
        read -r _ p50_input p95_input max_input < <(calc_stats "$input_vals" "$agg_dir/input.sorted")
        read -r _ p50_render p95_render max_render < <(calc_stats "$render_vals" "$agg_dir/render.sorted")
        local tick_render_loops=0
        if [[ -s "$tick_vals" ]]; then
            tick_render_loops=$(wc -l <"$tick_vals")
        fi

        echo -e "${label}\t${scenario}\t${input_loops}\t${p50_total}\t${p95_total}\t${max_total}\t${p50_input}\t${p95_input}\t${max_input}\t${p50_render}\t${p95_render}\t${max_render}\t${wait_dirty_count}\t${tick_render_loops}" >>"$summary_tsv"
    }

    summarize_case baseline tabbed_content
    summarize_case head tabbed_content
    summarize_case baseline tick
    summarize_case head tick

    local report_md="$out_dir/report.md"
    {
        echo "# Runtime Benchmark Report"
        echo
        echo "- baseline: \`$baseline\` (\`$base_sha\`)"
        echo "- head: \`$head_sha\`"
        echo "- runs per scenario: \`$runs\`"
        echo "- timeout per run: \`${timeout_s}s\`"
        echo
        echo "## Raw Summary"
        echo
        echo "| label | scenario | input loops | p50 input-loop total (us) | p95 input-loop total (us) | max input-loop total (us) | p50 input dispatch (us) | p95 input dispatch (us) | max input dispatch (us) | p50 nonzero render sum (us) | p95 nonzero render sum (us) | max nonzero render sum (us) | wait dirty count | tick render loops |"
        echo "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
        awk -F'\t' 'NR>1 {
          printf("| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |\n",
            $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
        }' "$summary_tsv"
        echo
        echo "## Deltas (head vs baseline)"
        echo
        awk -F'\t' '
          NR==1 {next}
          {
            key=$2
            if ($1=="baseline") {
              b_total[key]=$5+0
              b_input[key]=$8+0
              b_render[key]=$11+0
              b_wait[key]=$13+0
              b_tick[key]=$14+0
            } else if ($1=="head") {
              h_total[key]=$5+0
              h_input[key]=$8+0
              h_render[key]=$11+0
              h_wait[key]=$13+0
              h_tick[key]=$14+0
            }
          }
          END {
            print "| scenario | p95 total delta | p95 input dispatch delta | p95 render sum delta | wait dirty delta | tick render loops delta |"
            print "|---|---:|---:|---:|---:|---:|"
            for (k in b_total) {
              td = pct(h_total[k], b_total[k])
              id = pct(h_input[k], b_input[k])
              rd = pct(h_render[k], b_render[k])
              wd = h_wait[k] - b_wait[k]
              tk = h_tick[k] - b_tick[k]
              printf("| %s | %s | %s | %s | %d | %d |\n", k, td, id, rd, wd, tk)
            }
          }
          function pct(h,b) {
            if (b == 0) return "n/a"
            d = (h-b) / b * 100
            return sprintf("%+.1f%%", d)
          }
        ' "$summary_tsv"
    } >"$report_md"

    echo
    echo "Wrote:"
    echo "  - $summary_tsv"
    echo "  - $report_md"
}

main "$@"
