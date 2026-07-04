//! Launcher for the in-image Smalltalk test suite (st/tests/ui/).
//!
//! The tests themselves are Smalltalk: TestCase subclasses with in-image
//! assertions, run by the in-image TestRunner (st/tests/ui/Harness.st).
//! This side only cross-compiles ONE UI image with the suite layered on
//! top (extra file-in args to build_ui_image.st), runs it on the VM, and
//! checks the machine-readable verdict. On failure the whole transcript —
//! which names each failing case and its expected/actual — is the panic
//! message.

use smallishtalk::vm::{Vm, VmConfig};
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

/// The suite, in file-in (and run) order: harness first, then one file per
/// TestCase class. Adding a file here and to the runAll: list below is all
/// it takes to add a suite.
const SUITE_SOURCES: &[&str] = &[
    "st/tests/ui/Harness.st",
    "st/tests/ui/KernelTests.st",
    "st/tests/ui/GfxTests.st",
    "st/tests/ui/WidgetTests.st",
    "st/tests/ui/WmTests.st",
    "st/tests/ui/ReflectionTests.st",
    "st/tests/ui/BrowserTests.st",
    "st/tests/ui/WorkspaceTests.st",
];

#[test]
fn in_image_test_suite_passes() {
    let dir = std::env::temp_dir().join(format!("smallishtalk-stsuite-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let image = dir.join("suite.im");
    let shot = dir.join("browser-shot.png");

    // The driver: point the screenshot test at a scratch path, then run
    // every suite. The verdict lines come from the in-image TestRunner.
    // More suites than the kernel's Array class with:*5 can hold — build
    // the list with an OrderedCollection instead.
    let driver = format!(
        "| suites |\n\
         TestShotPath := '{}'.\n\
         suites := OrderedCollection new.\n\
         suites add: KernelTests; add: GfxTests; add: WidgetTests; add: WmTests; \
         add: ReflectionTests; add: BrowserTests; add: WorkspaceTests.\n\
         TestRunner runAll: suites.\n",
        shot.display()
    );
    let driver_path = dir.join("suite.driver.st");
    std::fs::write(&driver_path, driver).unwrap();

    let tool = format!("{}/st/tools/build_ui_image.st", root());
    let mut cmd = Command::new("gst");
    cmd.arg("-Q")
        .args(compiler_sources())
        .arg(&tool)
        .arg("-a")
        .arg(&driver_path)
        .arg(&image);
    for src in SUITE_SOURCES {
        cmd.arg(src);
    }
    let out = cmd.current_dir(root()).output().expect("run gst");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("IMAGE-WRITTEN"),
        "suite image build failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let mut vm = Vm::load_image(image.to_str().unwrap(), VmConfig::default()).expect("load");
    let active = vm.active_process;
    vm.run(active).expect("run");
    let transcript = String::from_utf8_lossy(&std::mem::take(&mut vm.stdout_capture)).into_owned();
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        transcript.contains("ALL-TESTS-PASSED"),
        "in-image suite failed:\n{transcript}"
    );
}
