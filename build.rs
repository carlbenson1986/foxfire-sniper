use std::env;

fn main() {
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/opt/homebrew/opt/postgresql@16/lib");
        println!("cargo:rustc-link-arg=-Wl,-rpath,/opt/homebrew/opt/libiconv/lib");
    }
}
