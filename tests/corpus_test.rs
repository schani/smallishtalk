//! Phase 3: the handshake (SPEC §20). corpus/*.st programs are
//! cross-compiled to images by the GST-side compiler and executed by this
//! VM; captured stdout is diffed against the .expected files. The corpus
//! is append-only and is the permanent regression suite.
//!
//! Phase 4 modes: the same corpus also runs under GC stress (64KB young
//! space, aggressive tenuring) and — for programs that opt in — through a
//! snapshot/reload round-trip.

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

/// Cross-compile kernel+program to an image via GST. Panics with the GST
/// transcript on failure.
pub fn build_image(program_path: &Path, image_path: &Path) {
    std::fs::create_dir_all(image_path.parent().unwrap()).unwrap();
    let kernel = format!("{}/st/kernel/kernel.st", root());
    let tool = format!("{}/st/tools/build_corpus_image.st", root());
    let out = Command::new("gst")
        .arg("-Q")
        .args(compiler_sources())
        .arg(&tool)
        .arg("-a")
        .arg(&kernel)
        .arg(program_path)
        .arg(image_path)
        .output()
        .expect("run gst");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("IMAGE-WRITTEN"),
        "cross-compile of {} failed:\n{}\n{}",
        program_path.display(),
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
            assert!(
                expected.exists(),
                "missing {} for {}",
                expected.display(),
                st.display()
            );
            (st, expected)
        })
        .collect()
}

fn run_image(image: &Path, config: VmConfig) -> Result<Vec<u8>, String> {
    let mut vm = Vm::load_image(image.to_str().unwrap(), config)
        .map_err(|e| format!("load: {e:?}"))?;
    let active = vm.active_process;
    vm.run(active).map_err(|e| format!("run: {e:?}"))?;
    Ok(vm.stdout_capture)
}

fn check_corpus(config_for: impl Fn() -> VmConfig, mode: &str) {
    let entries = corpus_entries();
    assert!(!entries.is_empty(), "corpus must not be empty");
    let dir = std::env::temp_dir().join(format!(
        "smallishtalk-corpus-{}-{}",
        mode,
        std::process::id()
    ));
    let mut failures = Vec::new();
    for (st, expected_path) in &entries {
        let name = st.file_stem().unwrap().to_string_lossy().into_owned();
        let image = dir.join(format!("{name}.im"));
        build_image(st, &image);
        let expected = std::fs::read(expected_path).unwrap();
        match run_image(&image, config_for()) {
            Ok(output) => {
                if output != expected {
                    failures.push(format!(
                        "{name} [{mode}]:\n  expected: {:?}\n  actual:   {:?}",
                        String::from_utf8_lossy(&expected),
                        String::from_utf8_lossy(&output)
                    ));
                }
            }
            Err(e) => failures.push(format!("{name} [{mode}]: VM error: {e}")),
        }
    }
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        failures.is_empty(),
        "corpus failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn corpus_interpreter() {
    check_corpus(VmConfig::default, "plain");
}

#[test]
fn corpus_gc_stress() {
    // SPEC §20 phase 4: the whole corpus under a tiny young space with
    // aggressive tenuring, so collection happens constantly.
    check_corpus(
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
}

#[test]
fn corpus_snapshot_roundtrip() {
    // SPEC §20 phase 4 snapshot mode: run each program to a marker (the
    // Nth send), snapshot, reload in a fresh VM, run to completion; the
    // concatenated output must equal the uninterrupted run's.
    let entries = corpus_entries();
    let dir = std::env::temp_dir().join(format!("smallishtalk-snap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut failures = Vec::new();
    for (st, expected_path) in &entries {
        let name = st.file_stem().unwrap().to_string_lossy().into_owned();
        let image = dir.join(format!("{name}.im"));
        build_image(st, &image);
        let expected = std::fs::read(expected_path).unwrap();

        // Interrupted run: snapshot at the 200th send, keep running.
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
            // Program finished in fewer than 200 sends — nothing to resume.
            continue;
        };

        // Resume from the snapshot in a fresh VM.
        let mut vm2 = Vm::load_image(snap_path.to_str().unwrap(), VmConfig::default()).unwrap();
        let active2 = vm2.active_process;
        if let Err(e) = vm2.run_until_idle(active2) {
            failures.push(format!("{name}: resumed run failed: {e:?}"));
            continue;
        }
        let mut combined = expected[..prefix_len].to_vec();
        combined.extend_from_slice(&vm2.stdout_capture);
        if combined != expected {
            failures.push(format!(
                "{name}: snapshot round-trip diverged\n  expected: {:?}\n  resumed:  {:?} (prefix {prefix_len})",
                String::from_utf8_lossy(&expected),
                String::from_utf8_lossy(&vm2.stdout_capture)
            ));
        }
    }
    std::fs::remove_dir_all(&dir).ok();
    assert!(failures.is_empty(), "snapshot failures:\n{}", failures.join("\n"));
}
