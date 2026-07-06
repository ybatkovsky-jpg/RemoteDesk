fn main() {
    // libsodium on Windows GNU needs advapi32 for SystemFunction036 (RtlGenRandom).
    // Using rustc-link-arg to force the lib at the end of the linker line
    // (link order matters: -ladvapi32 must come after libsodium).
    #[cfg(all(target_os = "windows", target_env = "gnu"))]
    {
        println!("cargo:rustc-link-arg=-ladvapi32");
    }
}
