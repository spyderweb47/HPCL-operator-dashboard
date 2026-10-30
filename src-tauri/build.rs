fn main() {
    // The cloud URI can be baked in at build time (see cloud::load_config).
    println!("cargo:rerun-if-env-changed=HPCL_MONGODB_URI");
    println!("cargo:rerun-if-env-changed=HPCL_MONGODB_DB");
    tauri_build::build()
}
