use std::{env, fs};
fn main() {
    println!("cargo:rerun-if-changed=VERSION");
    println!("cargo:rerun-if-changed=Cargo.toml");
    let raw = fs::read_to_string("VERSION").expect("VERSION is required");
    let version = raw.trim_end_matches(['\r', '\n']);
    let parts: Vec<_> = version.split('.').collect();
    assert!(parts.len() == 4 && parts.iter().all(|p| !p.is_empty()
        && p.bytes().all(|b| b.is_ascii_digit()) && (p.len() == 1 || !p.starts_with('0')))
        && parts[3] == "0", "VERSION must be canonical A.B.C.0; nonzero revision mapping requires an explicit policy change");
    assert_eq!(
        parts[..3].join("."),
        env::var("CARGO_PKG_VERSION").unwrap(),
        "VERSION and Cargo package version mapping disagree"
    );
    println!("cargo:rustc-env=BLUE_DRAGON_PUBLIC_VERSION={version}");
}
