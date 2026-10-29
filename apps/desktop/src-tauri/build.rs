fn main() {
    // The updater's capability lives one level down so the store build,
    // which compiles no updater plugin, can leave it out: tauri-build
    // rejects a permission whose plugin is not a dependency, and `*` does
    // not descend into `capabilities/updater/`.
    let pattern = if std::env::var_os("CARGO_FEATURE_SELF_UPDATE").is_some() {
        "./capabilities/**/*"
    } else {
        "./capabilities/*.json"
    };
    println!("cargo:rerun-if-changed=capabilities");
    if let Err(e) =
        tauri_build::try_build(tauri_build::Attributes::new().capabilities_path_pattern(pattern))
    {
        // A build script reports failure by panicking; `tauri_build::build()`
        // does the same internally.
        panic!("tauri-build failed: {e:#}");
    }
}
