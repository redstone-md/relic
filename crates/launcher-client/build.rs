fn main() {
    // The languages are built in from this folder; a file added to it has
    // to be picked up even when no Rust source changed.
    println!("cargo:rerun-if-changed=locales");
}
