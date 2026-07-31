# fncc

Rust workspace (ed2024, resolver 3) — a compiler/framework that translates Astro-like `.fui` files into native GPUI Rust apps.

## Workspace crates (publish order)

| Crate | Role |
|---|---|
| `fncc-macros` | `#[fncc::command]` proc-macro (must publish first) |
| `fncc-styles` | CSS-like style compiler (token/theme resolution, GPUI mapping) |
| `fncc-core` | Pest parser + codegen (build-dependency for apps) |
| `fncc-runtime` | Re-exports `gpui` + convenience wrappers |
| `fncc` | Umbrella crate re-exporting macros+runtime |
| `xtask` | Release tooling (not published) |
| `fncc-example` | Example app (not published) |
| `fncc-styles-app` | Styles demo app — tokens, themes, `.fncss` cascade, `<Styles>` inline blocks (not published) |

## Developer commands (run from workspace root)

```bash
cargo test --workspace           # all tests
cargo test -p fncc-core          # single crate
cargo test -p fncc-core parser::tests::test_simple_element_parses_correctly  # single test
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
taplo format --check
cargo check --workspace          # faster verification than build
cargo build -p fncc-example      # build the example app (requires GPUI system deps)
```

CI order: `taplo format --check` → `cargo fmt --all --check` → `clippy -D warnings` → `cargo test --workspace`.

## Agent Workflows

Agents working on this repository MUST consult and follow the workflows in `.agents/workflows/`:

- `/develop`: End-to-end workflow for implementing features, fixes, or refactors.
- `/validate`: Local validation pipeline matching CI order (`taplo` → `fmt` → `clippy` → `tests`).
- `/create-changeset`: Generate version bump declarations for published crates (`fncc`, `fncc-core`, `fncc-styles`, `fncc-macros`, `fncc-runtime`).
- `/review-changes`: Quick audit of uncommitted/staged changes before finalizing tasks.
- `/review-pr`: Comprehensive code review for pull requests.
- `/use-skills`: Guidance on discovering and applying project skills (`coding-guidelines`, `rust-best-practices`, `rust-pragmatic`, `rust-review`).

## System dependencies (GPUI on Linux)

```bash
sudo apt-get install libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev
```

## How .fui compilation works

1. `build.rs` calls `fncc_core::generate_all(ui_dir, &out_file)`
2. `main.rs` does `include!(concat!(env!("OUT_DIR"), "/generated.rs"))`
3. `.fui` files live in `src/ui/` by default
4. Frontmatter uses `@state TypeName` to declare state; imports go in frontmatter too
5. Codegen generates `impl Render for TypeName` (stateful) or `pub fn render_xxx()` (stateless)

## How styles work

1. Styles can be defined inline in `<Styles>` blocks in `.fui` files or in standalone `.fncss` files
2. `.fncss` files are discovered by directory — each dir's styles cascade over its parent's
3. Cascade order (most specific wins): inline `<Styles>` > same dir `.fncss` > parent dir `.fncss` > root `.fncss`
4. Tokens are declared as `$name: value;` inside `:root { }` blocks (and `theme name { $token: value; }` for themes)
5. Themes activated via `@theme name` in frontmatter or global config
6. Token/theme resolution is compile-time only
7. Unknown CSS properties → hard build error; unknown classes → silently skipped
8. Mapped GPUI subset: padding/margin/gap, flex/grid, size/min/max, background/color/border/radius/shadow, typography, overflow, position, cursor

## #[fncc::command] levels

| Args | Signature |
|---|---|
| 0 | `fn handler()` |
| 1 | `fn handler(&ClickEvent)` |
| 2 | `fn handler(&mut State, &mut Context<State>)` |

Generates a `__fncc_cmd_{name}` trampoline. Commands must return `()`.

## Framework quirks

- State interpolation uses `state.field` in `.fui`; codegen strips `state.` prefix to `self.field`
- Only `Stack`, `Text`, `Button` have special codegen; unknown elements fall back to `div()` with `.attr()` calls
- `onclick` in stateful components uses entity handle pattern (`handle.update(cx, ...)`)
- `fncc-runtime` re-exports everything from `gpui` — use `use fncc::*` in apps

## Release workflow (changeset-based)

1. Add a change declaration: `cargo run -p xtask -- change` (creates `.changes/{crate}-{timestamp}.md`)
2. Validate: `cargo run --locked -p xtask -- check`
3. On push to main, CI auto-creates a release PR or publishes to crates.io

Changeset format:
```markdown
---
fncc-runtime: minor
---

- Description for changelog
```