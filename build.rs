use std::path::PathBuf;
use std::process::Command;

// Builds the vendored projectM 4.x as static libraries and links them into the binary, so
// the result is self-contained (depends only on system frameworks, no Homebrew/dylib).
fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let src = manifest.join("vendor/projectm");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let build_dir = out.join("pm4");

    assert!(
        src.join("CMakeLists.txt").exists(),
        "vendored projectM not found at {} - run: git submodule update --init --recursive",
        src.display()
    );

    let configure = Command::new("cmake")
        .args([
            "-S",
            src.to_str().unwrap(),
            "-B",
            build_dir.to_str().unwrap(),
            "-DCMAKE_BUILD_TYPE=Release",
            "-DBUILD_SHARED_LIBS=OFF",
            "-DENABLE_SYSTEM_GLM=OFF",
            "-DENABLE_SYSTEM_PROJECTM_EVAL=OFF",
            "-DENABLE_PLAYLIST=ON",
            "-DENABLE_SDL_UI=OFF",
            "-DBUILD_TESTING=OFF",
            "-DENABLE_INSTALL=OFF",
        ])
        .status()
        .expect("failed to run `cmake` configure (is CMake installed?)");
    assert!(configure.success(), "cmake configure failed");

    let build = Command::new("cmake")
        .args(["--build", build_dir.to_str().unwrap(), "-j", "8"])
        .status()
        .expect("failed to run `cmake` build");
    assert!(build.success(), "cmake build failed");

    // The vendored eval/glad archives are built but not installed, so link them straight
    // from the build tree alongside the two main libraries.
    let bd = build_dir.display();
    println!("cargo:rustc-link-search=native={bd}/src/libprojectM");
    println!("cargo:rustc-link-search=native={bd}/src/playlist");
    println!("cargo:rustc-link-search=native={bd}/vendor/projectm-eval/projectm-eval");
    println!("cargo:rustc-link-search=native={bd}/vendor/glad");
    println!("cargo:rustc-link-lib=static=projectM-4");
    println!("cargo:rustc-link-lib=static=projectM-4-playlist");
    println!("cargo:rustc-link-lib=static=projectM_eval");
    println!("cargo:rustc-link-lib=static=glad");

    // The media poller links the private MediaRemote framework (see media.rs).
    println!("cargo:rustc-link-search=framework=/System/Library/PrivateFrameworks");

    // projectM is C++ and pulls in these system frameworks.
    println!("cargo:rustc-link-lib=c++");
    println!("cargo:rustc-link-lib=framework=OpenGL");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");

    // Stock presets live in `<manifest>/presets/presets_stock` (see main.rs).
    println!("cargo:rustc-env=PROJECTM_DATADIR={}", manifest.display());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=vendor/projectm/CMakeLists.txt");
}
