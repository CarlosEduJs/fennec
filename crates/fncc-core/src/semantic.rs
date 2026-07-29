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
        }
    }
}

#[derive(Debug, Default)]
pub struct SemanticDb {
    pub commands: HashMap<String, CommandDef>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Analyze Rust source files in `src_dir` looking for `#[fncc::command]` functions.
pub fn analyze_rs_files(src_dir: &Path) -> Result<SemanticDb, anyhow::Error> {
    let mut db = SemanticDb::default();
    let mut files = Vec::new();
    collect_rs_files(src_dir, &mut files)?;

    for path in files {
        let content = std::fs::read_to_string(&path).map_err(|e| anyhow::anyhow!("failed to read {:?}: {e}", path))?;
        let file_name = path.to_string_lossy().to_string();
        let commands = extract_commands(&content, &file_name);
        for cmd in commands {
            db.commands.entry(cmd.name.clone()).or_insert(cmd);
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
    for item in &syntax.items {
        let func = match item {
            Item::Fn(f) => f,
            _ => continue,
        };

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

    commands
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
}
