fn main() {
    // Tell the Rust compiler to link against xmllite.lib
    println!("cargo:rustc-link-lib=dylib=xmllite");
}
