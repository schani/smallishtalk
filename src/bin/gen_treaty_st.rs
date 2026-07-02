//! Generate st/compiler/Treaty.st from the canonical Treaty constants
//! (SPEC §20 Phase 0: "generated or checksummed" — we generate, and a cargo
//! test asserts the on-disk file is current).

use smallishtalk::treaty::treaty_st_source;

fn main() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/st/compiler/Treaty.st");
    std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
    std::fs::write(path, treaty_st_source()).unwrap();
    println!("wrote {path}");
}
