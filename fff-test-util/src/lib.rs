use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static BASE_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest_dir)
        .parent()
        .expect("Failed to get parent directory of CARGO_MANIFEST_DIR")
        .to_path_buf()
});

/// Helper function to find WASM binary with environment variable override and fallback logic
fn find_wasm_path(env_var: &str, default_candidates: &[PathBuf]) -> PathBuf {
    // Check environment variable first
    if let Ok(path) = std::env::var(env_var) {
        let path_buf = PathBuf::from(path);
        if path_buf.exists() {
            return path_buf;
        }
        eprintln!(
            "Warning: {} is set to {:?} but file doesn't exist, trying defaults",
            env_var, path_buf
        );
    }

    // Try default candidates in order
    for candidate in default_candidates {
        if candidate.exists() {
            return candidate.clone();
        }
    }

    // If nothing found, return first candidate (will fail later with clear error)
    default_candidates[0].clone()
}

// Research-only paths (archived in research/ directory)
// These are kept for backward compatibility with existing tests
pub static NOOP_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    find_wasm_path(
        "FFF_NOOP_WASM_PATH",
        &[
            BASE_PATH.join("research/wasm-libs/fff-ude-example-noop/target/wasm32-wasip1/release/fff_ude_example_noop.wasm"),
            BASE_PATH.join("target/wasm32-wasip1/release/fff_ude_example_noop.wasm"),
        ],
    )
});
pub const NOOP_FUNC: &str = "noop_ffi";

pub static MEM_TEST_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    find_wasm_path(
        "FFF_MEMTEST_WASM_PATH",
        &[
            BASE_PATH.join("research/wasm-libs/fff-ude-example-memory-test/target/wasm32-wasip1/release/fff_ude_example_memory_test.wasm"),
            BASE_PATH.join("target/wasm32-wasip1/release/fff_ude_example_memory_test.wasm"),
        ],
    )
});
pub const MEM_TEST_FUNC: &str = "test_ffi";

// Production WASM paths
pub static BP_WASM_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    find_wasm_path(
        "FFF_BP_WASM_PATH",
        &[
            BASE_PATH.join("target/wasm32-wasip1/release/fff_ude_example.wasm"),
            PathBuf::from("/usr/local/lib/fff/fff_ude_example.wasm"),
        ],
    )
});
pub const BP_WASM_FUNC: &str = "decode_bp_ffi";

pub static VORTEX_WASM_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    find_wasm_path(
        "FFF_VORTEX_WASM_PATH",
        &[
            BASE_PATH.join("target/wasm32-wasip1/release/fff_ude_example_vortex.wasm"),
            PathBuf::from("/usr/local/lib/fff/fff_ude_example_vortex.wasm"),
        ],
    )
});
pub const VORTEX_WASM_FUNC: &str = "decode_vortex_ffi";
pub const VORTEX_WASM_FUNC_GENERAL: &str = "decode_vortex_general_ffi";

pub static BUILTIN_WASM_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    find_wasm_path(
        "FFF_BUILTIN_WASM_PATH",
        &[
            BASE_PATH.join("target/wasm32-wasip1/opt-size-lvl3/fff_ude_example_fff.wasm"),
            BASE_PATH.join("target/wasm32-wasip1/release/fff_ude_example_fff.wasm"),
            PathBuf::from("/usr/local/lib/fff/fff_ude_example_fff.wasm"),
        ],
    )
});
pub const WASM_FUNC_GENERAL: &str = "decode_general_ffi";

pub const TEST_SCHEMES: [&str; 6] = ["pco", "lz4", "flsbp", "fff", "gzip", "zstd"];
