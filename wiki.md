# fncc 🦊

> Like Astro → GPUI Native
> A compiler/framework that translates a declarative syntax inspired by Astro into native Rust applications, using GPUI as the rendering engine.

---

## 1. Overview

fncc lets you write user interfaces with a syntax familiar to frontend developers (declarative components, separation of UI and logic, hot reload), compiling to a native binary — no Electron, no WebView, GPU-accelerated rendering, low memory footprint via [GPUI](https://www.gpui.rs/) (the framework powering Zed).

### Developer experience goals
- Declarative components, `.fui`-like syntax
- Hot reload in dev
- Clear separation between UI (`.fui`) and logic (`.rs`)
- Component autocomplete, real-time errors, safe refactors

### Runtime goals
- Native binary
- GPU-accelerated rendering (GPUI)
- Low memory usage
- No Electron, no WebView

### Guiding principle
**No hidden magic.** Every design decision below prioritizes: Rust Analyzer working out of the box, explicit reactivity, safe refactors, and the compiler doing the minimum "magic translation" — preferring to pass through real Rust whenever possible.

---

## 2. Example

**Input (`.fui`):**
```fui
---
@state CounterState
---

<Stack direction="vertical" gap="12">
    <Text size="xl">Count: {state.count}</Text>
    <Button onclick="handle_click">+1</Button>
</Stack>
```

**Output (generated Rust + GPUI):**
```rust
impl Render for CounterState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let handle = cx.entity().downgrade();
        div()
            .gap(px(12.))
            .flex()
            .flex_col()
            .child(
                div()
                    .text_xl()
                    .child(format!("Count: {}", self.count))
            )
            .child(
                div()
                    .id("+1")
                    .cursor_pointer()
                    .on_click({
                        let handle = handle.clone();
                        move |_, _, cx| {
                            handle.update(cx, |this, cx| {
                                __fncc_cmd_handle_click(this, cx);
                            }).ok();
                        }
                    })
                    .child("+1")
            )
    }
}
```

**User writes in `.rs`:**
```rust
#[derive(Default)]
struct CounterState {
    count: i32,
}

#[fncc::command]
fn handle_click(state: &mut CounterState, cx: &mut Context<CounterState>) {
    state.count += 1;
    cx.notify();
}
```

---

## 3. Project structure

```
fncc/
├── crates/
│   ├── fncc-core/          # Parser (pest) + Codegen
│   │   ├── src/
│   │   │   ├── parser.rs     # pest grammar → AST
│   │   │   ├── codegen.rs    # AST → Rust+GPUI source
│   │   │   └── config.rs     # fncc.config.toml parsing
│   │   └── Cargo.toml
│   │
│   ├── fncc-macros/        # proc-macro crate
│   │   └── src/lib.rs        # #[fncc::command]
│   │
│   ├── fncc-runtime/       # Runtime re-exports (gpui + helpers)
│   │   └── src/lib.rs        # pub use gpui::* + convenience wrappers
│   │
│   └── fncc/               # Unified crate: use fncc::*
│       └── src/lib.rs        # Re-exports macros + runtime
│
├── apps/
│   └── example/              # Example app validating the POC
│       ├── src/
│       │   ├── ui/
│       │   │   └── App.fui
│       │   └── main.rs
│       ├── build.rs           # calls fncc_core::generate_all()
│       └── Cargo.toml
│
├── Cargo.toml                 # Workspace root
├── wiki.md
└── README.md
```

**Application project structure (for users):**
```
my-app/
├── src/
│   ├── ui/
│   │   └── App.fui
│   ├── main.rs
│   └── build.rs
├── Cargo.toml
└── fncc.config.toml
```

---

## 4. Design decisions

### 4.1 The frontmatter is real Rust code (with one extension)

The block between `---` markers in `.fui` is Rust code. The compiler copies it directly into the generated `.rs`. This gives:

- Zero runtime binding — everything resolved at compile-time by `rustc`
- Free type-checking
- Native Rust Analyzer support

**Extension — `@state` directive:** We added a single non-Rust directive to the frontmatter. `@state TypeName` tells the codegen which struct implements `Render` and holds the component's state. Lines starting with `@state` are stripped before emission.

```fui
---
@state CounterState
use crate::some_import;
---
```

This is a pragmatic concession for the POC. A future version may infer the state type from the command signatures instead.

### 4.2 Commands via macro (`#[fncc::command]`)

Inspired by Tauri's `#[tauri::command]`. Functions handling UI events are explicitly marked:

```rust
#[fncc::command]
pub fn handle_click() {
    println!("clicked!");
}
```

The macro generates a **trampoline function** (`__fncc_cmd_{name}`) that adapts the user's function signature to match the GPUI handler the codegen expects. The trampoline name is deterministic, so the codegen can reference it without knowing the signature.

```fui
<Button onclick="handle_click">Click here</Button>
```

**Decision about Rust Analyzer and strings:** `"handle_click"` inside `.fui` is a plain string — no autocomplete, no rename tracking. **Conscious decision.** The developer is responsible for the correct name; the compiler validates at build time with a clear error if the command doesn't exist. No custom LSP during the POC phase.

### 4.3 Command arguments — three levels

**Level 1 — No argument:**
```rust
#[fncc::command]
pub fn handle_click() {
    println!("clicked!");
}
```
Macro generates a trampoline that matches GPUI's `on_click` handler signature `(&ClickEvent, &mut Window, &mut App)` and discards all arguments.

**Level 2 — Native GPUI event:**
```rust
#[fncc::command]
pub fn handle_click(event: &ClickEvent) {
    println!("clicked at {:?}", event.position());
}
```
Macro inspects the function signature via `syn` and generates a trampoline that passes the event reference.

**Level 3 — State + context (explicit reactivity):**
```rust
#[fncc::command]
pub fn handle_click(state: &mut CounterState, cx: &mut Context<CounterState>) {
    state.count += 1;
    cx.notify(); // developer decides when to notify — no magic
}
```
Macro extracts the state type from the first parameter and generates `fn trampoline(state: &mut StateType, cx: &mut Context<StateType>)`. The codegen wraps the call in GPUI's `handle.update(cx, |this, cx| { trampoline(this, cx) })` pattern.

**Implementation detail:** the codegen runs at build-script time (`build.rs`), before `rustc`. It cannot inspect Rust function signatures. The trampoline convention bridges this gap: the macro generates the adapter, the codegen generates the invocation using a deterministic name.

### 4.4 Codegen strategy: hybrid `build.rs` + `include!()`

**Decision:** The compiler runs as a Cargo build script (`build.rs`). It reads `.fui` files from `src/ui/`, parses them, and writes generated `.rs` code to `$OUT_DIR/generated.rs`. The application's `main.rs` includes this file via `include!(concat!(env!("OUT_DIR"), "/generated.rs"))`.

Compared to alternatives:
- **`proc_macro`** (compile-time macro): harder to debug, couples codegen to the Rustc compilation pipeline.
- **External CLI tool** (`fncc build`): better for DX but adds a build step outside Cargo.

The hybrid approach gives us fast iteration during the POC while keeping the Cargo-native workflow. A dedicated CLI (`fncc build`) can be built later on top of `fncc-core`.

### 4.5 Reactivity: explicit, not implicit

**Decision:** fncc uses explicit reactivity — the developer calls `cx.notify()` manually. No implicit "variable changed → UI updates" magic.

**Reason:** debuggability. Implicit reactivity is great on day one but becomes a nightmare to debug in larger apps ("why did this component re-render?"). This is aligned with the fncc target audience: developers willing to trade "easy" for "predictable and native."

**Positive consequence:** fncc doesn't need to invent a reactivity system. GPUI's `Model`/`View` + `cx.notify()` already delivers this behavior. The compiler's job is only to generate the right call in the right place.

Interpolation like `{state.count}` in markup is lowered at **compile-time**: code generation emits `format!("{}", self.count)` into the `render` method, while `self.count` is evaluated at runtime during rendering and recomputed after `cx.notify()` triggers a re-render.

### 4.6 State: explicit, not implicit

State lives in normal Rust structs declared by the developer. No global state magic. The struct implements `Render` (the generated `impl Render for StateType` block), and the developer passes it directly to `cx.new(|_| StateType::default())`.

```rust
#[derive(Default)]
struct CounterState {
    count: i32,
}
```

```fui
---
@state CounterState
---

<Stack>
  <Text>{state.count}</Text>
  <Button onclick="handle_click">+1</Button>
</Stack>
```

### 4.7 Imports: Rust-style `use`, not ES Modules

**Decision:** Frontmatter uses real `use crate::...` statements, not JS/Astro-style `import { X } from "../lib"`.

```fui
---
use crate::lib::{CounterState, handle_click};
---
```

**Reason:** consistency with "frontmatter is real Rust." The compiler doesn't need to resolve relative paths, there's no re-export ambiguity, and Rust Analyzer understands the line natively — it's copied almost verbatim to the generated `.rs`.

**Open question:** whether `.fui` component imports (`Button`, `Stack`) should follow the same `use` pattern or keep Astro-style file paths. **Not decided.**

### 4.8 Configuration: `fncc.config.toml` + `Cargo.toml` coexist

**Decision:** `fncc.config.toml` handles only UI/dev-experience metadata. `Cargo.toml` continues to exist normally, managed by the developer (dependencies, etc.). Clean separation, no magical generation of one from the other.

```toml
[paths]
ui = "src/ui"
lib = "src"
output = "target/fncc"

[app]
entry = "src/ui/App.fui"

[window]
title = "My application"
width = 800
height = 600
resizable = true

[dev]
hot_reload = true
watch = ["src/ui", "src"]
```

---

## 5. Related projects

- **Dioxus** — JSX-like in Rust macros, targets native/web/mobile. Closest spiritual competitor.
- **Slint** — Own declarative DSL, compiles to native with GPU rendering.
- **Tauri** — the `#[tauri::command]` + `invoke()` model that inspired fncc's command system (and shares the same fragility: a string-based bridge without type-checking).
- **Zed / GPUI** — GPUI used internally with pure Rust, no DSL. fncc is the first declarative syntax layer on top of it.

---

## 6. Roadmap (Phase 0 — POC)

### Completed
1. Define minimum syntax subset (Stack, Text, Button — 3 components)
2. Hand-translate `.fui` → Rust+GPUI to validate GPUI API
3. Write parser using `pest`
4. Automate parser → codegen via `build.rs` + `include!()`
5. `#[fncc::command]` Level 1 (no args) and Level 2 (GPUI event)
6. Explicit state (Level 3) with counter + `cx.notify()`
7. Command trampoline system (macro generates adapter, codegen calls it by name)

### Next up
- **`fncc` CLI** — standalone binary replacing `build.rs`, with `fncc build` and `fncc dev` (watch mode)
- **Multi-file components** — imports between `.fui` files (`Button.fui` referenced from `App.fui`)
- **More components** — `Input`, `Image`, `List`, `Scroll`
- **`@state` inference** — derive state type from command signatures instead of the `@state` directive
- **Hot reload** — `hot-lib-reloader` (dylib reload) or fast recompile + restart with incremental compilation

## 6. Semantic Analysis

Introduced in `fncc-core 0.4.0` — a build-time view into the Rust source that enables state inference, command validation, and future features like typed props and LSP.

### Architecture

```
┌─────────────────────┐
│  .rs files           │
│  (main.rs, etc.)     │──→ semantic::analyze_rs_files() → SemanticDb
└─────────────────────┘      ┌─────────────────────────┐
                             │  commands: HashMap       │
┌─────────────────────┐      │  diagnostics: Vec        │
│  .fui files          │──→  │  (future: components,    │
│  (src/ui/)           │      │   state_types, types)   │
└─────────────────────┘      └─────────────────────────┘
                                      │
                                      ▼
                             codegen::generate_with_imports()
                             (receives resolved_state_type)
```

### `GenerateOptions` API

Apps opt into semantic analysis by setting `src_dir` in `GenerateOptions`:

```rust
// build.rs — new API
fncc_core::generate_all_with_options(fncc_core::GenerateOptions {
    ui_dir: Path::new("src/ui"),
    out_file: &out_file,
    src_dir: Some(Path::new("src")),  // enables semantic analysis
}).unwrap();
```

The legacy `generate_all(ui_dir, out_file)` continues to work unchanged.

### What it does

1. **Scans `.rs` files** for `#[fncc::command]` annotated functions (recursive, skips unparseable files)
2. **Extracts command metadata** — name, level (1/2/3), state type (for Level 3)
3. **Hard-error validation** — every `onclick="cmd"` in `.fui` must have a matching `#[fncc::command] fn cmd()` in `.rs`. If not found, build fails immediately with a clear diagnostic.
4. **State inference** — when a `.fui` omits `@state`, the compiler infers the state type from Level 3 command signatures. If `@state` is present and conflicts with inference, build fails.
5. **Multiple state type check** — if commands in the same `.fui` reference different state types, build fails.

### Example: `semantic-app`

**`src/ui/App.fui`** — no `@state` directive:
```fui
<Stack direction="vertical" gap="12">
    <Text size="xl">Counter: {state.count}</Text>
    <Button onclick="increment">+1</Button>
</Stack>
```

**`src/main.rs`** — state type inferred from command:
```rust
#[derive(Default)]
struct CounterState { count: i32 }

#[fncc::command]
fn increment(state: &mut CounterState, cx: &mut Context<CounterState>) {
    state.count += 1;
    cx.notify();
}
```

The compiler infers `CounterState` as the component state type from the `increment` command signature. No `@state` needed.

### `SemanticDb` — compiler foundation

```rust
pub struct SemanticDb {
    pub commands: HashMap<String, CommandDef>,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct CommandDef {
    pub name: String,
    pub level: CommandLevel,  // Level1, Level2, or Level3
    pub state_type: Option<String>,
    pub file: String,
}
```

This structure is designed to grow: future versions will add `components`, `state_types`, `props`, and `types` fields, feeding into codegen, LSP metadata, documentation generation, and incremental compilation.

### Future phases

| Phase | Feature | Depends on |
|-------|---------|-----------|
| P1 | Command scanning + state inference | ✅ Done |
| P2 | Typed component props | `SemanticDb.components` + `.fui` attribute validation |
| P3 | LSP metadata generation | `SemanticDb.to_lsp_metadata()` serialization |
| P4 | Incremental compilation | `SemanticDb` cache + file-change tracking |

---

### Known problems, still open
- **Hot reload in compiled Rust:** candidates — `hot-lib-reloader` (dylib reload) for dev, or fast recompile + restart with incremental compilation.
- **Imports for `.fui` components:** `use`-style (Rust) vs. file-path (Astro) — still undecided.
- **GPUI version lock:** the runtime pins a specific GPUI version (currently 0.2.2). GPUI is still evolving rapidly; breaking changes from upstream require manual updates to `fncc-runtime`.
