fn main() {
    println!("cargo:rerun-if-changed=src/ui");
    let ui_dir = std::path::Path::new("src/ui");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_file = std::path::Path::new(&out_dir).join("generated.rs");
    fncc_core::generate_all(ui_dir, &out_file);
}
