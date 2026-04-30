fn main() {
    // nvcc-compiled CUDA objects in libggml-cuda.a use R_X86_64_PC32
    // relocations against libc symbols (e.g. `stderr`). Those can't be
    // linked into a position-independent executable. -no-pie tells the
    // linker to emit a non-PIE binary. We use `rustc-link-arg-bins` so
    // it only applies to the final executable, NOT to proc-macro .so
    // builds (which would then fail with "undefined symbol: main").
    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-arg-bins=-no-pie");
    }
    tauri_build::build()
}
