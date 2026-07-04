//! The full bootstrap fixpoint (SPEC §20 Phase 5, taken to closure):
//!
//!   S0 = the compiler compiled by GST
//!   S1 = the compiler compiled by S0
//!   S2 = the compiler compiled by S1
//!
//! and all three images are **bit-identical**. The trick that makes byte
//! equality across generations possible is a self-replicating driver: the
//! image's program compiles kernel + compiler + its own source and writes
//! the result to one fixed path, so every generation is built from exactly
//! the same bytes. S0==S1 proves GST and the in-image compiler agree; S1==S2
//! proves the in-image compiler is a true fixpoint of itself.

use smallishtalk::vm::{Vm, VmConfig};
use std::path::Path;
use std::process::Command;

fn root() -> &'static str {
    env!("CARGO_MANIFEST_DIR")
}

fn compiler_sources() -> Vec<String> {
    [
        "Compat.st", "Treaty.st", "Platform.st", "AST.st", "Lexer.st", "Parser.st",
        "ChunkReader.st", "CodeGen.st", "Encoder.st", "ImageWriter.st", "Compiler.st",
    ]
    .iter()
    .map(|f| format!("{}/st/compiler/{}", root(), f))
    .collect()
}

/// The compiler sources as filed into a self-host image, in the same order
/// as st/tools/build_selfhost_image.st.
const SELFHOST_SOURCES: &[&str] = &[
    "st/selfhost/PlatformImage.st",
    "st/compiler/Treaty.st",
    "st/compiler/AST.st",
    "st/compiler/Lexer.st",
    "st/compiler/Parser.st",
    "st/compiler/ChunkReader.st",
    "st/compiler/CodeGen.st",
    "st/compiler/Encoder.st",
    "st/compiler/ImageWriter.st",
    "st/compiler/Compiler.st",
];

/// Run `image` on the VM; panic unless its transcript contains `marker`.
fn run_stage(image: &Path, marker: &str) {
    let mut vm = Vm::load_image(image.to_str().unwrap(), VmConfig::default()).expect("load stage");
    let active = vm.active_process;
    vm.run_until_idle(active).expect("run stage");
    let out = String::from_utf8_lossy(&vm.stdout_capture).into_owned();
    assert!(
        out.contains(marker),
        "stage {} did not complete:\n{out}",
        image.display()
    );
}

#[test]
fn bootstrap_fixpoint_s0_s1_s2_are_bit_identical() {
    let dir = std::env::temp_dir().join(format!("smallishtalk-boot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let driver_path = dir.join("driver.st");
    let next = dir.join("next.im");

    // The self-replicating driver: rebuild kernel + compiler + THIS driver
    // to the fixed path `next.im`. Identical bytes in every generation.
    let mut driver = String::from("| b |\nb := StImageBuilder new.\n");
    driver.push_str(&format!("b fileInFile: '{}/st/kernel/kernel.st'.\n", root()));
    for f in SELFHOST_SOURCES {
        driver.push_str(&format!("b fileInFile: '{}/{f}'.\n", root()));
    }
    driver.push_str(&format!(
        "b programSource: (Platform readFile: '{}').\n\
         b writeTo: '{}'.\n\
         Transcript showCr: 'BOOTSTRAP-STAGE-DONE'\n",
        driver_path.display(),
        next.display()
    ));
    std::fs::write(&driver_path, driver).unwrap();

    // S0: GST cross-compiles the compiler.
    let s0 = dir.join("s0.im");
    let kernel = format!("{}/st/kernel/kernel.st", root());
    let out = Command::new("gst")
        .arg("-Q")
        .args(compiler_sources())
        .arg(format!("{}/st/tools/build_selfhost_image.st", root()))
        .arg("-a")
        .args([&kernel, &driver_path.display().to_string(), &s0.display().to_string()])
        .current_dir(root())
        .output()
        .expect("run gst");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("SELFHOST-IMAGE-WRITTEN"),
        "S0 build failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // S1: the compiler compiled by S0.
    run_stage(&s0, "BOOTSTRAP-STAGE-DONE");
    let s1 = dir.join("s1.im");
    std::fs::rename(&next, &s1).expect("S1 written");

    // S2: the compiler compiled by S1.
    run_stage(&s1, "BOOTSTRAP-STAGE-DONE");
    let s2 = dir.join("s2.im");
    std::fs::rename(&next, &s2).expect("S2 written");

    let b0 = std::fs::read(&s0).unwrap();
    let b1 = std::fs::read(&s1).unwrap();
    let b2 = std::fs::read(&s2).unwrap();
    assert_eq!(b0, b1, "S1 (compiled by S0) differs from S0 (compiled by GST)");
    assert_eq!(b1, b2, "S2 (compiled by S1) differs from S1 (compiled by S0)");
    std::fs::remove_dir_all(&dir).ok();
}
