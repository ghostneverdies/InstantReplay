// MinHook is compiled by engine/hooks/build.ps1 into engine/hooks/obj/minhook.
// Point the Rust linker at that directory so the #[link(name = "minhook")] block
// resolves both from the build script and from a plain `cargo build`.
fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let minhook = std::path::Path::new(&manifest)
        .join("..")
        .join("obj")
        .join("minhook");
    println!("cargo:rustc-link-search=native={}", minhook.display());
    println!("cargo:rerun-if-changed={}", minhook.join("minhook.lib").display());
}
