# Changelog - fncc-styles

All notable changes to `fncc-styles` crate will be documented in this file.

## [0.1.0] - 2026-07-31

### Initial release

`fncc-styles` is a new workspace crate: a compile-time CSS-like style compiler for the fncc framework. It parses `.fncss` files and `<Styles>` blocks, resolves tokens and themes, and maps the supported CSS subset to GPUI styled-call chains.

#### What's included

- **Parser** (`css_parser.rs`) — parses `:root { $token: value }`, `theme name { ... }`, class rules, inline `<Styles>` blocks, and `@font-face { font-family; src: url(...) }`.
- **Token/theme resolution** (`lib.rs`) — `resolve()` maps a class/inline-style declaration set to GPUI method calls, substituting `$token` values and honoring the active theme.
- **Cascade merging** — `merge()` combines multiple sheets with last-writer-wins per property, used by `fncc-core` to build the directory cascade.
- **GPUI mapping** (`gpui_map.rs`) — supported properties: padding/margin/gap (incl. `%` → `relative()`), flex/grid, size/min/max, background/color/border/radius/shadow, typography (size, weight, `font-family`), overflow, position, cursor, `align-items`, `justify-content`.

#### Example

```css
:root {
  $primary: #0066cc;
  $radius: 8px;
}
.container {
  padding: 16px;
  background: $primary;
  border-radius: $radius;
}
```

Resolves to GPUI calls such as:

```rust
.p(px(16.)).bg(rgba(0x0066ccff)).rounded(px(8.))
```

#### Design decisions

- **Compile-time resolution only** — no runtime token machinery; tokens are substituted during codegen.
- **Hard errors for unknown properties** — a misspelled property fails the build; unknown classes are ignored.
- **Fonts** — `@font-face` src paths are stored per sheet and handed to `fncc-core`, which copies and embeds the font files.

