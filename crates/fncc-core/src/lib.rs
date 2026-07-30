pub mod codegen;
pub mod config;
pub mod parser;
pub mod semantic;

pub use parser::parse;

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

/// Recursively collect all .fui files under `dir`.
fn collect_fui_files(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    if !dir.is_dir() {
        return Ok(files);
    }
    for entry in std::fs::read_dir(dir).context("failed to read ui dir")? {
        let entry = entry.context("failed to read entry")?;
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_fui_files(&path)?);
        } else if path.extension().is_some_and(|ext| ext == "fui") {
            files.push(path);
        }
    }
    Ok(files)
}

/// Structured parsed file for two-pass processing.
struct ParsedFile {
    path: std::path::PathBuf,
    /// Path relative to ui_dir, forward-slash-separated, without extension.
    /// e.g. "components/Header"
    relative_stem: String,
    ast: parser::Document,
}

/// Configuration for the fncc compilation pipeline.
/// Use `GenerateOptions::new(ui_dir, out_file)` for default (legacy) behavior,
/// or set `src_dir` to enable semantic analysis (state inference, command validation).
pub struct GenerateOptions<'a> {
    pub ui_dir: &'a Path,
    pub out_file: &'a Path,
    /// If `Some`, enables semantic analysis (scans `.rs` files for `#[fncc::command]`).
    /// Expected to point to the Rust source directory (e.g. `"src/"`).
    pub src_dir: Option<&'a Path>,
}

impl<'a> GenerateOptions<'a> {
    pub fn new(ui_dir: &'a Path, out_file: &'a Path) -> Self {
        GenerateOptions {
            ui_dir,
            out_file,
            src_dir: None,
        }
    }
}

/// Legacy entry point: parse all .fui files under `ui_dir` and write
/// generated Rust code into `out_file`. No semantic analysis.
pub fn generate_all(ui_dir: &Path, out_file: &Path) -> Result<()> {
    generate_all_with_options(GenerateOptions::new(ui_dir, out_file))
}

/// Full entry point: parse `.fui` files and optionally run semantic analysis
/// (state inference, command validation) when `opts.src_dir` is `Some`.
pub fn generate_all_with_options(opts: GenerateOptions) -> Result<()> {
    let ui_dir = opts.ui_dir;
    let out_file = opts.out_file;
    let files = collect_fui_files(ui_dir)?;

    // Pass 1: Parse all files
    let mut parsed_files: Vec<ParsedFile> = Vec::new();
    for path in &files {
        let source = std::fs::read_to_string(path).with_context(|| format!("failed to read {:?}", path))?;
        let ast = parser::parse(&source).map_err(|e| anyhow::anyhow!("failed to parse {:?}: {e}", path))?;

        // Compute relative stem from ui_dir
        let relative_stem = match path.strip_prefix(ui_dir) {
            Ok(rel) => {
                let stem = rel.file_stem().unwrap_or_default();
                let parent = rel.parent().unwrap_or_else(|| std::path::Path::new(""));
                parent.join(stem).to_string_lossy().to_string()
            }
            Err(_) => path.file_stem().unwrap_or_default().to_string_lossy().to_string(),
        };
        let relative_stem = relative_stem.replace(std::path::MAIN_SEPARATOR_STR, "/");

        parsed_files.push(ParsedFile {
            path: path.clone(),
            relative_stem,
            ast,
        });
    }

    // Build import resolution index: "ui::path::Component" -> render function name
    let mut import_index: HashMap<String, String> = HashMap::new();
    for pf in &parsed_files {
        let ui_path = format!("ui::{}", pf.relative_stem.replace('/', "::"));
        let component_name = pf.relative_stem.split('/').next_back().unwrap_or("");
        let render_fn = format!("render_{}", codegen::to_snake_case(component_name));
        import_index.insert(ui_path, render_fn);
    }

    // Build props type index: render_fn_name -> props type name (e.g. "HeaderProps")
    // Only set when the component's template actually uses {props.xxx} interpolation.
    let mut render_fn_to_props: HashMap<String, Option<String>> = HashMap::new();
    for pf in &parsed_files {
        let component_name = pf.relative_stem.split('/').next_back().unwrap_or("");
        let render_fn = format!("render_{}", codegen::to_snake_case(component_name));
        let props_type = parser::uses_props_interpolation(&pf.ast.root)
            .then(|| {
                pf.ast.imports.iter().find_map(|imp| {
                    if matches!(imp.source, parser::ImportSource::PropsType) {
                        Some(imp.name.clone())
                    } else {
                        None
                    }
                })
            })
            .flatten();
        render_fn_to_props.insert(render_fn, props_type);
    }

    // Validate imports: every FuiPath import must exist in the index
    for pf in &parsed_files {
        for imp in &pf.ast.imports {
            if let parser::ImportSource::FuiPath(ref ui_path) = imp.source
                && !import_index.contains_key(ui_path)
            {
                anyhow::bail!(
                    "in '{}': import `{}` -> file not found at `{}.fui`",
                    pf.path.display(),
                    ui_path,
                    ui_path.strip_prefix("ui::").unwrap_or(ui_path)
                );
            }
        }
    }

    // Semantic analysis (optional): scan .rs files for #[fncc::command]
    let semantic_db = if let Some(src_dir) = opts.src_dir {
        let db = semantic::analyze_rs_files(src_dir)?;
        // Extraction-level diagnostics (e.g. duplicate commands) are hard errors
        if let Some(diag) = db.diagnostics.first() {
            anyhow::bail!("{}", diag);
        }
        Some(db)
    } else {
        None
    };

    // Per-file state type resolution: @state or inferred from commands
    let mut state_types: HashMap<usize, Option<String>> = HashMap::new();
    if let Some(ref db) = semantic_db {
        for (file_id, pf) in parsed_files.iter().enumerate() {
            let command_refs = parser::collect_commands(&pf.ast.root);

            let mut inferred_types: Vec<&str> = Vec::new();
            for cmd_name in &command_refs {
                let cmd = db.commands.get(cmd_name).ok_or_else(|| {
                    anyhow::anyhow!(
                        "in '{}': onclick=\"{}\" references #[fncc::command] fn {}() which was not found in any Rust source file",
                        pf.path.display(), cmd_name, cmd_name,
                    )
                })?;
                if let Some(ref st) = cmd.state_type
                    && !inferred_types.contains(&st.as_str())
                {
                    inferred_types.push(st);
                }
            }

            let resolved = match (pf.ast.state_type.as_ref(), inferred_types.as_slice()) {
                (Some(declared), []) => Some(declared.clone()),
                (Some(declared), [inferred]) if declared.as_str() == *inferred => Some(declared.clone()),
                (Some(declared), [inferred]) => anyhow::bail!(
                    "in '{}': @state {} conflicts with inferred state type {} from command signatures — remove @state or align the types",
                    pf.path.display(),
                    declared,
                    inferred,
                ),
                (Some(d), _) => anyhow::bail!(
                    "in '{}': commands reference multiple state types ({}) but @state {} was declared",
                    pf.path.display(),
                    inferred_types.join(", "),
                    d,
                ),
                (None, []) => None,
                (None, [inferred]) => Some((*inferred).to_string()),
                (None, _) => anyhow::bail!(
                    "in '{}': commands reference multiple state types ({}) — a component can only have one",
                    pf.path.display(),
                    inferred_types.join(", "),
                ),
            };

            state_types.insert(file_id, resolved);
        }
    }

    // Pass 2: Generate code with resolved imports and props
    let mut output = String::new();
    output.push_str("// Generated by fncc-core. Do not edit.\n\n");

    for (file_id, pf) in parsed_files.iter().enumerate() {
        let resolved: Vec<(&str, &str)> = pf
            .ast
            .imports
            .iter()
            .map(|imp| {
                let render_fn = match &imp.source {
                    parser::ImportSource::FuiPath(ui_path) => {
                        import_index.get(ui_path).map(|s| s.as_str()).unwrap_or("")
                    }
                    parser::ImportSource::Gpui | parser::ImportSource::PropsType => "",
                };
                (imp.name.as_str(), render_fn)
            })
            .collect();

        // Resolve import props: for each FuiPath import, find the target's props type
        let import_props: Vec<(&str, Option<&str>)> = pf
            .ast
            .imports
            .iter()
            .filter_map(|imp| {
                let render_fn = match &imp.source {
                    parser::ImportSource::FuiPath(ui_path) => import_index.get(ui_path),
                    _ => None,
                }?;
                let props = render_fn_to_props.get(render_fn).and_then(|p| p.as_deref());
                Some((imp.name.as_str(), props))
            })
            .collect();

        // This component's own props type — only set if the template actually
        // uses {props.xxx} interpolation (meaning it receives props, vs merely
        // referencing a PropsType import for a child component).
        let uses_props = parser::uses_props_interpolation(&pf.ast.root);
        let own_props_type = uses_props
            .then(|| {
                pf.ast.imports.iter().find_map(|imp| {
                    if matches!(imp.source, parser::ImportSource::PropsType) {
                        Some(imp.name.as_str())
                    } else {
                        None
                    }
                })
            })
            .flatten();

        // Validate props attributes against field definitions
        if let Some(ref db) = semantic_db {
            validate_props_usage(&pf.path, &pf.ast.root, &import_props, &db.props_types)?;
        }

        let component_name = pf.relative_stem.split('/').next_back();
        let resolved_state = state_types.get(&file_id).and_then(|s| s.as_deref());

        // Stateful components cannot receive props
        if resolved_state.is_some() && own_props_type.is_some() {
            anyhow::bail!(
                "in '{}': component cannot have both state and props — props are only supported on stateless components",
                pf.path.display(),
            );
        }

        let prop_fields = semantic_db.as_ref().map(|db| &db.props_types);
        let generated = codegen::generate_with_imports(
            &pf.ast,
            file_id,
            &resolved,
            component_name,
            resolved_state,
            own_props_type,
            &import_props,
            prop_fields,
        );
        output.push_str(&generated);
        output.push('\n');
    }

    std::fs::write(out_file, &output).context("failed to write generated file")?;
    Ok(())
}

/// Validate that all attributes on imported components with props match their field definitions:
/// - Unknown attributes cause a hard error
/// - Missing required (non-Option) fields cause a hard error
fn validate_props_usage(
    file_path: &Path,
    el: &parser::Element,
    import_props: &[(&str, Option<&str>)],
    props_types: &HashMap<String, Vec<semantic::PropField>>,
) -> Result<()> {
    if let Some((_, Some(props_type_name))) = import_props.iter().find(|(name, _)| name == &el.name)
        && let Some(fields) = props_types.get(*props_type_name)
    {
        for (attr_name, _) in &el.attrs {
            if !fields.iter().any(|f| f.name == *attr_name) {
                let available: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
                anyhow::bail!(
                    "in '{}': component `{}` has no prop `{}` — available props: {}",
                    file_path.display(),
                    el.name,
                    attr_name,
                    available.join(", "),
                );
            }
        }
        for field in fields {
            if !field.is_optional && !el.attrs.iter().any(|(name, _)| name == &field.name) {
                anyhow::bail!(
                    "in '{}': component `{}` requires prop `{}` (type {})",
                    file_path.display(),
                    el.name,
                    field.name,
                    field.type_expr,
                );
            }
        }
    }
    for child in &el.children {
        if let parser::Node::Element(child_el) = child {
            validate_props_usage(file_path, child_el, import_props, props_types)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static LIB_TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn test_dir() -> (PathBuf, PathBuf) {
        let id = LIB_TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("fncc_lib_test_{id}"));
        let ui_dir = dir.join("ui");
        std::fs::create_dir_all(&ui_dir).unwrap();
        (dir, ui_dir)
    }

    #[test]
    fn test_generate_all_creates_output_file() {
        let (dir, ui_dir) = test_dir();
        std::fs::write(ui_dir.join("App.fui"), "<Text>hello</Text>").unwrap();
        let out_file = dir.join("out.rs");

        generate_all(&ui_dir, &out_file).unwrap();

        assert!(out_file.exists());
        let content = std::fs::read_to_string(&out_file).unwrap();
        assert!(content.contains("// Generated by fncc-core. Do not edit."));
        // Function name uses file stem ("App"), not root element name
        assert!(content.contains("pub fn render_app()"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_generate_all_ignores_non_fui_files() {
        let (dir, ui_dir) = test_dir();
        std::fs::write(ui_dir.join("App.fui"), "<A></A>").unwrap();
        std::fs::write(ui_dir.join("notes.txt"), "not a fui").unwrap();
        std::fs::write(ui_dir.join("main.rs"), "fn main() {}").unwrap();
        let out_file = dir.join("out.rs");

        generate_all(&ui_dir, &out_file).unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        // Function name uses file stem ("App"), not root element name
        assert!(content.contains("pub fn render_app()"));
        assert!(!content.contains("notes"));
        assert!(!content.contains("main"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_generate_all_with_import() {
        let (dir, ui_dir) = test_dir();

        // Header.fui — stateless component
        std::fs::write(ui_dir.join("Header.fui"), "<Text size=\"xl\">Welcome</Text>").unwrap();
        // App.fui — imports Header
        std::fs::write(
            ui_dir.join("App.fui"),
            "---\nuse ui::Header;\n---\n<Stack><Header /></Stack>",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        generate_all(&ui_dir, &out_file).unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        // Header generates its own render function
        assert!(content.contains("pub fn render_header()"));
        // App uses the imported component
        assert!(content.contains("render_header()"));
        // No ui:: import leaks into generated Rust
        assert!(!content.contains("use ui::"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_generate_all_with_grouped_imports() {
        let (dir, ui_dir) = test_dir();

        std::fs::create_dir_all(ui_dir.join("components")).unwrap();
        std::fs::write(
            ui_dir.join("components").join("Button.fui"),
            "<Button onclick=\"handle_click\">Click</Button>",
        )
        .unwrap();
        std::fs::write(
            ui_dir.join("components").join("Card.fui"),
            "<Stack><Text>card</Text></Stack>",
        )
        .unwrap();
        std::fs::write(
            ui_dir.join("App.fui"),
            "---\nuse ui::components::{Button, Card};\n---\n<Stack><Card /><Button /></Stack>",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        generate_all(&ui_dir, &out_file).unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        assert!(content.contains("pub fn render_button()"));
        assert!(content.contains("pub fn render_card()"));
        assert!(content.contains("render_card()"));
        assert!(content.contains("render_button()"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_generate_all_import_missing_file_errors() {
        let (dir, ui_dir) = test_dir();

        std::fs::write(ui_dir.join("App.fui"), "---\nuse ui::Missing;\n---\n<Stack></Stack>").unwrap();

        let out_file = dir.join("out.rs");
        let result = generate_all(&ui_dir, &out_file);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("Missing") || err.contains("not found"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_generate_all_with_gpui_import() {
        let (dir, ui_dir) = test_dir();

        std::fs::write(
            ui_dir.join("App.fui"),
            "---\nuse gpui::TextInput;\n---\n<Stack></Stack>",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        generate_all(&ui_dir, &out_file).unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        // gpui imports are real Rust — preserved in frontmatter
        assert!(content.contains("use gpui::TextInput;"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_generate_all_recursive_scan() {
        let (dir, ui_dir) = test_dir();

        // File in subdirectory
        std::fs::create_dir_all(ui_dir.join("widgets")).unwrap();
        std::fs::write(ui_dir.join("widgets").join("Footer.fui"), "<Text>footer</Text>").unwrap();
        // File in root
        std::fs::write(
            ui_dir.join("App.fui"),
            "---\nuse ui::widgets::Footer;\n---\n<Stack><Footer /></Stack>",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        generate_all(&ui_dir, &out_file).unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        assert!(
            content.contains("pub fn render_footer()"),
            "content missing render_footer fn:\n{content}"
        );
        assert!(content.contains("render_footer()"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // --- Semantic analysis tests ---

    fn test_dir_with_src() -> (PathBuf, PathBuf, PathBuf) {
        let id = LIB_TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("fncc_sem_lib_test_{id}"));
        let ui_dir = dir.join("ui");
        let src_dir = dir.join("src");
        std::fs::create_dir_all(&ui_dir).unwrap();
        std::fs::create_dir_all(&src_dir).unwrap();
        (dir, ui_dir, src_dir)
    }

    #[test]
    fn test_generate_all_with_options_infers_state_from_rs() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        // .fui file without @state
        std::fs::write(
            ui_dir.join("App.fui"),
            r#"<Stack direction="vertical" gap="12">
    <Text>Count: {state.count}</Text>
    <Button onclick="inc">+1</Button>
</Stack>"#,
        )
        .unwrap();

        // .rs file with Level 3 command
        std::fs::write(
            src_dir.join("main.rs"),
            "#[fncc::command]\nfn inc(state: &mut CounterState, cx: &mut Context<CounterState>) { state.count += 1; cx.notify(); }\n",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        })
        .unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        // Should generate impl Render for CounterState (inferred from command)
        assert!(content.contains("impl Render for CounterState {"));
        assert!(content.contains("__fncc_cmd_inc"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_generate_all_with_options_hard_error_on_missing_command() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        std::fs::write(
            ui_dir.join("App.fui"),
            r#"<Button onclick="nonexistent">Click</Button>"#,
        )
        .unwrap();

        // No .rs file with #[fncc::command] nonexistent
        std::fs::write(src_dir.join("main.rs"), "fn main() {}\n").unwrap();

        let out_file = dir.join("out.rs");
        let result = generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        });

        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("nonexistent"), "error should mention command name: {err}");
        assert!(err.contains("not found"), "error should say not found: {err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_generate_all_with_options_state_conflict_errors() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        // .fui with @state that conflicts with command signature
        std::fs::write(
            ui_dir.join("App.fui"),
            "---\n@state OldState\n---\n<Button onclick=\"upd\">Update</Button>",
        )
        .unwrap();

        std::fs::write(
            src_dir.join("main.rs"),
            "#[fncc::command]\nfn upd(state: &mut NewState, cx: &mut Context<NewState>) {}\n",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        let result = generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        });

        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("@state OldState"),
            "error should mention @state declaration: {err}"
        );
        assert!(err.contains("NewState"), "error should mention inferred type: {err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_generate_all_with_options_stateless_remains_stateless() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        // Stateless component, no @state, no Level 3 commands
        std::fs::write(ui_dir.join("App.fui"), "<Text>hello</Text>").unwrap();
        std::fs::write(src_dir.join("main.rs"), "fn main() {}\n").unwrap();

        let out_file = dir.join("out.rs");
        generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        })
        .unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        // Should be stateless — standalone render function
        assert!(content.contains("pub fn render_app()"));
        assert!(!content.contains("impl Render for"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_generate_all_with_options_state_mismatch_across_commands() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        std::fs::write(
            ui_dir.join("App.fui"),
            r#"<Stack>
    <Button onclick="cmd_a">A</Button>
    <Button onclick="cmd_b">B</Button>
</Stack>"#,
        )
        .unwrap();

        std::fs::write(
            src_dir.join("main.rs"),
            "#[fncc::command]\nfn cmd_a(s: &mut TypeA, cx: &mut Context<TypeA>) {}\n\
             #[fncc::command]\nfn cmd_b(s: &mut TypeB, cx: &mut Context<TypeB>) {}\n",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        let result = generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        });

        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("multiple state types"),
            "error should mention multiple types: {err}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_generate_all_legacy_still_works_without_src_dir() {
        // Verify that legacy generate_all (no src_dir) still works unchanged
        let (dir, ui_dir) = test_dir();

        std::fs::write(
            ui_dir.join("App.fui"),
            "---\n@state CounterState\n---\n<Button onclick=\"inc\">+1</Button>",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        generate_all(&ui_dir, &out_file).unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        assert!(content.contains("impl Render for CounterState {"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // --- Props integration tests ---

    #[test]
    fn test_props_component_with_props_generates_props_signature() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        // Component .fui — declares its own props type
        std::fs::write(
            ui_dir.join("Header.fui"),
            "---\nuse props::HeaderProps;\n---\n<Text>{props.title}</Text>",
        )
        .unwrap();

        // .rs file with #[derive(Props)] struct
        std::fs::write(
            src_dir.join("lib.rs"),
            "#[derive(fncc::Props)]\nstruct HeaderProps {\n    pub title: String,\n}\n",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        })
        .unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        assert!(
            content.contains("pub fn render_header(props: &HeaderProps)"),
            "expected props signature, got:\n{content}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_props_caller_constructs_props_struct() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        // Component that defines props
        std::fs::write(
            ui_dir.join("Header.fui"),
            "---\nuse props::HeaderProps;\n---\n<Text>{props.title}</Text>",
        )
        .unwrap();

        // App that imports and calls it with attribute values
        std::fs::write(
            ui_dir.join("App.fui"),
            "---\nuse ui::Header;\nuse props::HeaderProps;\n---\n<Stack><Header title=\"Hello\" /></Stack>",
        )
        .unwrap();

        // .rs file with props struct
        std::fs::write(
            src_dir.join("lib.rs"),
            "#[derive(fncc::Props)]\nstruct HeaderProps {\n    pub title: String,\n}\n",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        })
        .unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        assert!(content.contains("render_header(&HeaderProps {"));
        assert!(content.contains("title: \"Hello\".into(),"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_props_multiple_fields() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        std::fs::write(
            ui_dir.join("Banner.fui"),
            "---\nuse props::BannerProps;\n---\n<Text>{props.heading}{props.sub}</Text>",
        )
        .unwrap();

        std::fs::write(
            ui_dir.join("App.fui"),
            "---\nuse ui::Banner;\nuse props::BannerProps;\n---\n<Banner heading=\"Hi\" sub=\"World\" />",
        )
        .unwrap();

        std::fs::write(
            src_dir.join("lib.rs"),
            "#[derive(fncc::Props)]\nstruct BannerProps {\n    pub heading: String,\n    pub sub: String,\n}\n",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        })
        .unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        assert!(content.contains("heading: \"Hi\".into(),"));
        assert!(content.contains("sub: \"World\".into(),"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_props_option_field() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        std::fs::write(
            ui_dir.join("Card.fui"),
            "---\nuse props::CardProps;\n---\n<Text>{props.title}</Text>",
        )
        .unwrap();

        std::fs::write(
            ui_dir.join("App.fui"),
            "---\nuse ui::Card;\nuse props::CardProps;\n---\n<Card title=\"Optional\" />",
        )
        .unwrap();

        // Option<String> field — .into() handles conversion at codegen level
        std::fs::write(
            src_dir.join("lib.rs"),
            "#[derive(fncc::Props)]\nstruct CardProps {\n    pub title: Option<String>,\n}\n",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        })
        .unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        assert!(content.contains("title: \"Optional\".into(),"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // --- Props validation tests ---

    #[test]
    fn test_props_unknown_attribute_errors() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        std::fs::write(
            ui_dir.join("Widget.fui"),
            "---\nuse props::WidgetProps;\n---\n<Text>{props.title}</Text>",
        )
        .unwrap();

        std::fs::write(
            ui_dir.join("App.fui"),
            "---\nuse ui::Widget;\nuse props::WidgetProps;\n---\n<Widget title=\"Hi\" unknown=\"val\" />",
        )
        .unwrap();

        std::fs::write(
            src_dir.join("lib.rs"),
            "#[derive(fncc::Props)]\npub struct WidgetProps { pub title: String, }\n",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        let result = generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        });

        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("unknown"), "error should mention unknown prop: {err}");
        assert!(err.contains("unknown"), "error should contain attribute name: {err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_props_missing_required_field_errors() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        std::fs::write(
            ui_dir.join("Card.fui"),
            "---\nuse props::CardProps;\n---\n<Text>{props.heading}</Text>",
        )
        .unwrap();

        // Missing required field "heading"
        std::fs::write(
            ui_dir.join("App.fui"),
            "---\nuse ui::Card;\nuse props::CardProps;\n---\n<Card />",
        )
        .unwrap();

        std::fs::write(
            src_dir.join("lib.rs"),
            "#[derive(fncc::Props)]\npub struct CardProps { pub heading: String, }\n",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        let result = generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        });

        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("requires"), "error should mention requires: {err}");
        assert!(err.contains("heading"), "error should mention field name: {err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_props_optional_field_allows_absence() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        std::fs::write(
            ui_dir.join("Banner.fui"),
            "---\nuse props::BannerProps;\n---\n<Text>{props.title}</Text>",
        )
        .unwrap();

        // subtitle is Option<String> — allowed to be absent
        std::fs::write(
            ui_dir.join("App.fui"),
            "---\nuse ui::Banner;\nuse props::BannerProps;\n---\n<Banner title=\"Hi\" />",
        )
        .unwrap();

        std::fs::write(
            src_dir.join("lib.rs"),
            "#[derive(fncc::Props)]\npub struct BannerProps { pub title: String, pub subtitle: Option<String>, }\n",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        })
        .unwrap();

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_props_all_optional_none_provided_ok() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        std::fs::write(
            ui_dir.join("Msg.fui"),
            "---\nuse props::MsgProps;\n---\n<Text>{props.body}</Text>",
        )
        .unwrap();

        // All fields optional, none provided — should succeed
        std::fs::write(
            ui_dir.join("App.fui"),
            "---\nuse ui::Msg;\nuse props::MsgProps;\n---\n<Msg />",
        )
        .unwrap();

        std::fs::write(
            src_dir.join("lib.rs"),
            "#[derive(fncc::Props)]\npub struct MsgProps { pub body: Option<String>, }\n",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        })
        .unwrap();

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_props_missing_required_with_optionals_present_errors() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        std::fs::write(
            ui_dir.join("Form.fui"),
            "---\nuse props::FormProps;\n---\n<Text>{props.name}</Text>",
        )
        .unwrap();

        // name is required (String) and missing; email is Option<String> and present
        std::fs::write(
            ui_dir.join("App.fui"),
            "---\nuse ui::Form;\nuse props::FormProps;\n---\n<Form email=\"a@b.com\" />",
        )
        .unwrap();

        std::fs::write(
            src_dir.join("lib.rs"),
            "#[derive(fncc::Props)]\npub struct FormProps { pub name: String, pub email: Option<String>, }\n",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        let result = generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        });

        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("name"), "error should mention missing field name: {err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_props_grouped_imports() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        std::fs::create_dir_all(ui_dir.join("widgets")).unwrap();
        std::fs::write(
            ui_dir.join("widgets").join("Header.fui"),
            "---\nuse props::HeaderProps;\n---\n<Text>{props.title}</Text>",
        )
        .unwrap();
        std::fs::write(
            ui_dir.join("widgets").join("Footer.fui"),
            "---\nuse props::FooterProps;\n---\n<Text>{props.msg}</Text>",
        )
        .unwrap();

        std::fs::write(
            ui_dir.join("App.fui"),
            "---\nuse ui::widgets::{Header, Footer};\nuse props::{HeaderProps, FooterProps};\n---\n<Stack><Header title=\"A\" /><Footer msg=\"B\" /></Stack>",
        )
        .unwrap();

        std::fs::write(
            src_dir.join("lib.rs"),
            "#[derive(fncc::Props)]\nstruct HeaderProps { pub title: String, }\n\
             #[derive(fncc::Props)]\nstruct FooterProps { pub msg: String, }\n",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        })
        .unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        assert!(content.contains("pub fn render_header(props: &HeaderProps)"));
        assert!(content.contains("pub fn render_footer(props: &FooterProps)"));
        assert!(content.contains("render_header(&HeaderProps {"));
        assert!(content.contains("title: \"A\".into(),"));
        assert!(content.contains("render_footer(&FooterProps {"));
        assert!(content.contains("msg: \"B\".into(),"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_props_nested_component_receiving_props() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        std::fs::write(
            ui_dir.join("Header.fui"),
            "---\nuse props::HeaderProps;\n---\n<Text>{props.title}</Text>",
        )
        .unwrap();

        // App has Header nested inside a Stack, with both props and non-props children
        std::fs::write(
            ui_dir.join("App.fui"),
            "---\nuse ui::Header;\nuse props::HeaderProps;\n---\n<Stack><Header title=\"Nest\" /><Text>plain</Text></Stack>",
        )
        .unwrap();

        std::fs::write(
            src_dir.join("lib.rs"),
            "#[derive(fncc::Props)]\nstruct HeaderProps { pub title: String, }\n",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        })
        .unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        assert!(content.contains("render_header(&HeaderProps {"));
        assert!(content.contains("title: \"Nest\".into(),"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_props_mixed_components_with_and_without_props() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        // Component with props
        std::fs::write(
            ui_dir.join("Header.fui"),
            "---\nuse props::HeaderProps;\n---\n<Text>{props.title}</Text>",
        )
        .unwrap();

        // Component without props
        std::fs::write(ui_dir.join("Footer.fui"), "<Text>static footer</Text>").unwrap();

        // App uses both
        std::fs::write(
            ui_dir.join("App.fui"),
            "---\nuse ui::{Header, Footer};\nuse props::HeaderProps;\n---\n<Stack><Header title=\"X\" /><Footer /></Stack>",
        )
        .unwrap();

        std::fs::write(
            src_dir.join("lib.rs"),
            "#[derive(fncc::Props)]\nstruct HeaderProps { pub title: String, }\n",
        )
        .unwrap();

        let out_file = dir.join("out.rs");
        generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        })
        .unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        assert!(content.contains("render_header(&HeaderProps {"));
        assert!(content.contains("title: \"X\".into(),"));
        assert!(content.contains("render_footer()"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_props_component_without_props_uses_direct_call() {
        let (dir, ui_dir, src_dir) = test_dir_with_src();

        std::fs::write(ui_dir.join("Footer.fui"), "<Text>footer</Text>").unwrap();

        std::fs::write(ui_dir.join("App.fui"), "---\nuse ui::Footer;\n---\n<Footer />").unwrap();

        std::fs::write(src_dir.join("lib.rs"), "fn main() {}\n").unwrap();

        let out_file = dir.join("out.rs");
        generate_all_with_options(GenerateOptions {
            ui_dir: &ui_dir,
            out_file: &out_file,
            src_dir: Some(&src_dir),
        })
        .unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        assert!(content.contains("render_footer()"));
        assert!(!content.contains("&"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
