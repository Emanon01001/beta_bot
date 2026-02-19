#![deny(rust_2018_idioms)]

#[cfg(feature = "generate_binding")]
use std::path::PathBuf;
use std::{env, fmt::Display};

/// Outputs the library-file's prefix as word usable for actual arguments on
/// commands or paths.
const fn rustc_linking_word(is_static_link: bool) -> &'static str {
    if is_static_link {
        "static"
    } else {
        "dylib"
    }
}

/// Generates a new binding at `src/lib.rs` using `src/wrapper.h`.
#[cfg(feature = "generate_binding")]
fn generate_binding() {
    const ALLOW_UNCONVENTIONALS: &'static str = "#![allow(non_upper_case_globals)]\n\
                                                 #![allow(non_camel_case_types)]\n\
                                                 #![allow(non_snake_case)]\n";

    let bindings = bindgen::Builder::default()
        .header("src/wrapper.h")
        .raw_line(ALLOW_UNCONVENTIONALS)
        .generate()
        .expect("Unable to generate binding");

    let binding_target_path = PathBuf::new().join("src").join("lib.rs");

    bindings
        .write_to_file(binding_target_path)
        .expect("Could not write binding to the file at `src/lib.rs`");

    println!("cargo:info=Successfully generated binding.");
}

fn build_opus(is_static: bool) {
    use std::env;
    use std::path::Path;

    let opus_path = Path::new("opus");

    println!(
        "cargo:info=Opus source path used: {:?}.",
        opus_path
            .canonicalize()
            .expect("Could not canonicalise to absolute path")
    );

    println!("cargo:info=Building Opus via CMake.");

    let mut cfg = cmake::Config::new(opus_path);

    // Android cross builds need the ABI/level explicitly, otherwise CMake
    // defaults to armeabi-v7a and fails when targeting aarch64-linux-android.
    let target = env::var("TARGET").unwrap_or_default();
    if target.contains("android") {
        let platform = env::var("ANDROID_PLATFORM").unwrap_or_else(|_| "21".into());
        let abi = if target.starts_with("aarch64") { "arm64-v8a" } else { "armeabi-v7a" };
        cfg.define("ANDROID_ABI", abi)
            .define("ANDROID_PLATFORM", platform);
    }

    if cfg!(target_os = "windows") && cfg!(target_env = "msvc") {
        if env::var_os("CMAKE_GENERATOR").is_none() {
            cfg.generator("NMake Makefiles");
        }
    }

    // プロファイルはお任せで良ければ省略でもOK（cmake::build と同じ挙動）
    // 必要なら MinSizeRel 等に変えても良い
    // cfg.profile("MinSizeRel");

    let opus_build_dir = cfg.build();
    // ★ ここまで

    link_opus(is_static, opus_build_dir.display())
}


fn link_opus(is_static: bool, opus_build_dir: impl Display) {
    let is_static_text = rustc_linking_word(is_static);

    println!(
        "cargo:info=Linking Opus as {} lib: {}",
        is_static_text, opus_build_dir
    );
    println!("cargo:rustc-link-lib={}=opus", is_static_text);
    println!("cargo:rustc-link-search=native={}/lib", opus_build_dir);
}

#[cfg(any(unix, target_env = "gnu"))]
fn find_via_pkg_config(is_static: bool) -> bool {
    pkg_config::Config::new()
        .statik(is_static)
        .probe("opus")
        .is_ok()
}

/// Based on the OS or target environment we are building for,
/// this function will return an expected default library linking method.
///
/// If we build for Windows, MacOS, or Linux with musl, we will link statically.
/// However, if you build for Linux without musl, we will link dynamically.
///
/// **Info**:
/// This is a helper-function and may not be called if
/// if the `static`-feature is enabled, the environment variable
/// `LIBOPUS_STATIC` or `OPUS_STATIC` is set.
fn default_library_linking() -> bool {
    cfg!(any(windows, target_os = "macos", target_env = "musl"))
}


fn find_installed_opus() -> Option<String> {
    if let Ok(lib_directory) = env::var("LIBOPUS_LIB_DIR") {
        Some(lib_directory)
    } else if let Ok(lib_directory) = env::var("OPUS_LIB_DIR") {
        Some(lib_directory)
    } else {
        None
    }
}

fn is_static_build() -> bool {
    if cfg!(feature = "static") && cfg!(feature = "dynamic") {
        default_library_linking()
    } else if cfg!(feature = "static")
        || env::var("LIBOPUS_STATIC").is_ok()
        || env::var("OPUS_STATIC").is_ok()
    {
        println!("cargo:info=Static feature or environment variable found.");

        true
    } else if cfg!(feature = "dynamic") {
        println!("cargo:info=Dynamic feature enabled.");

        false
    } else {
        println!("cargo:info=No feature or environment variable found, linking by default.");

        default_library_linking()
    }
}

fn main() {
    #[cfg(feature = "generate_binding")]
    generate_binding();

    let is_static = is_static_build();

    #[cfg(any(unix, target_env = "gnu"))]
    {
        if std::env::var("LIBOPUS_NO_PKG").is_ok() || std::env::var("OPUS_NO_PKG").is_ok() {
            println!("cargo:info=Bypassed `pkg-config`.");
        } else if find_via_pkg_config(is_static) {
            println!("cargo:info=Found `Opus` via `pkg_config`.");

            return;
        } else {
            println!("cargo:info=`pkg_config` could not find `Opus`.");
        }
    }

    // Build scripts run on the host. Use TARGET env instead of cfg! here so
    // cross-compiling to Android on Windows does not incorrectly link msvcrt.
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("windows-msvc") {
        // Choose debug CRT when building in debug/profile (tests use debug),
        // otherwise choose the release CRT.
        let profile = std::env::var("PROFILE").unwrap_or_default();
        if profile == "debug" {
            println!("cargo:rustc-link-lib=dylib=msvcrtd");
        } else {
            println!("cargo:rustc-link-lib=dylib=msvcrt");
        }
    }

    if let Some(installed_opus) = find_installed_opus() {
        link_opus(is_static, installed_opus);
    } else {
        build_opus(is_static);
    }
}
