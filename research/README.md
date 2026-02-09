# Research Archive

This directory contains code and artifacts from the F3 research prototype that are preserved for paper reproduction and historical reference but are not part of the production codebase.

## Contents

### deprecated_code/
Contains three deprecated crates that were removed from active development:
- **fff-wasm/**: Early WASM compilation experiments (replaced by fff-ude-wasm)
- **fff-ude-macros/**: Macro-based approach (superseded by direct implementation)
- **fff-encoding-wasm/**: WASM encoding decoder experiments

**Reason for archival:** These crates were commented out of the workspace (Cargo.toml lines 6-7, 30) and marked as deprecated. They represent early explorations that informed the current architecture but are no longer maintained.

### wasm-libs/
Research-only WASM examples used for experiments and paper benchmarks:
- **fff-ude-example-noop/**: No-op encoder for baseline testing
- **fff-ude-example2/**: Duplicate/alternative example (superseded)
- **fff-ude-example-memory-test/**: Memory allocation testing prototype
- **test-size/**: Binary size measurement experiment
- **test-wmemcheck/**: Memory check experiment

**Reason for archival:** These examples were created specifically for research experiments and paper benchmarks. They are not needed for production use but are preserved for reproducibility.

### exp_scripts/
Deprecated experiment scripts:
- **layout_chunk_size.sh**: Marked "This script should be deprecated" in source
- **layout_rg_size.sh**: Related to deprecated layout experiments

**Reason for archival:** These scripts were for one-time experiments documented in the paper. They are superseded by the scripts in the main `exp_scripts/` directory.

## Building Research Code

All research code remains buildable for reference:

```bash
# Build deprecated crates (from research/deprecated_code/)
cd research/deprecated_code/fff-wasm
cargo build

# Build research wasm-libs
cd research/wasm-libs/fff-ude-example-noop
cargo build --target wasm32-wasip1
```

## Migration Date

Research code was archived on 2026-02-09 as part of the transition from research prototype to production-ready development.

## Paper Reproduction

For reproducing results from the SIGMOD 2025 paper "F3: The Open-Source Data File Format for the Future", see:
- [doc/paper_reproduction.md](../doc/paper_reproduction.md) for experiment reproduction steps
- [exp_scripts/](../exp_scripts/) for experiment automation scripts

The research archive complements these resources by preserving experimental implementations referenced during the research phase.
