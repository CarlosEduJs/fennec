//! fncc runtime — re-exports GPUI and provides convenience wrappers.
pub use gpui::prelude::*;
pub use gpui::*;

pub mod router;
pub use router::*;

/// A vertical stack (flex column) component.
pub fn stack() -> Div {
    div().flex().flex_col()
}

/// A horizontal stack (flex row) component.
pub fn h_stack() -> Div {
    div().flex()
}
