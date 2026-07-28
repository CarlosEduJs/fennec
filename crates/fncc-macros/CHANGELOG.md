# Changelog - fncc-macros

All notable changes to `fncc-macros` crate will be documented in this file.
## [0.2.0] - 2026-07-28

- `#[fncc::command]` — proc-macro that marks functions as GPUI command handlers, supporting three levels of injection: no arguments, event reference (`&ClickEvent`), and mutable state + context (`&mut S`, `&mut Context<S>`).
- `#[fncc::render]` — proc-macro that marks render functions, supporting stateless, stateful (`&mut S`, `&mut ViewContext<S>`), and derived (`&S`) forms.
- `CommandResult` — unified return type for command handlers.

