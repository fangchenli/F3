# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

F3 (Future-proof File Format) is a next-generation columnar data file format currently transitioning from research prototype to production-ready code. It addresses layout shortcomings of last-generation formats like Parquet while maintaining interoperability and extensibility via embedded WebAssembly (Wasm) decoders.

**Status**: Actively transitioning to production (as of Feb 2026). Research artifacts are archived in [research/](research/) directory - see [research/README.md](research/README.md) for details.

### Core Design Principles

F3 is built on three fundamental principles:

1. **Interoperability**: Ability to read files across different programming languages, library versions, query engines, and hardware platforms
2. **Extensibility**: Easy introduction of new features (especially encoding methods) without breaking compatibility
3. **Efficiency**: State-of-the-art performance for modern hardware and workloads (wide tables, ML features, random access)

## Build and Development Commands

### Initial Setup

```bash
# Initialize submodules (required for third-party dependencies)
git submodule update --init --recursive

# Install system dependencies (Debian/Ubuntu)
./scripts/setup_debian.sh
# This installs: Rust, build tools, protobuf, emscripten, binaryen (wasm-opt)
```

### Building

```bash
# Build the main F3 package
cargo build -p fff-poc

# Build with release optimizations
cargo build -p fff-poc --release

# Build all workspace members
cargo build --workspace

# Build WASM user-defined encoders (requires emscripten)
./exp_scripts/build_wasm.sh
```

### Testing

```bash
# Run tests for main package
cargo test -p fff-poc

# Run tests with all features enabled
cargo test -p fff-poc --all-features

# Run tests for format definitions
cargo test -p fff-format --all-features

# Run all workspace tests
cargo test --workspace
```

### Benchmarking

Set required environment variables before running benchmarks:

```bash
export FFF_BENCH_DATA_PATH="/path/to/benchmark/data"
export TMPDIR="/path/to/tmp"  # Should be on same device as data path
```

Common benchmark commands:

```bash
# Compression ratio benchmark
cargo run -p fff-bench --example bench --release -- compressed-size 2>&1 | tee compress_bench.log

# Decompression speed
./exp_scripts/decompression.sh

# Dictionary scope benchmark
cargo run -p fff-bench --example bench_dictscope --release -- --output-file ./results/dictscope_final_run.csv

# Memory benchmarks
cargo run -p fff-bench --example bench_mem --release
cargo run -p fff-bench --example bench_mem_fff --release

# WASM microbenchmarks
./exp_scripts/wasm_micro_exp.sh

# Random access
./exp_scripts/random_access.sh
```

See [doc/paper_reproduction.md](doc/paper_reproduction.md) for reproducing specific paper experiments.

## Codebase Architecture

### Workspace Structure

The project uses a Cargo workspace with the following key crates:

- **fff-poc**: Main implementation of the F3 format
  - Entry point for reading/writing F3 files
  - Contains reader (with projection, selection), writer, decoder, and encoder logic
  - Modules: `reader/`, `writer.rs`, `decoder/`, `encoder/`, `context.rs`, `dict.rs`

- **fff-core**: Core data structures and error handling
  - Minimal dependencies, foundational types
  - Error types and macros used across the project

- **fff-format**: FlatBuffer schema definitions
  - Located in [format/File.fbs](format/File.fbs)
  - Build script auto-generates Rust bindings from .fbs files
  - Defines the on-disk file format structure

- **fff-encoding**: Built-in encoding implementations
  - Wraps Vortex encoding library for standard compression schemes
  - Used for native (non-WASM) encoding/decoding

- **fff-ude**: User-Defined Encoding interface
  - Defines the API for custom encoders
  - Used by both WASM and native encoder implementations

- **fff-ude-wasm**: WASM runtime for user-defined encodings
  - Manages loading and executing WASM decoder modules
  - Provides memory management and FFI for WASM decoders

- **fff-bench**: Benchmarking suite
  - Contains examples in [fff-bench/examples/](fff-bench/examples/)
  - Compares F3 against Parquet, ORC, Lance, Vortex
  - Performance, compression ratio, and memory usage tests

- **fff-test-util**: Testing utilities shared across crates

### WASM User-Defined Encodings (UDE)

The [wasm-libs/](wasm-libs/) directory contains various UDE (User-Defined Encoding) implementations compiled to WASM:

- `fff-ude-example*`: Example and test encoders (noop, custom, memory-test)
- `fff-ude-example-pco*`: PCO (Piecewise Constant Offset) encoding
- `fff-ude-example-fsst`, `-lz4`, `-gzip`, `-zstd`: Standard compression algorithms
- `fff-ude-example-flsbp`: FastLanes bit-packing
- `fff-ude-example-vortex`: Vortex encoding as WASM module
- `adv-ude-fff`: Advanced UDE implementation

Each is a separate Rust crate that compiles to WASM for embedding in F3 files.

#### F3 Decoding API

All UDE implementations must implement F3's decoding API (outputs to Apache Arrow format):

```rust
/// Check supported features. Examples: decode_ranges, batch_size
fn check(metadata: EncUnitMetadata) -> HashSet<Feature>;

/// Initializes the decoder
fn init(encunit: Bytes, kwargs: HashMap<Feature, Word>) -> *mut Decoder;

/// Decodes the EncUnit into Arrow Arrays
/// Called repeatedly for incremental decoding. Returns None when complete.
fn decode(decoder: *mut Decoder) -> Option<arrow::ArrayRef>;
```

**Key Features**:
- `decode_ranges`: Supports row selection/skipping within an EncUnit
- `batch_size`: Controls vectorized decoding batch size (e.g., 2K rows for better cache locality)
- **Extensibility via kwargs**: New features can be added without breaking existing decoders (backward compatibility via fallback to basic decoding)
- **Same codebase for native and Wasm**: No separate implementations needed—compiles to both targets

### File Format Design

F3 files are organized into two major parts:

#### Metadata Part
- **OptionalData (OptData)**: Key-value metadata segment storing Wasm binaries and reserved for future indexes/filters
- **Column Metadata (ColMetadata)**: Per-column, per-row-group metadata serialized as FlatBuffers
  - Enables zero-copy deserialization (critical for wide tables with thousands of columns)
  - Stores offset/size of IOUnits and metadata for EncUnits
  - No inline headers/footers in IOUnits or EncUnits—all metadata is centralized
- **Footer**: FlatBuffer acting as directory to column-level metadata
- **Postscript**: Final metadata section

#### Data Part
- **Row Groups (RG)**: Logical grouping of rows (similar to Parquet/ORC)
- **I/O Units (IOUnit)**: Physical I/O granularity, decoupled from row groups
  - Allows independent tuning of I/O size based on storage media
  - Solves Parquet's OOM issues during writing (can flush IOUnits incrementally vs. buffering entire row groups)
  - Contains one or more EncUnits
- **Encoding Units (EncUnit)**: Minimal unit for encoding/decoding
  - Opaque byte buffer interpreted by decoding implementation
  - Uses Vortex encodings as built-in (15+ state-of-the-art methods)
  - No block compression by default (modern hardware benefits from lightweight encodings without additional compression)
- **Dictionary Units (DictUnit)**: Flexible dictionary scoping (major innovation)
  - Unlike Parquet/ORC (fixed to row group), F3 allows per-IOUnit dictionary decisions
  - Three modes: `noDict`, `local` (dictionary in IOUnit), `shared` (references dictionary in other IOUnits via dict_id)
  - Dictionaries can be shared across columns with overlapping values
  - Optimal scope varies by column (13% favor local, 35% favor global, 52% favor in-between)

#### Key Differentiators from Parquet/ORC
1. **Decoupled IOUnit from row groups**: Better memory management during writes, optimal I/O sizing
2. **Flexible dictionary scope**: Significant compression improvements (88% of columns achieve near-best CR with flexible scoping)
3. **FlatBuffer metadata**: Zero-copy deserialization critical for wide tables
4. **Embedded Wasm decoders**: Future-proof compatibility—files contain both data and code to decode it
5. **Nested data via Arrow L&P model**: Simpler than Parquet's Dremel, better in-memory format consistency

This design ensures that any platform can read F3 files even without native decoder support.

### Key Dependencies

- **Arrow**: Data representation (`arrow-array`, `arrow-schema`, etc.)
- **Parquet**: For comparison benchmarks
- **Vortex**: Used for built-in encodings (forked version)
- **Lance**: For comparison benchmarks (forked version)
- **Wasmtime**: WASM runtime (v28.0.0)
- **FlatBuffers**: Serialization for file metadata

### Build Profiles

- `release`: Standard optimized build
- `bench`: Optimized for benchmarking with debug info
- `wizard` / `opt-size`: Minimal size optimization (for WASM modules)
- `opt-size-lvl3`: Size optimization with -O3

Specific WASM packages use `opt-level = 's'` to minimize binary size.

## Development Workflow

1. **Modifying the file format**: Edit [format/File.fbs](format/File.fbs), rebuild `fff-format`
   - The build script auto-generates Rust bindings from FlatBuffer schemas
2. **Adding new encodings**: Create a new crate in `wasm-libs/` following existing examples
   - Implement the F3 decoding API (check, init, decode functions)
   - Same code compiles to both native and Wasm targets
   - Use `opt-level = 's'` or build profile `opt-size`/`wizard` for minimal Wasm binary size
3. **Testing changes**: Run `cargo test -p fff-poc` and relevant benchmark examples
4. **Performance testing**: Use examples in `fff-bench/examples/` with proper data paths configured

## Important Implementation Details

### Reader/Writer Architecture

The reader and writer logic in `fff-poc` handle the complexities of:

- **Writer** ([writer.rs](fff-poc/src/writer.rs)):
  - Incremental IOUnit flushing (prevents OOM vs. Parquet's buffering)
  - Dictionary scope selection strategies (local vs. shared vs. global)
  - Encoding method selection per column
  - Metadata generation and FlatBuffer serialization

- **Reader** ([reader/](fff-poc/src/reader/)):
  - **builder.rs**: Reader initialization and configuration
  - **projection.rs**: Column projection (metadata-only reads for wide tables)
  - **selection.rs**: Row range selection and filtering
  - **legacy.rs**: Backward compatibility handling
  - Zero-copy metadata deserialization via FlatBuffers

### Dictionary Scope Selection

Three strategies are implemented (see Section 7.6 of the paper):

1. **Global**: Single dictionary for entire column
2. **Local**: Dictionary per IOUnit
3. **Adaptive**: Profile-based selection per column

Optimal scope varies significantly:
- String columns with low NDV: Favor global (larger scope = fewer dictionaries)
- High NDV columns: Favor local (smaller scope = shorter codes)
- 88% of real-world columns achieve near-optimal compression with flexible scoping

### Wasm Integration

The [fff-ude-wasm](fff-ude-wasm/) crate provides:
- Wasmtime runtime for executing embedded decoders
- Memory management and FFI for Wasm modules
- Fallback to native decoders when available (performance optimization)
- Safety guarantees (sandboxing, resource limits)

## Benchmarking and Comparisons

F3 is evaluated against multiple file formats:

- **Parquet**: Legacy standard, most widely adopted
- **ORC**: Hadoop ecosystem standard
- **Lance**: ML-focused format with versioning
- **Vortex**: Modern format with adaptive compression
- **Nimble** (via C++ benchmarks): Meta's internal format

Key benchmark dimensions:
1. **Compression ratio**: File size vs. raw data
2. **Decompression speed**: Throughput when reading files
3. **Metadata overhead**: Critical for wide tables (thousands of columns)
4. **Random access**: Row selection and column projection
5. **Memory usage**: Peak memory during read/write operations
6. **Wasm overhead**: Native decoder vs. embedded Wasm decoder performance

See [fff-bench/examples/](fff-bench/examples/) for specific benchmark implementations.

## Special Considerations

- **Native CPU optimizations**: `.cargo/config.toml` sets `target-cpu=native` for all builds
  - Performance-critical for vectorized encoding/decoding operations
  - May affect reproducibility across different CPU architectures
- **Memory allocator**: Uses `mimalloc` globally in fff-poc
  - Better performance than system allocator for allocation-heavy workloads
- **Submodules**: The project uses git submodules for `third_party/` (Lance datagen)
  - Always run `git submodule update --init --recursive` after clone
- **32-bit support**: Some WASM compilation requires gcc-multilib for 32-bit support
  - Needed for emscripten-based Wasm compilation
- **Deprecated code**: The [deprecated_code/](deprecated_code/) directory contains old implementations not used in current system
  - Includes earlier Wasm experiments and encoding implementations
  - Safe to ignore for current development
- **FlatBuffer version pinning**: Uses specific flatbuffers version (24.3.25) to match arrow-ipc
  - Changing versions may break compatibility
- **Arrow version**: Uses Arrow 53.0.0
  - UDE implementations must use compatible Arrow versions for FFI
- **Feature flags**: `fff-poc` supports optional features like `list-offsets-pushdown`
  - Check Cargo.toml for available features and their impact

## Common Gotchas

1. **Wide table performance**: F3's FlatBuffer metadata shines with 1000+ columns
   - Small tables may not show performance benefits over Parquet
2. **Dictionary scope selection**: No automatic optimizer yet
   - Current strategies are heuristic-based
   - Offline profiling and optimization is future work
3. **Wasm binary size**: Critical for file size overhead
   - Use `opt-size` profile, not `release`, for UDE compilation
   - Target: <100KB per encoding implementation
4. **IOUnit size tuning**: Depends on storage characteristics
   - NVMe SSDs: 4-8MB IOUnits
   - S3/cloud storage: Larger IOUnits (16-32MB) to amortize latency
5. **No block compression by default**: Intentional design choice
   - Modern hardware benefits more from lightweight encodings
   - Can enable compression at storage layer if needed
