fn main() {
    // Compiled into release builds; without it the app sends nothing.
    println!("cargo:rerun-if-env-changed=DAGPEEK_GA_API_SECRET");
    tauri_build::build()
}
