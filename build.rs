fn main() {
    // Help the linker find libpam when only the versioned .so exists
    #[cfg(feature = "pam")]
    {
        // Add standard library paths
        println!("cargo:rustc-link-search=native=/usr/lib/x86_64-linux-gnu");
        println!("cargo:rustc-link-search=native=/usr/lib");

        // Check if libpam.so exists; if not, try the versioned name
        let paths = [
            "/usr/lib/x86_64-linux-gnu/libpam.so",
            "/usr/lib/libpam.so",
        ];
        let has_unversioned = paths.iter().any(|p| std::path::Path::new(p).exists());

        if !has_unversioned {
            // Try to find the versioned library and create a symlink in OUT_DIR
            let versioned_paths = [
                "/usr/lib/x86_64-linux-gnu/libpam.so.0",
                "/usr/lib/libpam.so.0",
            ];
            for path in &versioned_paths {
                if std::path::Path::new(path).exists() {
                    let out_dir = std::env::var("OUT_DIR").unwrap();
                    let link_path = format!("{}/libpam.so", out_dir);
                    let _ = std::os::unix::fs::symlink(path, &link_path);
                    println!("cargo:rustc-link-search=native={}", out_dir);
                    break;
                }
            }
        }
    }
}
