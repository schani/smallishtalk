//! Portability guards for the compiler's own source files.
//!
//! GST 3.2.5's *native* file-in lexer (which reads exactly these files —
//! all compiled-target source goes through our StLexer instead) rejects
//! non-ASCII `$` character literals. Target-language code may use Unicode
//! `$`-literals freely; the compiler's own source must not.

#[test]
fn compiler_sources_have_ascii_only_dollar_literals() {
    let dir = format!("{}/st/compiler", env!("CARGO_MANIFEST_DIR"));
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|x| x != "st") {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        for (i, w) in bytes.windows(2).enumerate() {
            if w[0] == b'$' && w[1] >= 0x80 {
                offenders.push(format!(
                    "{}: byte offset {} ($ followed by 0x{:02X})",
                    path.file_name().unwrap().to_string_lossy(),
                    i,
                    w[1]
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "non-ASCII $-literals in compiler sources (GST's native reader \
         cannot file these in):\n{}",
        offenders.join("\n")
    );
}
