//! fncc runtime — re-exports GPUI and provides convenience wrappers.
pub use gpui::*;
pub use gpui::prelude::*;

/// A vertical stack (flex column) component.
pub fn stack() -> Div {
    div().flex().flex_col()
}

/// A horizontal stack (flex row) component.
pub fn h_stack() -> Div {
    div().flex()
}
