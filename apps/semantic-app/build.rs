fn main() {
    println!("cargo:rerun-if-changed=src/ui");
    println!("cargo:rerun-if-changed=src");
    let ui_dir = std::path::Path::new("src/ui");
    let src_dir = std::path::Path::new("src");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_file = std::path::Path::new(&out_dir).join("generated.rs");
    if let Err(e) = fncc_core::generate_all_with_options(fncc_core::GenerateOptions {
        ui_dir,
        out_file: &out_file,
        src_dir: Some(src_dir),
    }) {
        panic!("fncc build failed: {e}");
    }
}
