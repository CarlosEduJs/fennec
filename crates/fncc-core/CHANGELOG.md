# Changelog - fncc-core

All notable changes to `fncc-core` crate will be documented in this file.
## [0.3.0] - 2026-07-29

- **Multi-file component imports** — `.fui` files can now import and compose components from other `.fui` files using Rust-style `use` paths in the frontmatter.

  **Import syntax:**
  ```fui
  ---
  @state AppState
  use ui::components::Header;
  use ui::components::{Footer, Sidebar};
  use gpui::TextInput;
  ---

  <Stack direction="vertical" gap="16">
    <Header />
    <Text>Body</Text>
    <Footer />
  </Stack>
  ```

  **How it works:**
  - `use ui::path::Component;` resolves `ui::path::Component` to `<ui_dir>/path/Component.fui` (e.g. `src/ui/components/Header.fui`)
  - Imported components must be stateless (no `@state` directive) — enforced at build time
  - Codegen generates direct function calls: `.child(render_header())` instead of entity handles
  - Render function names derive from the **file stem**, not the root element — so `Header.fui` always produces `render_header()` regardless of its root element

  **Grouped imports** use Rust's brace syntax: `use ui::components::{Button, Card};`

  **GPUI imports** (`use gpui::TextInput;`) are emitted as real Rust `use` statements and registered with the codegen for future native component support; templates still fall back to `div()` for these elements.

  **Design decisions:**
  - Frontmatter stays "real Rust" — `use ui::...` is the only convention; no new DSL syntax
  - `use crate::...` imports pass through unchanged — only `use ui::...` is intercepted
  - Recursive directory scanning: all `.fui` files under `ui_dir` are collected, enabling subdirectory-based component organization
  - Two-pass pipeline: first parses all files into an import index, then resolves cross-references before codegen

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

