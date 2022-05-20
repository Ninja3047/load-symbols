# Load Symbols
Binary Ninja plugin written in Rust to automatically apply symbol information from split debug info on Linux.

## Requirements
 * Last tested with Binary Ninja 3.0.3448-dev
 * Requires nightly Rust (last tested with rustc 1.63.0-nightly (c06728704 2022-05-19))

## Building
 * `cargo build --release`

## Installing
Copy or create a symlink from `./target/release/libload_symbols.so` to `~/.binaryninja/plugins/libload_symbols.so`.

## Usage
 * Enable `analysis.experimental.parseDebugInfo` setting in Binary Ninja
 * Ensure that split debug info file exists at `/usr/lib/debug` in the same directory structure as the desired binary target.
