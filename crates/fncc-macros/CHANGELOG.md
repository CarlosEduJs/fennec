# Changelog - fncc-macros

All notable changes to `fncc-macros` crate will be documented in this file.
## [0.1.2] - 2026-07-30

- Add #[derive(Props)] marker derive macro for typed component props

## [0.1.1] - 2026-07-29

- Added comprehensive unit tests for internal helpers: `extract_param_type`, `extract_state_type`, `is_unit`, and command level validation

## [0.1.0] - 2026-07-28

- `#[fncc::command]` — proc-macro that marks functions as GPUI command handlers, supporting three levels of injection: no arguments, event reference (`&ClickEvent`), and mutable state + context (`&mut S`, `&mut Context<S>`).
- `#[fncc::render]` — proc-macro that marks render functions, supporting stateless, stateful (`&mut S`, `&mut ViewContext<S>`), and derived (`&S`) forms.
- `CommandResult` — unified return type for command handlers.

