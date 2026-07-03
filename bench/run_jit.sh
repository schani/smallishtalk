#!/usr/bin/env bash
# JIT benchmark harness (JIT.md §18.8 / M6): the same workloads as
# bench/run.sh, interpreter vs JIT-warmed, release build, median of $RUNS
# with one discarded warmup invocation each. Checksums must agree between
# tiers (the differential rule applied to benchmarks).
#
# Also measures compile speed: methods compiled per ms by the in-image
# compiler once it is itself hot (the M6 target: median method < 1 ms).
#
# Usage: bench/run_jit.sh [workload ...]   (default: all)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BENCH="$ROOT/bench"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
RUNS="${RUNS:-5}"

COMPILER_SOURCES=(Compat.st Treaty.st Platform.st AST.st Lexer.st Parser.st
    ChunkReader.st CodeGen.st Encoder.st ImageWriter.st Compiler.st)
GST_COMPILER=()
for f in "${COMPILER_SOURCES[@]}"; do GST_COMPILER+=("$ROOT/st/compiler/$f"); done
KERNEL="$ROOT/st/kernel/kernel.st"

echo "== release build =="
cargo build --release --manifest-path "$ROOT/Cargo.toml" >/dev/null
VM="$ROOT/target/release/smallishtalk"

stats() {
    python3 -c '
import sys, statistics as st
xs = sorted(float(l) for l in sys.stdin if l.strip())
print(f"{st.median(xs):.1f}")'
}

extract() { awk -v k="$1" '$1 == k { print $2; exit }'; }

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
            echo "checksum flapped: $check vs $c" >&2; return 1
        fi
        echo "$ms"
    done
    echo "CHECKVAL $check" >&2
}

WORKLOADS=("$@")
if [ ${#WORKLOADS[@]} -eq 0 ]; then
    WORKLOADS=()
    for w in "$BENCH"/workloads/*.st; do WORKLOADS+=("$(basename "$w" .st)"); done
fi

echo "== compile speed (µs per method, compiler hot) =="
gst -Q "${GST_COMPILER[@]}" "$ROOT/st/tools/build_jit_bench_image.st" -a \
    "$KERNEL" "$BENCH/workloads/arith_loop.st" "$BENCH/vm_compile_driver.st" \
    "$TMP/compilespeed.im" >/dev/null
"$VM" "$TMP/compilespeed.im" | awk '$1=="USPERMETHOD" {print "  " $2 " µs/method"}'

gen_selfcompile_driver() {
    # $1 = output path, $2 = "jit" or "interp". Both variants run the
    # self-compile twice and time the second pass, so the comparison is
    # warm-vs-warm; the JIT variant additionally tiers the compiler up
    # during the first pass (JIT.md §3: the compiler compiles itself).
    local out="$1" mode="$2"
    local SOURCES=(st/kernel/kernel.st st/selfhost/PlatformImage.st
        st/compiler/Treaty.st st/compiler/AST.st st/compiler/Lexer.st
        st/compiler/Parser.st st/compiler/ChunkReader.st
        st/compiler/CodeGen.st st/compiler/Encoder.st
        st/compiler/ImageWriter.st st/compiler/Compiler.st)
    {
        echo "| b t0 |"
        if [ "$mode" = jit ]; then
            echo "StJIT threshold: 2."
            echo "StJIT startBackgroundCompiler."
        fi
        echo "b := StImageBuilder new."
        for s in "${SOURCES[@]}"; do echo "b fileInFile: '$ROOT/$s'."; done
        echo "b programSource: 'Transcript showCr: ''gen2'''."
        echo "b writeTo: '$TMP/gen2warm.$mode.im'."
        if [ "$mode" = jit ]; then echo "StJITCompiler drain."; fi
        echo "t0 := Profiler primClockMs."
        echo "b := StImageBuilder new."
        for s in "${SOURCES[@]}"; do echo "b fileInFile: '$ROOT/$s'."; done
        echo "b programSource: 'Transcript showCr: ''gen2'''."
        echo "b writeTo: '$TMP/gen2.$mode.im'."
        echo "Transcript showCr: 'MS ' , (Profiler primClockMs - t0) printString."
        echo "Transcript showCr: 'CHECK ' , (Platform readFile: '$TMP/gen2.$mode.im') size printString"
    } >"$out"
}

echo "== interpreter vs JIT (median of $RUNS, warmed) =="
for name in "${WORKLOADS[@]}"; do
    if [ "$name" = self_compile ]; then
        # The real workload: the compiler compiles itself in-image.
        # CHECK is the gen2 image's byte size — the two tiers must
        # produce bit-identical output (§18 determinism, applied).
        gen_selfcompile_driver "$TMP/sc_interp.st" interp
        gen_selfcompile_driver "$TMP/sc_jit.st" jit
        gst -Q "${GST_COMPILER[@]}" "$ROOT/st/tools/build_selfhost_image.st" -a \
            "$KERNEL" "$TMP/sc_interp.st" "$TMP/sc_interp.im" >/dev/null
        gst -Q "${GST_COMPILER[@]}" "$ROOT/st/tools/build_jit_selfhost_image.st" -a \
            "$KERNEL" "$TMP/sc_jit.st" "$TMP/sc_jit.im" >/dev/null

        "$VM" "$TMP/sc_interp.im" >/dev/null
        interp_ms="$(run_times "$RUNS" "$VM" "$TMP/sc_interp.im" 2>"$TMP/ic")"
        icheck="$(extract CHECKVAL <"$TMP/ic")"
        imed="$(stats <<<"$interp_ms")"

        "$VM" "$TMP/sc_jit.im" >/dev/null
        jit_ms="$(run_times "$RUNS" "$VM" "$TMP/sc_jit.im" 2>"$TMP/jc")"
        jcheck="$(extract CHECKVAL <"$TMP/jc")"
        jmed="$(stats <<<"$jit_ms")"

        if [ "$icheck" != "$jcheck" ]; then
            echo "self_compile: gen2 image size mismatch (interp=$icheck jit=$jcheck)" >&2
            exit 1
        fi
        speedup="$(python3 -c "print(f'{$imed / max($jmed, 0.001):.2f}')")"
        printf "%-22s interp %8s ms   jit %8s ms   speedup %6sx\n" \
            self_compile "$imed" "$jmed" "$speedup"
        continue
    fi
    w="$BENCH/workloads/$name.st"
    [ -f "$w" ] || { echo "no workload $name" >&2; continue; }

    gst -Q "${GST_COMPILER[@]}" "$ROOT/st/tools/build_bench_image.st" -a \
        "$KERNEL" "$w" "$BENCH/vm_driver.st" "$TMP/$name.im" >/dev/null
    gst -Q "${GST_COMPILER[@]}" "$ROOT/st/tools/build_jit_bench_image.st" -a \
        "$KERNEL" "$w" "$BENCH/vm_jit_driver.st" "$TMP/$name.jit.im" >/dev/null

    "$VM" "$TMP/$name.im" >/dev/null
    interp_ms="$(run_times "$RUNS" "$VM" "$TMP/$name.im" 2>"$TMP/ic")"
    icheck="$(extract CHECKVAL <"$TMP/ic")"
    imed="$(stats <<<"$interp_ms")"

    "$VM" "$TMP/$name.jit.im" >/dev/null
    jit_ms="$(run_times "$RUNS" "$VM" "$TMP/$name.jit.im" 2>"$TMP/jc")"
    jcheck="$(extract CHECKVAL <"$TMP/jc")"
    jmed="$(stats <<<"$jit_ms")"

    if [ "$icheck" != "$jcheck" ]; then
        echo "$name: CHECK mismatch between tiers (interp=$icheck jit=$jcheck)" >&2
        exit 1
    fi
    speedup="$(python3 -c "print(f'{$imed / max($jmed, 0.001):.2f}')")"
    printf "%-22s interp %8s ms   jit %8s ms   speedup %6sx\n" \
        "$name" "$imed" "$jmed" "$speedup"
done
