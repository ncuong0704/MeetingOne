// ============================================================================
// ONNX Runtime GPU dylib fix
// ============================================================================
// `ort-sys`'s own `download-binaries` mechanism (via crate `ort`, used by the CAPU
// punctuation engine and the ROVER RNNT decoder) fetches a package that only contains a
// *static* `onnxruntime.lib` — no matching `onnxruntime.dll`. With the `load-dynamic`
// ort feature (required for the CUDA execution provider's shared-provider bridge to
// work at all — static linking silently fails CUDA registration with a generic "not
// enabled in this build" error), `ort` falls back to whatever `onnxruntime.dll` it can
// find via the OS's standard DLL search — which, on a dev machine, is often a stale or
// mismatched version (e.g. left over from an old build, or a different tool's bundled
// copy), causing a hard version-mismatch panic at startup.
//
// Fix: download the official Microsoft `onnxruntime-win-x64-gpu` release archive (which
// bundles a matching `onnxruntime.dll` + all provider DLLs from the same build) and copy
// those four files into every location a compiled binary can end up (the profile dir
// itself, `deps/`, and `examples/` — Windows' DLL search doesn't walk up to a parent
// directory), so they take precedence over whatever `ort-sys` or the OS search would
// otherwise find. `ort-sys` is additionally pulled in as a build-dependency (see the
// `cuda` feature in Cargo.toml) purely to give this script a well-defined ordering
// relative to at least one of its own dylib-copying build-script instances; because
// Cargo's resolver still builds a second, separately-featured `ort-sys` instance for the
// regular dependency graph, full ordering isn't guaranteed — this script intentionally
// re-verifies (and self-heals) on every build rather than relying on it.
//
// Verified end-to-end via `cargo test --features cuda capu_engine::...::restore_punctuation_on_real_model
// -- --ignored --nocapture` with `ORT_LOG=verbose`: before this fix, `ort` panicked with
// "expected GetVersionString to return '1.22.x', but got '1.17.1'"; after, ONNX Runtime logs
// show `Successfully registered 'CUDAExecutionProvider'`-equivalent partitioning ("... for
// CUDAExecutionProvider") and `Extending BFCArena for Cuda`/`CudaPinned`, confirming real
// GPU execution, not a silent CPU fallback.

use std::path::{Path, PathBuf};

const ONNXRUNTIME_VERSION: &str = "1.22.0";
const DOWNLOAD_URL: &str =
    "https://github.com/microsoft/onnxruntime/releases/download/v1.22.0/onnxruntime-win-x64-gpu-1.22.0.zip";
const DLL_NAMES: &[&str] = &[
    "onnxruntime.dll",
    "onnxruntime_providers_cuda.dll",
    "onnxruntime_providers_shared.dll",
    "onnxruntime_providers_tensorrt.dll",
];

/// Ensures the profile output directory has a matching set of GPU-capable onnxruntime
/// dylibs. No-op on non-Windows or when the `cuda` feature isn't active (checked by the
/// caller). Cheap on repeat builds: only re-downloads if the persistent cache is missing
/// or incomplete; always does a fast local copy into the profile dir since `cargo clean`
/// wipes `target/` (and thus any dylibs `ort-sys` or a previous run placed there) without
/// touching this cache.
pub fn ensure_onnxruntime_gpu_dylibs() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let profile_dir = match profile_output_dir() {
        Some(d) => d,
        None => {
            println!(
                "cargo:warning=⚠️  Could not determine profile output directory; skipping onnxruntime GPU dylib fix"
            );
            return;
        }
    };

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR environment variable not set");
    let cache_dir = PathBuf::from(&manifest_dir)
        .join("onnxruntime-gpu-cache")
        .join(ONNXRUNTIME_VERSION);

    if !cache_is_complete(&cache_dir) {
        println!(
            "cargo:warning=📥 Downloading official onnxruntime-win-x64-gpu {} (ort-sys's own download only provides a version-mismatched static .lib, not a usable .dll — see build/onnxruntime_gpu.rs)",
            ONNXRUNTIME_VERSION
        );
        if let Err(e) = download_and_extract(&cache_dir) {
            println!(
                "cargo:warning=⚠️  Failed to download/extract onnxruntime GPU dylibs: {e}. CUDA execution provider may not work; falling back to whatever ort-sys/the OS search finds."
            );
            return;
        }
        println!("cargo:warning=✅ onnxruntime GPU dylibs cached at {}", cache_dir.display());
    }

    // Mirror `ort-sys`'s own `copy_libraries`: the compiled binary can end up directly in
    // the profile dir (`target/{profile}/`, the app/production binary), or in `deps/`
    // (test binaries) or `examples/` (example binaries) — Windows' DLL search does not
    // walk up to a parent directory, so each location needs its own copy of the dylibs.
    for dir in [profile_dir.clone(), profile_dir.join("deps"), profile_dir.join("examples")] {
        if !dir.is_dir() {
            continue;
        }
        if let Err(e) = copy_cache_to_profile_dir(&cache_dir, &dir) {
            println!("cargo:warning=⚠️  Failed to copy onnxruntime GPU dylibs into {}: {e}", dir.display());
            continue;
        }
        println!(
            "cargo:warning=✅ onnxruntime GPU dylibs ({}) placed in {}",
            ONNXRUNTIME_VERSION,
            dir.display()
        );
    }

    // Deliberately no `cargo:rerun-if-changed` here: `ort-sys` (a transitive dependency,
    // pulled in both as a build-dependency and a regular dependency — see the `cuda`
    // feature comment in Cargo.toml) also copies its own, version-mismatched onnxruntime
    // dylibs into this same directory, and Cargo does not guarantee our build script runs
    // after every one of its instances. Emitting no rerun instructions makes Cargo treat
    // this script as always-dirty, so it re-verifies (cheap: a handful of file-size stats,
    // full re-copy only on mismatch) on every build and self-heals if `ort-sys` wins a race.
}

/// `OUT_DIR` for a build script is `target/{profile}/build/{crate}-{hash}/out`; walk up
/// three ancestors to get `target/{profile}/`, the directory the compiled binary lives in.
fn profile_output_dir() -> Option<PathBuf> {
    let out_dir = std::env::var("OUT_DIR").ok()?;
    Path::new(&out_dir).ancestors().nth(3).map(|p| p.to_path_buf())
}

fn cache_is_complete(cache_dir: &Path) -> bool {
    DLL_NAMES.iter().all(|name| cache_dir.join(name).is_file())
}

fn copy_cache_to_profile_dir(cache_dir: &Path, profile_dir: &Path) -> Result<(), String> {
    for name in DLL_NAMES {
        let src = cache_dir.join(name);
        let dst = profile_dir.join(name);
        // Skip the copy if the destination is already byte-identical in size — avoids
        // needless I/O on every incremental build for these large (up to ~300MB) files.
        if let (Ok(src_meta), Ok(dst_meta)) = (std::fs::metadata(&src), std::fs::metadata(&dst)) {
            if src_meta.len() == dst_meta.len() {
                continue;
            }
        }
        std::fs::copy(&src, &dst).map_err(|e| format!("copy {} -> {}: {e}", src.display(), dst.display()))?;
    }
    Ok(())
}

fn download_and_extract(cache_dir: &Path) -> Result<(), String> {
    use std::io::Write;

    std::fs::create_dir_all(cache_dir).map_err(|e| format!("create cache dir: {e}"))?;

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(900))
        .build()
        .map_err(|e| format!("build HTTP client: {e}"))?;

    let response = client
        .get(DOWNLOAD_URL)
        .send()
        .map_err(|e| format!("download {DOWNLOAD_URL}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP error {} downloading {DOWNLOAD_URL}", response.status()));
    }

    let content = response.bytes().map_err(|e| format!("read response body: {e}"))?;
    println!("cargo:warning=📦 Downloaded {:.1} MB", content.len() as f64 / 1_048_576.0);

    let temp_zip = std::env::temp_dir().join(format!("onnxruntime-win-x64-gpu-{}.zip", ONNXRUNTIME_VERSION));
    {
        let mut file = std::fs::File::create(&temp_zip).map_err(|e| format!("create temp zip: {e}"))?;
        file.write_all(&content).map_err(|e| format!("write temp zip: {e}"))?;
    }

    let extract_dir = std::env::temp_dir().join(format!("onnxruntime-win-x64-gpu-{}-extract", ONNXRUNTIME_VERSION));
    let _ = std::fs::remove_dir_all(&extract_dir);
    std::fs::create_dir_all(&extract_dir).map_err(|e| format!("create extract dir: {e}"))?;

    let zip_file = std::fs::File::open(&temp_zip).map_err(|e| format!("open temp zip: {e}"))?;
    let mut archive = zip::ZipArchive::new(zip_file).map_err(|e| format!("read zip archive: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("read zip entry {i}: {e}"))?;
        let Some(name) = entry.enclosed_name() else {
            continue; // skip suspicious entries (path traversal)
        };
        let Some(filename) = name.file_name().and_then(|f| f.to_str()) else {
            continue;
        };
        if !DLL_NAMES.contains(&filename) {
            continue; // we only need the four DLLs, not the .lib/.pdb siblings
        }
        let out_path = extract_dir.join(filename);
        let mut out_file = std::fs::File::create(&out_path).map_err(|e| format!("create {}: {e}", out_path.display()))?;
        std::io::copy(&mut entry, &mut out_file).map_err(|e| format!("extract {filename}: {e}"))?;
    }

    for name in DLL_NAMES {
        let extracted = extract_dir.join(name);
        if !extracted.is_file() {
            return Err(format!("{name} not found in downloaded archive"));
        }
        std::fs::copy(&extracted, cache_dir.join(name)).map_err(|e| format!("cache {name}: {e}"))?;
    }

    let _ = std::fs::remove_file(&temp_zip);
    let _ = std::fs::remove_dir_all(&extract_dir);

    Ok(())
}
