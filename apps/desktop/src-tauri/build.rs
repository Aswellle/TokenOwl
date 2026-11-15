fn main() {
    tauri_build::build();
    // Test binaries link the same tauri runtime the app does. tauri-build
    // attaches its Windows resource (the Common-Controls v6 manifest that
    // comctl32's TaskDialogIndirect import needs) to bin targets only
    // (`rustc-link-arg-bins`), so any test binary that pulls the windowing
    // layer fails to load with STATUS_ENTRYPOINT_NOT_FOUND — the loader
    // binds comctl32 5.82 without the manifest. Re-attach the resource to
    // test targets as well; binaries that never touch comctl32 simply carry
    // an unused resource.
    if let Some(out_dir) = std::env::var_os("OUT_DIR") {
        let resource = std::path::Path::new(&out_dir).join("resource.lib");
        if resource.exists() {
            println!("cargo:rustc-link-arg-tests={}", resource.display());
        }
    }
}
