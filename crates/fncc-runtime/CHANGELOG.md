# Changelog - fncc-runtime

All notable changes to `fncc-runtime` crate will be documented in this file.
## [0.2.0] - 2026-07-31

- Implement the runtime support for Native File-Based Routing (NFBR).
  This includes the stack-based `Router` structure, deep link URI parsing, and navigation methods.
  It closes issue [#22](https://github.com/CarlosEduJs/fncc/issues/22).

  ### Details

  1. **Stack-based Router**: Maintains a navigation stack to support push, pop, and replace operations natively in GPUI desktop apps.
     
     ```rust
     pub struct Router<R> {
         stack: Vec<R>,
     }
     
     impl<R> Router<R> {
         pub fn new(initial: R) -> Self;
         pub fn push(&mut self, route: R);
         pub fn pop(&mut self) -> Option<R>;
         pub fn replace(&mut self, route: R);
         pub fn current(&self) -> &R;
     }
     ```

  2. **Deep Link Parsing**: Added helper trait and parser logic to convert deep link URIs (e.g. `myapp://users/alice`) into typed routes at runtime.

## [0.1.0] - 2026-07-28

- Full re-export of `gpui` and its prelude for convenient access.
- `stack()` — flex column component (`div().flex().flex_col()`).
- `h_stack()` — flex row component (`div().flex()`).

