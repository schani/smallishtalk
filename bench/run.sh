#!/usr/bin/env bash
# Benchmark harness (docs/profiling-plan.md §5).
#
# Methodology, enforced here: release build only; one discarded warmup run,
# then the median of $RUNS (default 5) with min and IQR reported and runs
# flagged when the spread exceeds 5%; the same workloads run under GST 3.2.5
# for a ratio column; checksums are cross-validated between hosts; a row per
# workload is appended to bench/history.csv; one extra run per workload
# captures the full VM counter table into bench/results/.
#
# Usage: bench/run.sh [workload ...]   (default: all + self-compile)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BENCH="$ROOT/bench"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
RUNS="${RUNS:-5}"
mkdir -p "$BENCH/results"

COMPILER_SOURCES=(Compat.st Treaty.st Platform.st AST.st Lexer.st Parser.st
    ChunkReader.st CodeGen.st Encoder.st ImageWriter.st Compiler.st)
GST_COMPILER=()
for f in "${COMPILER_SOURCES[@]}"; do GST_COMPILER+=("$ROOT/st/compiler/$f"); done
KERNEL="$ROOT/st/kernel/kernel.st"

echo "== release builds (timed: no gate; counters: --features vm-counters) =="
cargo build --release --manifest-path "$ROOT/Cargo.toml" >/dev/null
VM="$ROOT/target/release/smallishtalk"
cargo build --release --features vm-counters \
    --target-dir "$ROOT/target/counters" --manifest-path "$ROOT/Cargo.toml" >/dev/null
VMC="$ROOT/target/counters/release/smallishtalk"

REV="$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
DATE="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
HISTORY="$BENCH/history.csv"
[ -f "$HISTORY" ] || echo "date,rev,workload,vm_ms_median,vm_ms_min,vm_iqr_pct,gst_ms_median,ratio_vm_over_gst,flags" >"$HISTORY"

# stats <<< "ms per line" -> "median min iqr_pct"
stats() {
    python3 -c '
import sys, statistics as st
xs = sorted(float(l) for l in sys.stdin if l.strip())
med = st.median(xs)
q = st.quantiles(xs, n=4) if len(xs) >= 3 else [xs[0], med, xs[-1]]
iqr = q[2] - q[0]
pct = (100.0 * iqr / med) if med > 0 else 0.0
print(f"{med:.1f} {xs[0]:.1f} {pct:.1f}")'
}

# extract MARKER from output lines "MARKER value"
extract() { awk -v k="$1" '$1 == k { print $2; exit }'; }

# run_times <n> <cmd...>  -> ms per line on stdout; checks CHECK consistency
run_times() {
    local n="$1"; shift
    local check=""
    for _ in $(seq "$n"); do
        local out ms c
        out="$("$@" 2>/dev/null)"
        ms="$(extract MS <<<"$out")"
        c="$(extract CHECK <<<"$out")"
        [ -n "$ms" ] || { echo "no MS line from: $*" >&2; echo "$out" >&2; return 1; }
        if [ -z "$check" ]; then check="$c"; elif [ "$c" != "$check" ]; then
            echo "checksum flapped between runs: $check vs $c" >&2; return 1
        fi
        echo "$ms"
    done
    echo "CHECKVAL $check" >&2
}

bench_one() {
    local name="$1" image="$2" gst_cmd_file="$3"
    local vm_check gst_check

    # VM: warmup, then timed runs.
    "$VM" "$image" >/dev/null
    local vm_ms
    vm_ms="$(run_times "$RUNS" "$VM" "$image" 2>"$TMP/vmcheck")" || return 1
    vm_check="$(extract CHECKVAL <"$TMP/vmcheck")"
    read -r vmed vmin viqr <<<"$(stats <<<"$vm_ms")"

    # GST: warmup, then timed runs.
    local gst_ms gmed
    if [ -n "$gst_cmd_file" ]; then
        gst -Q $4 "$gst_cmd_file" >/dev/null
        gst_ms="$(run_times "$RUNS" gst -Q $4 "$gst_cmd_file" 2>"$TMP/gstcheck")" || return 1
        gst_check="$(extract CHECKVAL <"$TMP/gstcheck")"
        if [ "$vm_check" != "$gst_check" ]; then
            echo "$name: CHECK mismatch (vm=$vm_check gst=$gst_check)" >&2; return 1
        fi
        read -r gmed _ _ <<<"$(stats <<<"$gst_ms")"
    else
        gmed=""
    fi

    local ratio="" flags=""
    if [ -n "$gmed" ]; then ratio="$(python3 -c "print(f'{$vmed / $gmed:.2f}')")"; fi
    if awk -v v="$viqr" 'BEGIN { exit !(v > 5.0) }'; then flags="noisy"; fi

    # Counter capture with the instrumented binary — never a timed run
    # (the gated tier costs ~10% on the dispatch loop, which is exactly
    # why timed runs use the gate-free default build).
    SMALLISHTALK_STATS=1 SMALLISHTALK_GATE=1 "$VMC" "$image" \
        >/dev/null 2>"$BENCH/results/$name.counters.txt" || true

    printf "%-22s vm %8s ms (min %8s, iqr %5s%%)   gst %8s ms   ratio %5s %s\n" \
        "$name" "$vmed" "$vmin" "$viqr" "${gmed:--}" "${ratio:--}" "$flags"
    echo "$DATE,$REV,$name,$vmed,$vmin,$viqr,${gmed:-},${ratio:-},$flags" >>"$HISTORY"
}

WORKLOADS=("$@")
[ ${#WORKLOADS[@]} -gt 0 ] || WORKLOADS=(send_loop arith_loop block_value \
    ordered_collection dictionary string_build exceptions process_pingpong \
    self_compile)

echo "== workloads (median of $RUNS, 1 warmup) =="
for name in "${WORKLOADS[@]}"; do
    if [ "$name" = self_compile ]; then
        # The real workload: the compiler compiles itself in-image (VM) and
        # under GST; CHECK is the output image's byte size (bit-identity!).
        SOURCES=(st/kernel/kernel.st st/selfhost/PlatformImage.st
            st/compiler/Treaty.st st/compiler/AST.st st/compiler/Lexer.st
            st/compiler/Parser.st st/compiler/ChunkReader.st
            st/compiler/CodeGen.st st/compiler/Encoder.st
            st/compiler/ImageWriter.st st/compiler/Compiler.st)
        {
            echo "| b t0 |"
            echo "t0 := Profiler primClockMs."
            echo "b := StImageBuilder new."
            for s in "${SOURCES[@]}"; do echo "b fileInFile: '$ROOT/$s'."; done
            echo "b programSource: 'Transcript showCr: ''gen2'''."
            echo "b writeTo: '$TMP/gen2vm.im'."
            echo "Transcript showCr: 'MS ' , (Profiler primClockMs - t0) printString."
            echo "Transcript showCr: 'CHECK ' , (Platform readFile: '$TMP/gen2vm.im') size printString"
        } >"$TMP/selfcompile_vm.st"
        gst -Q "${GST_COMPILER[@]}" "$ROOT/st/tools/build_selfhost_image.st" -a \
            "$KERNEL" "$TMP/selfcompile_vm.st" "$TMP/selfcompile.im" \
            | grep -q "IMAGE-WRITTEN" || { echo "self-compile image build failed" >&2; exit 1; }
        {
            echo "| b t0 |"
            echo "t0 := Time millisecondClockValue."
            echo "b := StImageBuilder new."
            for s in "${SOURCES[@]}"; do echo "b fileInFile: '$ROOT/$s'."; done
            echo "b programSource: 'Transcript showCr: ''gen2'''."
            echo "b writeTo: '$TMP/gen2gst.im'."
            echo "Transcript showCr: 'MS ', (Time millisecondClockValue - t0) printString."
            echo "Transcript showCr: 'CHECK ', (Platform readFile: '$TMP/gen2gst.im') size printString."
        } >"$TMP/selfcompile_gst.st"
        bench_one self_compile "$TMP/selfcompile.im" "$TMP/selfcompile_gst.st" \
            "${GST_COMPILER[*]}"
    else
        wl="$BENCH/workloads/$name.st"
        [ -f "$wl" ] || { echo "unknown workload $name" >&2; exit 1; }
        image="$TMP/$name.im"
        gst -Q "${GST_COMPILER[@]}" "$ROOT/st/tools/build_bench_image.st" -a \
            "$KERNEL" "$wl" "$BENCH/vm_driver.st" "$image" \
            | grep -q "IMAGE-WRITTEN" || { echo "$name image build failed" >&2; exit 1; }
        bench_one "$name" "$image" "$BENCH/gst_driver.st" "$wl"
    fi
done

echo "history appended to $HISTORY; counter tables in $BENCH/results/"
