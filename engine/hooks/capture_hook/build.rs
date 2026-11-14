fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let minhook = std::path::Path::new(&manifest)
        .join("..")
        .join("obj")
        .join("minhook");
    println!("cargo:rustc-link-search=native={}", minhook.display());
    println!(
        "cargo:rerun-if-changed={}",
        minhook.join("minhook.lib").display()
    );
}
