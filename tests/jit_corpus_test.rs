//! M4: the Phase 6 differential modes (SPEC §20, JIT.md §18.3). Every
//! corpus program runs with the in-image JIT aboard — background JIT
//! process, tiering counters, organic compilation — in JIT-after-N and
//! JIT-always flavors, byte-diffed against the same .expected outputs as
//! the interpreter mode. Plus the two mode products: JIT x GC-stress
//! (collections constantly relocating stacks under compiled code) and
//! JIT x snapshot (tier-free images, organic re-tiering after reload).

use smallishtalk::heap::HeapConfig;
use smallishtalk::vm::{Vm, VmConfig};
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> &'static str {
    env!("CARGO_MANIFEST_DIR")
}

fn compiler_sources() -> Vec<String> {
    [
        "Compat.st",
        "Treaty.st",
        "Platform.st",
        "AST.st",
        "Lexer.st",
        "Parser.st",
        "ChunkReader.st",
        "CodeGen.st",
        "Encoder.st",
        "ImageWriter.st",
        "Compiler.st",
    ]
    .iter()
    .map(|f| format!("{}/st/compiler/{}", root(), f))
    .collect()
}

fn build_jit_image(program: &Path, image: &Path, threshold: u32) {
    std::fs::create_dir_all(image.parent().unwrap()).unwrap();
    let out = Command::new("gst")
        .arg("-Q")
        .args(compiler_sources())
        .arg(format!("{}/st/tools/build_jit_corpus_image.st", root()))
        .arg("-a")
        .arg(format!("{}/st/kernel/kernel.st", root()))
        .arg(program)
        .arg(image)
        .arg(threshold.to_string())
        .current_dir(root())
        .output()
        .expect("run gst");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("IMAGE-WRITTEN"),
        "JIT corpus image build of {} failed:\n{}\n{}",
        program.display(),
        stdout,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn corpus_entries() -> Vec<(PathBuf, PathBuf)> {
    let dir = PathBuf::from(root()).join("corpus");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus/ exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "st"))
        .collect();
    entries.sort();
    entries
        .into_iter()
        .map(|st| {
            let expected = st.with_extension("expected");
            (st, expected)
        })
        .collect()
}

fn check_jit_corpus(threshold: u32, config_for: impl Fn() -> VmConfig, mode: &str) -> u64 {
    let entries = corpus_entries();
    assert!(!entries.is_empty());
    let dir = std::env::temp_dir().join(format!(
        "smallishtalk-jitcorpus-{}-{}",
        mode,
        std::process::id()
    ));
    let mut failures = Vec::new();
    let mut total_installs = 0u64;
    for (st, expected_path) in &entries {
        let name = st.file_stem().unwrap().to_string_lossy().into_owned();
        let image = dir.join(format!("{name}.im"));
        build_jit_image(st, &image, threshold);
        let expected = std::fs::read(expected_path).unwrap();
        let mut vm = match Vm::load_image(image.to_str().unwrap(), config_for()) {
            Ok(vm) => vm,
            Err(e) => {
                failures.push(format!("{name} [{mode}]: load: {e:?}"));
                continue;
            }
        };
        let active = vm.active_process;
        match vm.run(active) {
            Ok(_) => {
                if vm.stdout_capture != expected {
                    failures.push(format!(
                        "{name} [{mode}]:\n  expected: {:?}\n  actual:   {:?}",
                        String::from_utf8_lossy(&expected),
                        String::from_utf8_lossy(&vm.stdout_capture)
                    ));
                }
                total_installs += vm.counters.jit_installs;
            }
            Err(e) => failures.push(format!("{name} [{mode}]: VM error: {e:?}")),
        }
    }
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        failures.is_empty(),
        "JIT corpus failures:\n{}",
        failures.join("\n")
    );
    total_installs
}

#[test]
fn corpus_jit_always() {
    // Threshold 1: everything that runs twice compiles; the JIT process
    // and the compiler tier themselves up along the way (JIT.md §3).
    let installs = check_jit_corpus(1, VmConfig::default, "always");
    assert!(installs > 100, "expected heavy tiering, got {installs} installs");
}

#[test]
fn corpus_jit_after_n() {
    check_jit_corpus(25, VmConfig::default, "after-n");
}

#[test]
fn corpus_jit_gc_stress() {
    // J1/J2's trial by fire: collections constantly relocate stacks and
    // literals under compiled code.
    let installs = check_jit_corpus(
        1,
        || VmConfig {
            heap: HeapConfig {
                young_bytes: 64 * 1024,
                old_bytes: 64 * 1024 * 1024,
                ..HeapConfig::default()
            },
            ..VmConfig::default()
        },
        "gc-stress",
    );
    assert!(installs > 0);
}

#[test]
fn corpus_jit_snapshot_roundtrip() {
    // JIT x snapshot: snapshot mid-corpus under JIT-always, reload in a
    // fresh VM (vmState reset by the loader — images are tier-free, J7),
    // complete, and diff. Re-tiering after the reload is organic.
    let entries = corpus_entries();
    let dir = std::env::temp_dir().join(format!("smallishtalk-jitsnap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut failures = Vec::new();
    let mut resumed_installs = 0u64;
    for (st, expected_path) in &entries {
        let name = st.file_stem().unwrap().to_string_lossy().into_owned();
        let image = dir.join(format!("{name}.im"));
        build_jit_image(st, &image, 1);
        let expected = std::fs::read(expected_path).unwrap();

        let snap_path = dir.join(format!("{name}.snap.im"));
        let mut vm = Vm::load_image(image.to_str().unwrap(), VmConfig::default()).unwrap();
        vm.snapshot_after_sends = Some((200, snap_path.to_string_lossy().into_owned()));
        let active = vm.active_process;
        if let Err(e) = vm.run_until_idle(active) {
            failures.push(format!("{name}: interrupted run failed: {e:?}"));
            continue;
        }
        if vm.stdout_capture != expected {
            failures.push(format!("{name}: interrupted run output diverged"));
            continue;
        }
        let Some(prefix_len) = vm.snapshot_fired_at_capture_len else {
            continue;
        };

        let mut vm2 = Vm::load_image(snap_path.to_str().unwrap(), VmConfig::default()).unwrap();
        let active2 = vm2.active_process;
        if let Err(e) = vm2.run_until_idle(active2) {
            failures.push(format!("{name}: resumed run failed: {e:?}"));
            continue;
        }
        resumed_installs += vm2.counters.jit_installs;
        let mut combined = expected[..prefix_len].to_vec();
        combined.extend_from_slice(&vm2.stdout_capture);
        if combined != expected {
            failures.push(format!(
                "{name}: JIT snapshot round-trip diverged\n  resumed: {:?} (prefix {prefix_len})",
                String::from_utf8_lossy(&vm2.stdout_capture)
            ));
        }
    }
    std::fs::remove_dir_all(&dir).ok();
    assert!(failures.is_empty(), "JIT snapshot failures:\n{}", failures.join("\n"));
    // At least one resumed run must have re-tiered organically.
    assert!(resumed_installs > 0, "no organic re-tiering after reload");
}

#[test]
fn jit_sampler_tier_residency() {
    // Deterministic sampling (every poll) over a loop-heavy program in
    // JIT-always mode: samples must land overwhelmingly in native code
    // (JIT.md M5 — the tier-residency meter), and every native-side
    // sample must symbolize through the ordinary frame walk.
    let dir = std::env::temp_dir().join(format!("smallishtalk-jitres-{}", std::process::id()));
    let program = PathBuf::from(root()).join("corpus/021_inline_loops.st");
    let image = dir.join("residency.im");
    build_jit_image(&program, &image, 1);
    let mut vm = Vm::load_image(image.to_str().unwrap(), VmConfig::default()).unwrap();
    vm.profiler.active = true;
    vm.profiler.sample_every_poll = true;
    // Compiled polls fire only when the flag is armed; stress mode keeps
    // re-arming it from the service routine.
    vm.safepoint
        .armed
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let active = vm.active_process;
    vm.run(active).expect("run");
    std::fs::remove_dir_all(&dir).ok();
    let native = vm.counters.jit_samples_native;
    let interp = vm.counters.jit_samples_interp;
    assert!(native > 0, "no native-tier samples");
    assert!(
        native * 10 > (native + interp) * 5,
        "expected mostly-native residency under JIT-always: native {native} interp {interp}"
    );
    assert!(
        !vm.profiler.names().iter().any(|n| n.contains("<invalid-frame>")),
        "unsymbolizable frame sampled from native code"
    );
}
