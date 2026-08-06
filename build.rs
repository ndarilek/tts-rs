//! Keeps the checked-in Android bridge dex in step with its Java source.
//!
//! The dex is checked in so that building this crate needs neither a JDK nor the Android SDK. To
//! keep that honest, this hashes _android/Bridge.java_ against a stamp beside the dex, and rebuilds
//! only on a mismatch. Runs on every target so an edit surfaces on any platform.

use std::{
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const SOURCE: &str = "android/Bridge.java";
const DEX: &str = "android/bridge.dex";
const STAMP: &str = "android/bridge.dex.stamp";

/// `InMemoryDexClassLoader`, which loads the bridge, arrived in API 26.
const MIN_API: &str = "26";

/// Nothing in _Bridge.java_ needs newer, and 8 keeps `d8`'s output stable across JDKs.
const JAVA_RELEASE: &str = "8";

fn main() {
    println!("cargo::rerun-if-changed={SOURCE}");
    println!("cargo::rerun-if-changed={DEX}");
    println!("cargo::rerun-if-changed={STAMP}");
    println!("cargo::rerun-if-changed=build.rs");

    let source = fs::read(SOURCE).unwrap_or_else(|e| panic!("Failed to read {SOURCE}: {e}"));
    let stamp = hash(&source);
    if fs::read_to_string(STAMP).is_ok_and(|s| s.trim() == stamp) && Path::new(DEX).exists() {
        return;
    }

    // Only reached after an edit, so the toolchain requirements land on whoever made it.
    println!("cargo::warning={SOURCE} changed; rebuilding {DEX}");
    build_dex();
    fs::write(STAMP, format!("{stamp}\n"))
        .unwrap_or_else(|e| panic!("Failed to write {STAMP}: {e}"));
}

/// FNV-1a: detecting an edit needs no cryptographic hash, and this keeps the script dependency-free.
fn hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mut s = String::new();
    write!(s, "{h:016x}").expect("Writing to a String cannot fail");
    s
}

fn build_dex() {
    let sdk = env::var_os("ANDROID_HOME").or_else(|| env::var_os("ANDROID_SDK_ROOT"));
    let Some(sdk) = sdk.map(PathBuf::from) else {
        panic!(
            "{SOURCE} changed, so {DEX} must be rebuilt, but neither ANDROID_HOME nor ANDROID_SDK_ROOT is set"
        )
    };
    let platforms = sdk.join("platforms");
    let Some(platform) = newest(&platforms, |name| {
        version_key(name.strip_prefix("android-")?)
    }) else {
        panic!("No SDK platform found under {}", platforms.display())
    };
    let android_jar = platform.join("android.jar");
    let build_tools = sdk.join("build-tools");
    let Some(d8) = newest(&build_tools, version_key) else {
        panic!("No build-tools found under {}", build_tools.display())
    };
    let d8 = d8.join("d8");

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo always sets OUT_DIR"));
    let classes = out.join("bridge-classes");
    let dex = out.join("bridge-dex");
    for dir in [&classes, &dex] {
        let _ = fs::remove_dir_all(dir);
        fs::create_dir_all(dir)
            .unwrap_or_else(|e| panic!("Failed to create {}: {e}", dir.display()));
    }

    run(Command::new(java_tool("javac"))
        .arg("-nowarn")
        .args(["--release", JAVA_RELEASE])
        .arg("-classpath")
        .arg(&android_jar)
        .arg("-d")
        .arg(&classes)
        .arg(SOURCE));
    run(Command::new(&d8)
        .arg("--release")
        .args(["--min-api", MIN_API])
        .arg("--lib")
        .arg(&android_jar)
        .arg("--output")
        .arg(&dex)
        .arg(classes.join("rs/tts/Bridge.class")));

    fs::copy(dex.join("classes.dex"), DEX)
        .unwrap_or_else(|e| panic!("Failed to install {DEX}: {e}"));
}

/// Resolves a JDK tool, preferring `JAVA_HOME` over whatever is on `PATH`.
fn java_tool(name: &str) -> PathBuf {
    env::var_os("JAVA_HOME").map_or_else(
        || PathBuf::from(name),
        |home| PathBuf::from(home).join("bin").join(name),
    )
}

/// Returns the entry of `dir` whose name sorts highest under `key`, ignoring names `key` rejects.
fn newest(dir: &Path, key: impl Fn(&str) -> Option<u64>) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let ranked = key(entry.file_name().to_str()?)?;
            Some((ranked, entry.path()))
        })
        .max_by_key(|(ranked, _)| *ranked)
        .map(|(_, path)| path)
}

/// Packs a dotted version into a sortable integer, e.g. `34.0.0` and `34` both to `34_000_000`.
/// `None` for anything non-numeric, which skips prereleases and extension levels.
fn version_key(name: &str) -> Option<u64> {
    let mut parts = name.split('.');
    let mut key = 0;
    for _ in 0..3 {
        let part = match parts.next() {
            Some(part) => part.parse::<u64>().ok()?,
            None => 0,
        };
        key = key * 1000 + part;
    }
    Some(key)
}

fn run(command: &mut Command) {
    let program = command.get_program().to_owned();
    let status = command
        .status()
        .unwrap_or_else(|e| panic!("Failed to run {}: {e}", Path::new(&program).display()));
    assert!(
        status.success(),
        "{} failed with {status}",
        Path::new(&program).display()
    );
}
