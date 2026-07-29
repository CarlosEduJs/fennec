---
fncc-core: minor
---

- Refactored `config.rs` to use proper TOML deserialization via `serde` + `toml` crate instead of homemade key-value parser
- `generate_all` now returns `anyhow::Result<()>` instead of panicking on errors
- Removed global `FILE_COUNTER` from codegen; added `generate_with_id()` with explicit file ID parameter
- Fixed parser: attribute names now support hyphens (e.g. `data-value`)
- Fixed parser: mismatched close tags now panic instead of being silently accepted
- Fixed parser: interpolation expressions inside quoted attribute values are now detected (e.g. `size="{expr}"`)
- Fixed parser: leading and trailing whitespace around documents is now accepted
- Added dependency: `anyhow`, `serde`, `toml`
- Added `generate_with_id` public function in codegen for multi-file projects
