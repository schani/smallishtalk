//! Phase 2 cross-language golden test (SPEC §20): the GST-side encoder's
//! bytes must decode with the Rust disassembler into the same mnemonic
//! sequence GST printed. This is the two-codebase agreement check for
//! instruction encodings, before any image exists.

use smallishtalk::asm::Insn;
use std::process::Command;

fn gst_available() -> bool {
    Command::new("gst")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn gst_encoded_bytes_decode_identically() {
    if !gst_available() {
        panic!("gst is required for the cross-language encoder test");
    }
    let dir = std::env::temp_dir().join(format!("smallishtalk-cross-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bin_path = dir.join("insns.bin");
    let script = dir.join("emit.st");

    // Compile a method exercising every instruction family, print the
    // mnemonics, and write the encoded bytes.
    std::fs::write(
        &script,
        format!(
            r#"| spec bytes |
spec := StCodeGen compileMethodSource: 'foo: x | t | t := x + 1. t := self bar: t. [t] value. ^super baz'
    inClass: StClassScope empty.
Transcript showCr: spec mnemonics.
Transcript showCr: '====='.
bytes := StEncoder bytesForAll: spec insns.
Platform writeBytes: bytes toFile: '{}'.
"#,
            bin_path.display()
        ),
    )
    .unwrap();

    let root = env!("CARGO_MANIFEST_DIR");
    let out = Command::new("gst")
        .arg("-Q")
        .args([
            &format!("{root}/st/compiler/Compat.st"),
            &format!("{root}/st/compiler/Treaty.st"),
            &format!("{root}/st/compiler/Platform.st"),
            &format!("{root}/st/compiler/AST.st"),
            &format!("{root}/st/compiler/Lexer.st"),
            &format!("{root}/st/compiler/Parser.st"),
            &format!("{root}/st/compiler/CodeGen.st"),
            &format!("{root}/st/compiler/Encoder.st"),
        ])
        .arg(&script)
        .output()
        .expect("run gst");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let gst_mnemonics: Vec<&str> = stdout
        .split("=====")
        .next()
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert!(
        !gst_mnemonics.is_empty(),
        "no mnemonics from gst; output: {stdout}"
    );

    let bytes = std::fs::read(&bin_path).expect("bytes written by gst");
    assert_eq!(bytes.len() % 4, 0);
    let rust_mnemonics: Vec<String> = bytes
        .chunks_exact(4)
        .map(|c| {
            let w = u32::from_le_bytes(c.try_into().unwrap());
            let insn = Insn::decode(w)
                .unwrap_or_else(|| panic!("undecodable instruction {w:#010x}"));
            format!("{insn}")
        })
        .collect();

    assert_eq!(
        rust_mnemonics.len(),
        gst_mnemonics.len(),
        "instruction count mismatch:\nGST:\n{}\nRust:\n{}",
        gst_mnemonics.join("\n"),
        rust_mnemonics.join("\n")
    );
    for (r, g) in rust_mnemonics.iter().zip(gst_mnemonics.iter()) {
        assert_eq!(r, g.trim(), "encoding disagreement");
    }
    std::fs::remove_dir_all(&dir).ok();
}
