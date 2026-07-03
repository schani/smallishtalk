//! M2: the in-image JIT differential selftest (JIT.md §18.2). The image
//! carries the whole Smalltalk-side JIT (assembler, macro assembler,
//! method compiler); the selftest runs every case interpreted, compiles
//! the subject class through primJITInstall, re-runs, and diffs. The
//! GC-stress product (64 KB young space) makes collections constantly
//! relocate stacks under compiled code (J1/J2's trial by fire).

use smallishtalk::heap::HeapConfig;
use smallishtalk::vm::{Vm, VmConfig};
use std::path::PathBuf;
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

fn build_selftest_image(image_path: &PathBuf, tag: &str) {
    std::fs::create_dir_all(image_path.parent().unwrap()).unwrap();
    let driver = PathBuf::from(root()).join(format!("target/tmp/jit_selftest_driver_{tag}.st"));
    std::fs::write(&driver, "JitSelftest run\n").unwrap();
    let out = Command::new("gst")
        .arg("-Q")
        .args(compiler_sources())
        .arg(format!("{}/st/tools/build_jit_image.st", root()))
        .arg("-a")
        .arg(format!("{}/st/kernel/kernel.st", root()))
        .arg(&driver)
        .arg(image_path)
        .current_dir(root())
        .output()
        .expect("run gst");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("IMAGE-WRITTEN"),
        "JIT image build failed:\n{}\n{}",
        stdout,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn run_selftest(config: VmConfig, tag: &str) -> Vm {
    let image = PathBuf::from(root()).join(format!("target/tmp/jit_selftest_{tag}.im"));
    build_selftest_image(&image, tag);
    let mut vm = Vm::load_image(image.to_str().unwrap(), config).expect("load image");
    let active = vm.active_process;
    vm.run_until_idle(active).expect("run");
    let out = String::from_utf8_lossy(&vm.stdout_capture).into_owned();
    assert!(
        out.contains("JIT-SELFTEST-PASSED"),
        "selftest failed:\n{out}"
    );
    vm
}

#[test]
fn jit_selftest_differential() {
    let vm = run_selftest(VmConfig::default(), "plain");
    assert!(vm.counters.jit_installs >= 10, "methods must actually install");
    assert!(vm.counters.jit_enters > 0, "native code must actually run");
}

#[test]
fn jit_selftest_under_gc_stress() {
    let vm = run_selftest(
        VmConfig {
            heap: HeapConfig {
                young_bytes: 64 * 1024,
                old_bytes: 64 * 1024 * 1024,
                ..HeapConfig::default()
            },
            max_stack_bytes: 16 * 1024 * 1024,
        },
        "gcstress",
    );
    assert!(vm.counters.jit_enters > 0);
    assert!(vm.scavenge_count > 0, "stress mode must actually collect");
}
