fn main() {
    let projectm = pkg_config::probe_library("libprojectM")
        .expect("libprojectM not found. Install with: brew install projectm");

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .file("shim.cpp")
        .flag("-std=c++17");

    for path in &projectm.include_paths {
        build.include(path);
    }

    build.compile("projectm_shim");

    pkg_config::probe_library("sdl2").expect("SDL2 not found. Install with: brew install sdl2");

    println!("cargo:rustc-link-lib=framework=OpenGL");
    println!("cargo:rustc-link-search=framework=/System/Library/PrivateFrameworks");
    println!("cargo:rerun-if-changed=shim.cpp");
}
