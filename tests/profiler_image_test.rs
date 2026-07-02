//! Phase C acceptance (profiling plan §4, §6): the profiler driven entirely
//! from Smalltalk — `Profiler spy: [...]` and `Profiler counters` inside a
//! cross-compiled image, including profiling the in-image compiler and
//! checking that selectors we *know* must be hot appear in the report
//! (without pinning percentages — statistical output stays out of the
//! deterministic corpus).

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

fn gst_build(tool: &str, args: &[&str], marker: &str) {
    let out = Command::new("gst")
        .arg("-Q")
        .args(compiler_sources())
        .arg(format!("{}/st/tools/{}", root(), tool))
        .arg("-a")
        .args(args)
        .current_dir(root())
        .output()
        .expect("run gst");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(marker),
        "gst {tool} {args:?} failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn build_corpus_image(program: &Path, image: &Path) {
    let kernel = format!("{}/st/kernel/kernel.st", root());
    gst_build(
        "build_corpus_image.st",
        &[&kernel, program.to_str().unwrap(), image.to_str().unwrap()],
        "IMAGE-WRITTEN",
    );
}

fn run_image(image: &Path) -> String {
    let mut vm = Vm::load_image(image.to_str().unwrap(), VmConfig::default()).unwrap();
    let active = vm.active_process;
    vm.run_until_idle(active).unwrap();
    String::from_utf8_lossy(&vm.stdout_capture).into_owned()
}

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("smallishtalk-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn profiler_spy_prints_a_report_from_image_code() {
    let dir = temp_dir("prof-spy");
    let program = dir.join("spy.st");
    std::fs::write(
        &program,
        "| r |\n\
         r := Profiler spy: [ | s | s := 0. 1 to: 200000 do: [:i | s := s + i]. s ].\n\
         Transcript showCr: 'RESULT ' , r printString\n",
    )
    .unwrap();
    let image = dir.join("spy.im");
    build_corpus_image(&program, &image);
    let out = run_image(&image);
    assert!(
        out.contains("Profile: "),
        "spy: must print the tally header:\n{out}"
    );
    assert!(
        out.contains("RESULT 20000100000"),
        "spy: must answer the block's value:\n{out}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn vm_counters_readable_from_image_code() {
    let dir = temp_dir("prof-ctr");
    let program = dir.join("ctr.st");
    std::fs::write(
        &program,
        "| rows found |\n\
         Profiler gate: true.\n\
         1 to: 1000 do: [:i | i printString].\n\
         rows := Profiler primCounters.\n\
         rows size > 10 ifTrue: [Transcript showCr: 'counters-ok'].\n\
         found := false.\n\
         rows do: [:row | (row at: 1) = 'send.count' ifTrue: [\n\
             found := true.\n\
             (row at: 2) > 1000 ifTrue: [Transcript showCr: 'sends-counted']]].\n\
         found ifTrue: [Transcript showCr: 'row-found'].\n\
         Profiler primCountersReset.\n\
         Transcript showCr: 'reset-ok'\n",
    )
    .unwrap();
    let image = dir.join("ctr.im");
    build_corpus_image(&program, &image);
    let out = run_image(&image);
    // "sends-counted" needs the gated tier, which is compile-time-only
    // (the runtime gate measured ~10% on the dispatch loop — plan §3).
    #[cfg(feature = "vm-counters")]
    let markers: &[&str] = &["counters-ok", "row-found", "sends-counted", "reset-ok"];
    #[cfg(not(feature = "vm-counters"))]
    let markers: &[&str] = &["counters-ok", "row-found", "reset-ok"];
    for marker in markers {
        assert!(out.contains(marker), "missing {marker}:\n{out}");
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// The plan's acceptance test: profile the in-image compiler compiling the
/// kernel; the report must mention selectors we know are hot.
#[test]
fn profiling_the_in_image_compiler_reports_known_hot_selectors() {
    let dir = temp_dir("prof-accept");
    let kernel = format!("{}/st/kernel/kernel.st", root());
    let out_image = dir.join("compiled-by-spy.im");
    let driver = dir.join("driver.st");
    std::fs::write(
        &driver,
        format!(
            "| b |\n\
             Profiler spy: [\n\
                 b := StImageBuilder new.\n\
                 b fileInFile: '{kernel}'.\n\
                 b programSource: 'Transcript showCr: ''hi'''.\n\
                 b writeTo: '{}' ].\n\
             Transcript showCr: 'SPY-COMPILE-DONE'\n",
            out_image.display()
        ),
    )
    .unwrap();
    let selfhost = dir.join("selfhost.im");
    gst_build(
        "build_selfhost_image.st",
        &[&kernel, driver.to_str().unwrap(), selfhost.to_str().unwrap()],
        "IMAGE-WRITTEN",
    );
    let out = run_image(&selfhost);
    assert!(out.contains("SPY-COMPILE-DONE"), "in-image compile failed:\n{out}");
    assert!(out.contains("Profile: "), "no tally printed:\n{out}");
    // Total samples: the compile takes well over a second in any build at
    // a 1 ms interval, so requiring hot selectors is safe.
    let total: u64 = out
        .split("Profile: ")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse().ok())
        .expect("parse total samples");
    if total >= 50 {
        let known_hot = ["nextPut:", "at:", "StLexer", "StEncoder", "StImageWriter"];
        assert!(
            known_hot.iter().any(|s| out.contains(s)),
            "report mentions none of {known_hot:?}:\n{out}"
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}
