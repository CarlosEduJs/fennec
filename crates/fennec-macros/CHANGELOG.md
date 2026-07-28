# Changelog - fennec-macros

All notable changes to `fennec-macros` crate will be documented in this file.
## [0.1.0] - 2026-07-28

- `#[fennec::command]` — proc-macro that marks functions as GPUI command handlers, supporting three levels of injection: no arguments, event reference (`&ClickEvent`), and mutable state + context (`&mut S`, `&mut Context<S>`).
- `#[fennec::render]` — proc-macro that marks render functions, supporting stateless, stateful (`&mut S`, `&mut ViewContext<S>`), and derived (`&S`) forms.
- `CommandResult` — unified return type for command handlers.

