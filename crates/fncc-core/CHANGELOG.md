# Changelog - fncc-core

All notable changes to `fncc-core` crate will be documented in this file.
## [0.5.0] - 2026-07-30

- Add typed component props support: #[derive(Props)] struct scanning (with fncc::Props support), ImportSource::PropsType variant, generate_with_imports extended with props parameters, interpolation_expr helper for props.field resolution
- Add PropField.is_optional to detect Option<T> fields
- Add validate_props_usage: hard errors for unknown attributes and missing required (non-Option) fields
- Add 5 validation tests for error/success paths

## [0.4.0] - 2026-07-29

### Semantic Analysis — compiler foundation (`semantic::SemanticDb`, `GenerateOptions`)

Adds a new `semantic` module that scans Rust source files at build time to extract `#[fncc::command]` definitions, enabling automatic state inference, hard-error command validation, and a shared data model (`SemanticDb`) designed to be extended for typed props, LSP metadata, and incremental compilation.

#### Motivation

The `@state` directive in `.fui` frontmatter broke the "frontmatter is pure Rust" principle (see `wiki.md`). The compiler had no visibility into the Rust side — it blindly trusted `@state` and only validated command existence via the trampoline reference trick (`__fncc_cmd_{name}`). This was a pragmatic POC concession, but it meant:

- No type checking between `.fui` and `.rs`
- No way to catch missing commands early
- No foundation for future features (typed props, LSP, documentation)

#### What changed

**`crates/fncc-core/src/semantic.rs`** (new, 340 lines):

| Type | Purpose |
|---|---|
| `SemanticDb` | Central database: `commands: HashMap<String, CommandDef>`, `diagnostics: Vec<Diagnostic>` |
| `CommandDef` | `name`, `level` (Level1/2/3), `state_type`, `file` |
| `Diagnostic` | Typed errors: `CommandNotFound`, `StateTypeConflict`, `StateTypeMismatch` |
| `analyze_rs_files()` | Recursive `.rs` scan, parses with `syn`, extracts `#[fncc::command]` functions |

**`crates/fncc-core/src/lib.rs`** — new entry point and options:

```rust
pub struct GenerateOptions<'a> {
    pub ui_dir: &'a Path,
    pub out_file: &'a Path,
    pub src_dir: Option<&'a Path>,  // None = legacy, Some = semantic analysis
}

pub fn generate_all(ui_dir: &Path, out_file: &Path) -> Result<()>;
pub fn generate_all_with_options(opts: GenerateOptions) -> Result<()>;
```

Legacy `generate_all()` preserved unchanged. Apps opt in to the full pipeline by passing `src_dir`:

```rust
// build.rs — new API
fncc_core::generate_all_with_options(fncc_core::GenerateOptions {
    ui_dir: Path::new("src/ui"),
    out_file: &out_file,
    src_dir: Some(Path::new("src")),
}).unwrap();
```

**`crates/fncc-core/src/codegen.rs`** — `generate_with_imports()` now accepts `resolved_state_type: Option<&str>` that overrides `@state` when provided.

**`crates/fncc-core/Cargo.toml`** — added `syn` dependency for Rust source parsing.

**`crates/fncc-core/src/parser.rs`** — `collect_commands()` made public (shared between codegen and semantic analysis).

#### Semantic analysis pipeline

1. **Scan `.rs` files** for `#[fncc::command]` (recursive, skips unparseable files)
2. **Extract command metadata** — name, level, state type (for Level 3)
3. **Hard-error validation** — every `onclick="cmd"` in `.fui` must have a matching `#[fncc::command] fn cmd()` in `.rs`. If not found, build fails immediately with a clear diagnostic: `"in 'App.fui': onclick=\"missing\" references #[fncc::command] fn missing() which was not found in any Rust source file"`
4. **State inference** — when `.fui` omits `@state`, the compiler infers the state type from Level 3 command signatures. If `@state` is present and conflicts with inference, build fails
5. **Multiple state type check** — if commands in the same `.fui` reference different state types, build fails

#### Example: no `@state` needed

**`src/ui/App.fui`:**
```fui
<Stack direction="vertical" gap="12">
    <Text size="xl">Counter: {state.count}</Text>
    <Button onclick="increment">+1</Button>
</Stack>
```

**`src/main.rs`:**
```rust
#[derive(Default)]
struct CounterState { count: i32 }

#[fncc::command]
fn increment(state: &mut CounterState, cx: &mut Context<CounterState>) {
    state.count += 1;
    cx.notify();
}
```

The compiler infers `CounterState` as the component state type from the `increment` command signature. The `@state` directive is no longer required.

#### `SemanticDb` — designed to grow

```rust
pub struct SemanticDb {
    pub commands: HashMap<String, CommandDef>,
    pub diagnostics: Vec<Diagnostic>,
    // Future: components, state_types, props, types
}
```

Future phases (typed props, LSP metadata, incremental compilation) will add fields without breaking the existing API.

#### New example: `apps/semantic-app`

A complete counter application demonstrating state inference, command validation, and the new `build.rs` pattern. Built by `cargo check -p semantic-app`.

#### Tests

- 20 new tests across `semantic.rs` and `lib.rs`
- Coverage: command extraction (all 3 levels), state inference, missing command errors, state type conflicts, multiple state type errors, recursive `.rs` scanning, legacy backward compat
- All 110 `fncc-core` tests pass

#### Design decisions

| Decision | Rationale |
|---|---|
| **Explicit API** (`GenerateOptions`) | No auto-detection. Apps explicitly pass `src_dir` or use legacy path. Scales for future options. |
| **Hard error** on missing command | Fail fast with clear diagnostic. The compiler knows the command doesn't exist — no reason to silently pass through and produce confusing `rustc` errors. |
| **`syn` in `fncc-core`** | Correct Rust parsing (not fragile regex). Already a workspace dependency via `fncc-macros`. Build-time only — no runtime impact. |
| **`SemanticDb` as foundation** | Single source of truth for all compiler passes. Codegen, LSP, docs, and validation all read from the same database. |

#### Upgrade guide (backward-compatible)

Existing projects using `generate_all(ui_dir, out_file)` continue to work unchanged. To opt in to semantic analysis, change `build.rs`:

```rust
// Before
fncc_core::generate_all(ui_dir, &out_file).unwrap();

// After
fncc_core::generate_all_with_options(fncc_core::GenerateOptions {
    ui_dir,
    out_file: &out_file,
    src_dir: Some(Path::new("src")),
}).unwrap();
```

Also add `cargo:rerun-if-changed=src` to the build script so Cargo re-runs when `.rs` files change.

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

