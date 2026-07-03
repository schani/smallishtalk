//! M1 cross-check (JIT.md §18.1): the Smalltalk assembler's golden blobs
//! are re-decoded by an independent disassembler (binutils objdump) and
//! the mnemonic sequences diffed — catching "self-consistent but wrong"
//! encodings, the classic assembler failure. Instruction *lengths* are
//! implicitly checked too: a length error desynchronizes the stream and
//! the mnemonics stop matching.

use std::path::PathBuf;
use std::process::Command;

fn root() -> &'static str {
    env!("CARGO_MANIFEST_DIR")
}

fn have(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Generate the golden file via GST (the same catalog the SUnit round-trip
/// tests consume).
fn generate_goldens(path: &PathBuf) {
    let sources = [
        "st/compiler/Compat.st",
        "st/compiler/Platform.st",
        "st/jit/AMD64Assembler.st",
        "st/jit/AMD64Goldens.st",
    ];
    let out = Command::new("gst")
        .arg("-Q")
        .args(sources.iter().map(|s| format!("{}/{}", root(), s)))
        .arg(format!("{}/st/tools/gen_amd64_goldens.st", root()))
        .arg("-a")
        .arg(path)
        .current_dir(root())
        .output()
        .expect("run gst");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("GOLDENS-WRITTEN"),
        "golden generation failed:\n{}\n{}",
        stdout,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn objdump_mnemonics(bytes: &[u8], tmp: &PathBuf) -> Vec<String> {
    std::fs::write(tmp, bytes).unwrap();
    let out = Command::new("objdump")
        .args(["-D", "-b", "binary", "-m", "i386:x86-64", "-M", "intel"])
        .arg(tmp)
        .output()
        .expect("run objdump");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    let mut mnems = Vec::new();
    for line in text.lines() {
        // "   0:\t48 89 d8    \tmov    rax,rbx"
        let Some((_, rest)) = line.split_once(":\t") else {
            continue;
        };
        let Some((_, insn)) = rest.split_once('\t') else {
            continue; // continuation line of a long instruction
        };
        let mnem = insn.split_whitespace().next().unwrap_or("").to_string();
        assert!(
            mnem != "(bad)",
            "objdump rejects encoding: {line}\nfull:\n{text}"
        );
        mnems.push(mnem);
    }
    mnems
}

#[test]
fn goldens_agree_with_objdump() {
    assert!(have("objdump"), "objdump (binutils) required for M1 cross-check");
    let dir = PathBuf::from(root()).join("target/tmp");
    std::fs::create_dir_all(&dir).unwrap();
    let golden_path = dir.join("amd64_goldens.txt");
    generate_goldens(&golden_path);

    let data = std::fs::read_to_string(&golden_path).unwrap();
    let tmp = dir.join("amd64_blob.bin");
    let mut checked = 0;
    for line in data.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, '|').collect();
        assert_eq!(parts.len(), 3, "bad golden line: {line}");
        let (name, hex, text) = (parts[0], parts[1], parts[2]);
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let ours: Vec<String> = text
            .split(';')
            .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
            .collect();
        let theirs = objdump_mnemonics(&bytes, &tmp);
        assert_eq!(
            ours, theirs,
            "{name}: mnemonic mismatch\n  ours:    {ours:?}\n  objdump: {theirs:?}\n  bytes: {hex}"
        );
        checked += 1;
    }
    assert!(checked >= 50, "expected the full catalog, got {checked}");
}
