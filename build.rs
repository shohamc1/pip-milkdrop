fn main() {
    let projectm = pkg_config::probe_library("libprojectM")
        .expect("libprojectM not found. Install with: brew install projectm");
    let projectm_datadir = pkg_config::get_variable("libprojectM", "pkgdatadir")
        .expect("libprojectM pkgdatadir not found");

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .file("shim.cpp")
        .flag("-std=c++17");

    for path in &projectm.include_paths {
        build.include(path);
    }

    build.compile("projectm_shim");

    println!("cargo:rustc-link-lib=framework=OpenGL");
    println!("cargo:rustc-link-search=framework=/System/Library/PrivateFrameworks");
    println!("cargo:rustc-env=PROJECTM_DATADIR={projectm_datadir}");
    println!("cargo:rerun-if-changed=shim.cpp");
}
