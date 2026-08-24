fn main() {
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-link-lib=dbghelp");
    }
    tauri_build::build()
}
