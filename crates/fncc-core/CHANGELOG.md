# Changelog - fncc-core

All notable changes to `fncc-core` crate will be documented in this file.
## [0.1.0] - 2026-07-28

- Parser for `.fui` (fncc UI) files, converting the declarative syntax into an AST.
- `codegen` module that generates Rust code from a `.fui` file's AST.
- `config` module for fncc configuration management.
- `generate_all()` entry point: parses all `.fui` files in a directory and writes the generated code to a single output file.

