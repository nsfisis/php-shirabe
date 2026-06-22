use std::path::PathBuf;

// Composer ships its JSON schemas in res/ and JsonFile resolves them via __DIR__.
// We copy those schema files next to the built executable so they can be located
// at runtime through std::env::current_exe().
fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // OUT_DIR is target/<profile>/build/<pkg>-<hash>/out; its 3rd ancestor is target/<profile>.
    let target_profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR has an unexpected layout");

    let composer_res = manifest_dir.join("../../composer/res");
    let files = ["composer-schema.json", "composer-lock-schema.json"];

    // Binaries live in target/<profile>, test/example binaries in target/<profile>/deps;
    // populate a res/ directory next to both so current_exe()/../res resolves either way.
    for dest_dir in [
        target_profile_dir.join("res"),
        target_profile_dir.join("deps").join("res"),
    ] {
        std::fs::create_dir_all(&dest_dir).unwrap();
        for file in files {
            std::fs::copy(composer_res.join(file), dest_dir.join(file)).unwrap();
        }
    }

    for file in files {
        println!(
            "cargo:rerun-if-changed={}",
            composer_res.join(file).display()
        );
    }
}
