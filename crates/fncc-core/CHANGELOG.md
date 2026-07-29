# Changelog - fncc-core

All notable changes to `fncc-core` crate will be documented in this file.
## [0.2.0] - 2026-07-29

- Refactored `config.rs` to use proper TOML deserialization via `serde` + `toml` crate instead of homemade key-value parser
- `generate_all` now returns `anyhow::Result<()>` instead of panicking on errors
- Removed global `FILE_COUNTER` from codegen; added `generate_with_id()` with explicit file ID parameter
- Fixed parser: attribute names now support hyphens (e.g. `data-value`)
- Fixed parser: mismatched close tags now panic instead of being silently accepted
- Fixed parser: interpolation expressions inside quoted attribute values are now detected (e.g. `size="{expr}"`)
- Fixed parser: leading and trailing whitespace around documents is now accepted
- Added dependency: `anyhow`, `serde`, `toml`
- Added `generate_with_id` public function in codegen for multi-file projects

## [0.1.0] - 2026-07-28

- Parser for `.fui` (fncc UI) files, converting the declarative syntax into an AST.
- `codegen` module that generates Rust code from a `.fui` file's AST.
- `config` module for fncc configuration management.
- `generate_all()` entry point: parses all `.fui` files in a directory and writes the generated code to a single output file.

