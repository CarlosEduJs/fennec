# Fennec 🦊

> Style Astro → GPUI Native

Fennec is a compiler/framework that translates a declarative syntax inspired by Astro into native Rust applications, using [GPUI](https://www.gpui.rs/) — the rendering engine behind Zed — as the runtime.

Write UI with a syntax that feels familiar to frontend developers. Compile it into a native binary. No Electron, no WebView, GPU-accelerated rendering, low memory footprint.

> **Status:** early design / POC phase. Not yet usable in production. 

---

## Why Fennec

- **Declarative components** with a `.fui`-like syntax
- **Native binaries** — no Electron, no WebView
- **GPU rendering** and low memory usage via GPUI
- **Hot reload** during development
- **Clear separation** between UI (`.fui`) and logic (`.rs`)
- **No hidden magic** — the compiler passes through real Rust wherever possible, so Rust Analyzer, autocomplete, and safe refactors keep working

## Example

**`App.fui`**
```fui
<App title="My application">
  <Stack direction="vertical" gap="12">
    <Text size="xl">
      Hello world
    </Text>

    <Button onclick={handleClick}>
      Click here
    </Button>
  </Stack>
</App>
```

**Compiles to (Rust + GPUI)**
```rust
App::new()
    .child(
        stack()
            .gap(12)
            .child(text("Hello world"))
            .child(button("Click here"))
    )
```

## Project structure

```
my-app/
├── src/
│   ├── ui/
│   │   ├── App.fui
│   │   └── components/
│   │       └── Button.fui
│   │
│   ├── lib.rs        ← Rust logic
│   └── main.rs
│
├── fncc.config.toml
├── Cargo.toml
└── package.json (?)
```

## Configuration

Fennec keeps UI/dev metadata (`fncc.config.toml`) separate from your dependencies (`Cargo.toml`) — no config file generates the other.

```toml
[package]
name = "my-app"
version = "0.1.0"

[paths]
ui = "src/ui"             # where .fui files live
lib = "src"                # where Rust logic lives
output = "target/fennec"   # generated intermediate .rs files

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

## Core concepts

### The frontmatter is real Rust
The block between `---` in a `.fui` file isn't a custom DSL — it's real Rust, copied straight into the generated `.rs`. That means compile-time type-checking and native Rust Analyzer support for free.

### Commands, not string-bound magic
UI events are wired to plain Rust functions marked with `#[fennec::command]`, inspired by Tauri:

```rust
#[fennec::command]
pub fn handle_click(state: &mut CounterState, cx: &mut Context<CounterState>) {
    state.count += 1;
    cx.notify();
}
```

```fui
<Button onclick="handle_click">Click here</Button>
```

Command names in markup are plain strings, so Rust Analyzer can't autocomplete or rename them there — the compiler validates command names at build time instead and fails with a clear error if one doesn't exist.

### Explicit reactivity, explicit state
No implicit "variable changes → UI updates" magic. You call `cx.notify()` yourself, and state lives in plain Rust structs you declare and pass explicitly to components. This leans on GPUI's own `Model`/`View` reactivity model instead of inventing a new one — trading a bit of convenience for predictability and easier debugging in larger apps.

### Rust-style imports
The frontmatter uses real `use crate::...`, not JS/Astro-style `import` paths — one less thing for the compiler to resolve, and Rust Analyzer understands it natively.

> Open question: should `.fui` component imports (`Button`, `Stack`, etc.) follow this same `use` pattern, or stay file-path based like Astro? Not decided yet.

## Roadmap (Phase 0 — POC)

1. Define a minimal syntax subset (`Stack`, `Text`, `Button`, `App` — 4–5 components)
2. Hand-translate one `.fui` example to Rust + GPUI, no parser, to validate the target API
3. Build the parser (candidate: [`pest`](https://pest.rs/))
4. Automate parser → codegen
5. Validate the command system with Level 1 (no args) and Level 2 (native GPUI event) arguments
6. Validate explicit state (Level 3) with a simple counter

### Known open problems
- **Hot reload for compiled Rust**: candidates are [`hot-lib-reloader`](https://github.com/rksm/hot-lib-reloader-rs) (dylib reload) for dev, or a fast recompile + restart flow using incremental compilation.
- **`.fui` component imports**: `use`-style vs. Astro-style file paths — still open.

## Related projects

| Project | Relation |
|---|---|
| [Dioxus](https://dioxuslabs.com/) | Closest spiritual competitor — JSX-like Rust macros, targets native/web/mobile |
| [Slint](https://slint.dev/) | Own declarative DSL, compiles to native with GPU rendering |
| [Tauri](https://tauri.app/) | Inspired Fennec's `#[command]` model (and its string-bridge trade-off) |
| [Zed](https://zed.dev/) / GPUI | GPUI used directly with pure Rust, no DSL — Fennec is a declarative syntax layer on top |

## Contributing

This project is in early design. Issues and design discussions are welcome — especially on the open questions above.

## License

MIT License. See [LICENSE](LICENSE) for details.