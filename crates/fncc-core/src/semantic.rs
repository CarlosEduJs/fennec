use std::collections::HashMap;
use std::path::Path;
use syn::{Attribute, FnArg, Item, Type};

#[derive(Debug, Clone, PartialEq)]
pub enum CommandLevel {
    Level1,
    Level2,
    Level3,
}

#[derive(Debug, Clone)]
pub struct CommandDef {
    pub name: String,
    pub level: CommandLevel,
    pub state_type: Option<String>,
    pub file: String,
}

#[derive(Debug, Clone)]
pub struct ComponentDef {
    pub name: String,
    pub render_fn: String,
    pub source_path: String,
    pub props_type: Option<String>,
    pub props_fields: Vec<PropField>,
}

#[derive(Debug, Clone)]
pub struct PropField {
    pub name: String,
    pub type_expr: String,
    pub is_optional: bool,
}

#[derive(Debug)]
pub enum Diagnostic {
    CommandNotFound {
        command: String,
        fui_file: String,
    },
    StateTypeConflict {
        fui_file: String,
        declared: String,
        inferred: String,
    },
    StateTypeMismatch {
        fui_file: String,
        types: Vec<String>,
    },
    DuplicateCommand {
        name: String,
        first_file: String,
        second_file: String,
    },
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Diagnostic::CommandNotFound { command, fui_file } => {
                write!(
                    f,
                    "in '{fui_file}': command `onclick=\"{command}\"` references `#[fncc::command] fn {command}()` which was not found in any Rust source file"
                )
            }
            Diagnostic::StateTypeConflict {
                fui_file,
                declared,
                inferred,
            } => {
                write!(
                    f,
                    "in '{fui_file}': `@state {declared}` conflicts with inferred state type `{inferred}` from command signatures — remove `@state` or align the types"
                )
            }
            Diagnostic::StateTypeMismatch { fui_file, types } => {
                write!(
                    f,
                    "in '{fui_file}': commands reference multiple state types ({}) — a component can only have one state type",
                    types.join(", ")
                )
            }
            Diagnostic::DuplicateCommand {
                name,
                first_file,
                second_file,
            } => {
                write!(
                    f,
                    "duplicate `#[fncc::command] fn {name}()` found in '{first_file}' and '{second_file}' — command names must be unique"
                )
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct SemanticDb {
    pub commands: HashMap<String, CommandDef>,
    pub components: HashMap<String, ComponentDef>,
    pub props_types: HashMap<String, Vec<PropField>>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Analyze Rust source files in `src_dir` looking for `#[fncc::command]` functions
/// and `#[derive(Props)]` structs.
pub fn analyze_rs_files(src_dir: &Path) -> Result<SemanticDb, anyhow::Error> {
    let mut db = SemanticDb::default();
    let mut files = Vec::new();
    collect_rs_files(src_dir, &mut files)?;

    for path in files {
        let content = std::fs::read_to_string(&path).map_err(|e| anyhow::anyhow!("failed to read {:?}: {e}", path))?;
        let file_name = path.to_string_lossy().to_string();
        let commands = extract_commands(&content, &file_name);
        for cmd in commands {
            if let Some(existing) = db.commands.get(&cmd.name) {
                db.diagnostics.push(Diagnostic::DuplicateCommand {
                    name: cmd.name.clone(),
                    first_file: existing.file.clone(),
                    second_file: cmd.file,
                });
            } else {
                db.commands.insert(cmd.name.clone(), cmd);
            }
        }

        let props = extract_props_types(&content, &file_name);
        for (name, fields) in props {
            db.props_types.entry(name).or_insert(fields);
        }
    }

    Ok(db)
}

fn collect_rs_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<(), anyhow::Error> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).map_err(|e| anyhow::anyhow!("failed to read dir {:?}: {e}", dir))? {
        let entry = entry.map_err(|e| anyhow::anyhow!("failed to read entry: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn extract_commands(content: &str, file_name: &str) -> Vec<CommandDef> {
    let syntax = match syn::parse_file(content) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };

    let mut commands = Vec::new();
    extract_from_items(&syntax.items, file_name, &mut commands);
    commands
}

fn extract_from_items(items: &[Item], file_name: &str, commands: &mut Vec<CommandDef>) {
    for item in items {
        match item {
            Item::Fn(func) => {
                if !is_command_attr(&func.attrs) {
                    continue;
                }

                let name = func.sig.ident.to_string();
                let arg_count = func.sig.inputs.len();

                let (level, state_type) = match arg_count {
                    0 => (CommandLevel::Level1, None),
                    1 => (CommandLevel::Level2, None),
                    2 => {
                        let st = extract_state_type_from_first_arg(&func.sig.inputs);
                        (CommandLevel::Level3, st)
                    }
                    _ => continue,
                };

                commands.push(CommandDef {
                    name,
                    level,
                    state_type,
                    file: file_name.to_string(),
                });
            }
            Item::Mod(m) => {
                if let Some((_, nested_items)) = &m.content {
                    extract_from_items(nested_items, file_name, commands);
                }
            }
            _ => {}
        }
    }
}

fn is_command_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let path = attr.path();
        match path.segments.len() {
            1 => path.segments.last().is_some_and(|s| s.ident == "command"),
            2 => path.segments[0].ident == "fncc" && path.segments[1].ident == "command",
            _ => false,
        }
    })
}

/// Extract props type fields from structs with `#[derive(Props)]`.
/// Returns a map of struct name → fields.
fn extract_props_types(content: &str, file_name: &str) -> HashMap<String, Vec<PropField>> {
    let syntax = match syn::parse_file(content) {
        Ok(file) => file,
        Err(_) => return HashMap::new(),
    };

    let mut result = HashMap::new();
    collect_props_from_items(&syntax.items, file_name, &mut result);
    result
}

#[allow(clippy::only_used_in_recursion)]
fn collect_props_from_items(items: &[Item], _file_name: &str, result: &mut HashMap<String, Vec<PropField>>) {
    for item in items {
        match item {
            Item::Struct(s) => {
                if !has_props_derive(&s.attrs) {
                    continue;
                }
                if result.contains_key(&s.ident.to_string()) {
                    continue;
                }
                let fields = match &s.fields {
                    syn::Fields::Named(n) => n.named.iter().filter_map(extract_prop_field).collect(),
                    _ => continue,
                };
                result.insert(s.ident.to_string(), fields);
            }
            Item::Mod(m) => {
                if let Some((_, nested_items)) = &m.content {
                    collect_props_from_items(nested_items, _file_name, result);
                }
            }
            _ => {}
        }
    }
}

fn has_props_derive(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| attr.path().is_ident("derive") && attr.parse_args_with(PropsDerive::parse_multi).is_ok())
}

struct PropsDerive;

impl PropsDerive {
    /// Accept `Props` or `fncc::Props` (and skip non-matching idents in `derive(...)`).
    fn parse_multi(input: syn::parse::ParseStream) -> syn::Result<Self> {
        while !input.is_empty() {
            // Try a path like `fncc::Props` or just `Props`
            if input.peek(syn::Ident) {
                let path: syn::Path = input.parse()?;
                if path.segments.last().is_some_and(|s| s.ident == "Props") {
                    return Ok(PropsDerive);
                }
            } else {
                return Err(input.error("expected identifier or path"));
            }
            // Skip optional comma
            let _ = input.parse::<syn::Token![,]>();
        }
        Err(input.error("expected `Props` or `fncc::Props` in derive list"))
    }
}

fn extract_prop_field(field: &syn::Field) -> Option<PropField> {
    let name = field.ident.as_ref()?.to_string();
    let ty = &field.ty;
    let type_expr = quote::quote!(#ty).to_string();
    let is_optional = is_option_type(ty);
    Some(PropField {
        name,
        type_expr,
        is_optional,
    })
}

/// Detect whether a type is `Option<T>` (with any inner type).
fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
    {
        return segment.ident == "Option";
    }
    false
}

fn extract_state_type_from_first_arg(inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>) -> Option<String> {
    let first = inputs.first()?;
    let typed = match first {
        FnArg::Typed(t) => t,
        _ => return None,
    };

    let ref_type = match typed.ty.as_ref() {
        Type::Reference(r) if r.mutability.is_some() => r,
        _ => return None,
    };

    match ref_type.elem.as_ref() {
        Type::Path(type_path) => type_path.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_commands_empty_file() {
        let cmds = extract_commands("fn main() {}", "test.rs");
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_extract_level1_command() {
        let src = "#[fncc::command]\nfn greet() { println!(\"hi\"); }";
        let cmds = extract_commands(src, "test.rs");
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].name, "greet");
        assert_eq!(cmds[0].level, CommandLevel::Level1);
        assert!(cmds[0].state_type.is_none());
    }

    #[test]
    fn test_extract_level2_command() {
        let src = "#[fncc::command]\nfn handle_click(_: &ClickEvent) {}";
        let cmds = extract_commands(src, "test.rs");
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].name, "handle_click");
        assert_eq!(cmds[0].level, CommandLevel::Level2);
        assert!(cmds[0].state_type.is_none());
    }

    #[test]
    fn test_extract_level3_command() {
        let src =
            "#[fncc::command]\nfn inc(state: &mut CounterState, cx: &mut Context<CounterState>) { state.count += 1; }";
        let cmds = extract_commands(src, "test.rs");
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].name, "inc");
        assert_eq!(cmds[0].level, CommandLevel::Level3);
        assert_eq!(cmds[0].state_type.as_deref(), Some("CounterState"));
    }

    #[test]
    fn test_extract_multiple_commands() {
        let src = r#"
#[fncc::command]
fn a() {}

#[fncc::command]
fn b(_: &ClickEvent) {}

#[fncc::command]
fn c(s: &mut AppState, cx: &mut Context<AppState>) {}
"#;
        let cmds = extract_commands(src, "test.rs");
        assert_eq!(cmds.len(), 3);
    }

    #[test]
    fn test_extract_non_command_ignored() {
        let src = r#"
#[derive(Default)]
struct Foo {}

fn bar() {}
"#;
        let cmds = extract_commands(src, "test.rs");
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_diagnostic_display_command_not_found() {
        let d = Diagnostic::CommandNotFound {
            command: "foo".into(),
            fui_file: "App.fui".into(),
        };
        let msg = d.to_string();
        assert!(msg.contains("foo"));
        assert!(msg.contains("App.fui"));
        assert!(msg.contains("not found"));
    }

    #[test]
    fn test_diagnostic_display_state_conflict() {
        let d = Diagnostic::StateTypeConflict {
            fui_file: "App.fui".into(),
            declared: "A".into(),
            inferred: "B".into(),
        };
        let msg = d.to_string();
        assert!(msg.contains("@state A"));
        assert!(msg.contains("B"));
    }

    #[test]
    fn test_analyze_rs_files_empty_dir() {
        let dir = std::env::temp_dir().join("fncc_semantic_test_empty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = analyze_rs_files(&dir).unwrap();
        assert!(db.commands.is_empty());
        assert!(db.diagnostics.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_analyze_rs_files_finds_command() {
        let dir = std::env::temp_dir().join("fncc_semantic_test_cmd");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("main.rs"),
            "#[fncc::command]\nfn inc(s: &mut S, cx: &mut Context<S>) {}\n",
        )
        .unwrap();
        let db = analyze_rs_files(&dir).unwrap();
        assert_eq!(db.commands.len(), 1);
        assert_eq!(db.commands.get("inc").unwrap().level, CommandLevel::Level3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_analyze_rs_files_ignores_non_rs_files() {
        let dir = std::env::temp_dir().join("fncc_semantic_test_ignore");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("not_rust.txt"), "not a rust file").unwrap();
        let db = analyze_rs_files(&dir).unwrap();
        assert!(db.commands.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_analyze_rs_files_recursive() {
        let dir = std::env::temp_dir().join("fncc_semantic_test_recursive");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("nested").join("cmds.rs"), "#[fncc::command]\nfn a() {}").unwrap();
        let db = analyze_rs_files(&dir).unwrap();
        assert_eq!(db.commands.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_extract_state_type_generic_context() {
        let src = "#[fncc::command]\nfn upd(state: &mut MyState, cx: &mut Context<MyState>) {}";
        let cmds = extract_commands(src, "test.rs");
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].state_type.as_deref(), Some("MyState"));
    }

    #[test]
    fn test_extract_state_type_module_qualified() {
        let src = "#[fncc::command]\nfn upd(state: &mut some::DeepState, cx: &mut Context<some::DeepState>) {}";
        let cmds = extract_commands(src, "test.rs");
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].state_type.as_deref(), Some("DeepState"));
    }

    #[test]
    fn test_extract_syntax_error_skips_file() {
        let cmds = extract_commands("this is not valid rust @@@", "bad.rs");
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_extract_commands_from_inline_module() {
        let src = r#"
mod handlers {
    #[fncc::command]
    fn handle_click(_: &ClickEvent) {}

    mod nested {
        #[fncc::command]
        fn deep(state: &mut Inner, cx: &mut Context<Inner>) {}
    }
}

#[fncc::command]
fn top_level() {}
"#;
        let cmds = extract_commands(src, "test.rs");
        assert_eq!(cmds.len(), 3);
        let names: Vec<&str> = cmds.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"handle_click"));
        assert!(names.contains(&"deep"));
        assert!(names.contains(&"top_level"));

        // deep should have Inner as state type
        let deep = cmds.iter().find(|c| c.name == "deep").unwrap();
        assert_eq!(deep.state_type.as_deref(), Some("Inner"));
    }

    #[test]
    fn test_analyze_rs_files_duplicate_command_detected() {
        let dir = std::env::temp_dir().join("fncc_semantic_test_dup");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("a.rs"), "#[fncc::command]\nfn foo() {}\n").unwrap();
        std::fs::write(dir.join("b.rs"), "#[fncc::command]\nfn foo() {}\n").unwrap();

        let db = analyze_rs_files(&dir).unwrap();
        // Should still have foo (first one wins)
        assert_eq!(db.commands.len(), 1);
        assert!(db.commands.contains_key("foo"));
        // Should have a duplicate diagnostic
        assert_eq!(db.diagnostics.len(), 1);
        let diag = &db.diagnostics[0];
        match diag {
            Diagnostic::DuplicateCommand { name, .. } => assert_eq!(name, "foo"),
            other => panic!("expected DuplicateCommand, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_diagnostic_display_duplicate_command() {
        let d = Diagnostic::DuplicateCommand {
            name: "foo".into(),
            first_file: "a.rs".into(),
            second_file: "b.rs".into(),
        };
        let msg = d.to_string();
        assert!(msg.contains("duplicate"));
        assert!(msg.contains("foo"));
        assert!(msg.contains("a.rs"));
        assert!(msg.contains("b.rs"));
    }

    #[test]
    fn test_analyze_rs_files_unique_commands_no_diagnostics() {
        let dir = std::env::temp_dir().join("fncc_semantic_test_unique");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("a.rs"), "#[fncc::command]\nfn foo() {}\n").unwrap();
        std::fs::write(dir.join("b.rs"), "#[fncc::command]\nfn bar() {}\n").unwrap();

        let db = analyze_rs_files(&dir).unwrap();
        assert_eq!(db.commands.len(), 2);
        assert!(db.diagnostics.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_analyze_rs_files_duplicate_deterministic_first_wins() {
        // Regardless of file traversal order, the first definition encountered
        // is retained and the second is a diagnostic.
        let dir = std::env::temp_dir().join("fncc_semantic_test_dup_det");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("z_first.rs"), "#[fncc::command]\nfn cmd() {}\n").unwrap();
        std::fs::write(dir.join("a_second.rs"), "#[fncc::command]\nfn cmd() {}\n").unwrap();

        let db = analyze_rs_files(&dir).unwrap();
        assert_eq!(db.diagnostics.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
