//! The admin panel is embedded from `web/dist`. Make sure the folder exists
//! so the daemon also builds before the panel has been built.

fn main() {
    let dist = std::path::Path::new("../../web/dist");
    let _ = std::fs::create_dir_all(dist);
    println!("cargo:rerun-if-changed=../../web/dist");
    println!("cargo:rerun-if-changed=migrations");
}
