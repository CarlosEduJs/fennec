pub mod codegen;
pub mod config;
pub mod parser;
pub mod router;
pub mod semantic;

pub use parser::parse;
pub use router::{RouteInfo, RouteTree};

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Recursively collect all .fui files under `dir`.
fn collect_fui_files(dir: &Path) -> Result<Vec<PathBuf>> {
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

/// Collect all .fncss files under `dir`, returning map from parent dir to parsed stylesheet.
fn collect_fncss_files(dir: &Path) -> Result<HashMap<PathBuf, fncc_styles::Stylesheet>> {
    let mut sheets = HashMap::new();
    collect_fncss_recursive(dir, &mut sheets)?;
    Ok(sheets)
}

fn collect_fncss_recursive(current: &Path, sheets: &mut HashMap<PathBuf, fncc_styles::Stylesheet>) -> Result<()> {
    if !current.is_dir() {
        return Ok(());
    }
    let mut entries: Vec<_> = std::fs::read_dir(current)
        .with_context(|| format!("failed to read dir {:?}", current))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("failed to read entry in {:?}", current))?;
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_fncss_recursive(&path, sheets)?;
        } else if path.extension().is_some_and(|ext| ext == "fncss") {
            let content = std::fs::read_to_string(&path).with_context(|| format!("failed to read {:?}", path))?;
            match fncc_styles::css_parser::parse(&content) {
                Ok(mut ss) => {
                    let parent = path.parent().unwrap_or(current).to_path_buf();
                    resolve_font_paths(&mut ss, &parent)?;
                    let merged = match sheets.remove(&parent) {
                        Some(prev) => fncc_styles::merge(vec![prev, ss]),
                        None => ss,
                    };
                    sheets.insert(parent, merged);
                }
                Err(e) => anyhow::bail!("failed to parse {:?}: {e}", path),
            }
        }
    }
    Ok(())
}

/// Resolve relative `@font-face` src paths against the `.fncss` file's directory,
/// producing absolute paths for the build.
fn resolve_font_paths(ss: &mut fncc_styles::Stylesheet, base_dir: &Path) -> Result<()> {
    for path in ss.fonts.values_mut() {
        let p = Path::new(path.as_str());
        if !p.is_absolute() {
            *path = base_dir.join(p).to_string_lossy().to_string();
        }
    }
    Ok(())
}

/// Copy all declared `@font-face` fonts into `$OUT_DIR/fncc_fonts` and generate a
/// `fncc_load_fonts(cx: &mut App)` function that registers them with GPUI via
/// `cx.text_system().add_fonts(...)`. Returns empty string when no fonts are declared.
fn generate_fonts_code(sheets: &HashMap<PathBuf, fncc_styles::Stylesheet>, out_file: &Path) -> Result<String> {
    let mut fonts: Vec<(&str, &str)> = Vec::new();
    for sheet in sheets.values() {
        for (family, path) in &sheet.fonts {
            if !fonts.iter().any(|(f, p)| f == family && *p == path.as_str()) {
                fonts.push((family.as_str(), path.as_str()));
            }
        }
    }

    if fonts.is_empty() {
        return Ok(String::new());
    }

    let out_dir = out_file.parent().context("out_file has no parent directory")?;
    let fonts_dir = out_dir.join("fncc_fonts");
    std::fs::create_dir_all(&fonts_dir).context("failed to create fncc_fonts dir")?;

    let mut entries = String::new();
    for (_, path) in &fonts {
        let src = Path::new(path);
        // Deterministic unique destination name derived from the full source path,
        // so fonts with the same file name in different directories never collide.
        let file_name = src
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '.' { c } else { '_' })
            .collect::<String>();
        let dest = fonts_dir.join(&file_name);
        std::fs::copy(src, &dest).with_context(|| format!("failed to copy font {} to {:?}", src.display(), dest))?;
        entries.push_str(&format!(
            "        Cow::Borrowed(include_bytes!(concat!(env!(\"OUT_DIR\"), \"/fncc_fonts/{file_name}\")) as &[u8]),\n"
        ));
    }

    Ok(format!(
        "/// Register custom fonts declared via `@font-face` in `.fncss` files.\n\
         /// Call this in app startup, before opening any window:\n\
         /// ```no_run\n\
         /// Application::new().run(|cx: &mut App| {{ fncc_load_fonts(cx); ... }});\n\
         /// ```\n\
         pub fn fncc_load_fonts(cx: &mut App) {{\n\
         \x20   use std::borrow::Cow;\n\
         \x20   if let Err(e) = cx.text_system().add_fonts(vec![\n\
         {entries}\
         \x20   ]) {{\n\
         \x20       eprintln!(\"Failed to load fonts: {{e:?}}\");\n\
         \x20   }}\n\
         }}\n\n"
    ))
}

/// Build the style cascade for a .fui file at `fui_path` relative to `ui_dir`.
/// Merges from least specific to most specific: root → parent dirs → same dir → inline <Styles>.
fn build_cascade(
    fui_path: &Path,
    ui_dir: &Path,
    fncss_sheets: &HashMap<PathBuf, fncc_styles::Stylesheet>,
    inline_styles: Option<&str>,
) -> Result<fncc_styles::Stylesheet> {
    let mut cascade = Vec::new();

    let fui_dir = fui_path.parent().unwrap_or(Path::new(""));

    // Collect .fncss from root to the .fui's directory
    let mut ancestors: Vec<PathBuf> = Vec::new();
    let mut current = Some(fui_dir.to_path_buf());
    while let Some(dir) = current {
        if dir == ui_dir || dir.starts_with(ui_dir) || ui_dir.starts_with(&dir) {
            ancestors.push(dir.clone());
        }
        if dir == ui_dir {
            break;
        }
        current = dir.parent().map(|p| p.to_path_buf());
    }
    ancestors.reverse();

    for dir in &ancestors {
        if let Some(ss) = fncss_sheets.get(dir) {
            cascade.push(ss.clone());
        }
    }

    // Parse inline <Styles> and merge on top
    if let Some(styles_text) = inline_styles {
        let inline_ss = fncc_styles::css_parser::parse(styles_text)
            .map_err(|e| anyhow::anyhow!("failed to parse <Styles> block: {e}"))?;
        cascade.push(inline_ss);
    }

    Ok(fncc_styles::merge(cascade))
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

    // Collect and parse .fncss files
    let fncss_sheets = collect_fncss_files(ui_dir)?;

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

    // Check for routes directory upfront
    let routes_dir = if ui_dir.file_name() == Some(std::ffi::OsStr::new("routes")) {
        Some(ui_dir.to_path_buf())
    } else if ui_dir.join("routes").is_dir() {
        Some(ui_dir.join("routes"))
    } else {
        None
    };

    let route_tree = if let Some(ref r_dir) = routes_dir {
        Some(RouteTree::scan(r_dir)?)
    } else {
        None
    };

    // Helper to resolve unique render function name for any component (route or standard)
    let resolve_render_fn = |pf: &ParsedFile| -> String {
        if let (Some(r_dir), Some(tree)) = (routes_dir.as_ref(), route_tree.as_ref())
            && let Ok(rel) = pf.path.strip_prefix(r_dir)
        {
            if rel.file_name() == Some(std::ffi::OsStr::new("layout.fui")) {
                let parent = rel.parent().unwrap_or_else(|| Path::new(""));
                let parent_str = parent.to_string_lossy();
                if parent_str.is_empty() {
                    return "render_layout".to_string();
                } else {
                    let pascal = router::to_pascal_case(&parent_str.replace('/', "_"));
                    return format!("render_{}_layout", codegen::to_snake_case(&pascal));
                }
            }

            if let Some(route) = tree.routes.iter().find(|r| r.file_path == pf.path) {
                return format!("render_{}", codegen::to_snake_case(&route.variant_name));
            }

            if let Some(ref fb) = tree.fallback
                && fb.file_path == pf.path
            {
                return "render_fallback".to_string();
            }
        }

        let component_name = pf.relative_stem.split('/').next_back().unwrap_or("");
        format!("render_{}", codegen::to_snake_case(component_name))
    };

    // Build import resolution index: "ui::path::Component" -> render function name
    let mut import_index: HashMap<String, String> = HashMap::new();
    for pf in &parsed_files {
        let ui_path = format!("ui::{}", pf.relative_stem.replace('/', "::"));
        let render_fn = resolve_render_fn(pf);
        import_index.insert(ui_path, render_fn);
    }

    // Build props type index: render_fn_name -> props type name (e.g. "HeaderProps")
    // Only set when the component's template actually uses {props.xxx} interpolation.
    let mut render_fn_to_props: HashMap<String, Option<String>> = HashMap::new();
    for pf in &parsed_files {
        let render_fn = resolve_render_fn(pf);
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

    // Build slot index: render_fn_name -> has_slot
    let mut render_fn_to_slot: HashMap<String, bool> = HashMap::new();
    for pf in &parsed_files {
        let render_fn = resolve_render_fn(pf);
        let has_slot = parser::has_slot(&pf.ast.root);
        render_fn_to_slot.insert(render_fn, has_slot);
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

    // Collect custom fonts declared via @font-face and emit a loader function.
    // Font files are copied to $OUT_DIR/fncc_fonts so the generated code can
    // embed them with include_bytes!.
    let fonts_code = generate_fonts_code(&fncss_sheets, out_file)?;
    output.push_str(&fonts_code);

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

        let render_fn = resolve_render_fn(pf);
        let comp_name = render_fn.strip_prefix("render_").unwrap_or(&render_fn);
        let component_name = Some(comp_name);
        let resolved_state = state_types.get(&file_id).and_then(|s| s.as_deref());

        // Stateful components cannot receive props
        if resolved_state.is_some() && own_props_type.is_some() {
            anyhow::bail!(
                "in '{}': component cannot have both state and props — props are only supported on stateless components",
                pf.path.display(),
            );
        }

        // Does this component itself have a slot? (computed once, used for both validation and codegen)
        let has_slot = parser::has_slot(&pf.ast.root);

        // Stateful components cannot have slots
        if resolved_state.is_some() && has_slot {
            anyhow::bail!(
                "in '{}': stateful components cannot use `<Slot>` — slots are only supported on stateless components",
                pf.path.display(),
            );
        }

        // Resolve import_has_slots: for each FuiPath import, does the target have a slot?
        let import_has_slots: Vec<(&str, bool)> = pf
            .ast
            .imports
            .iter()
            .filter_map(|imp| {
                let render_fn = match &imp.source {
                    parser::ImportSource::FuiPath(ui_path) => import_index.get(ui_path),
                    _ => None,
                }?;
                let has_slot = render_fn_to_slot.get(render_fn).copied().unwrap_or(false);
                Some((imp.name.as_str(), has_slot))
            })
            .collect();

        let prop_fields = semantic_db.as_ref().map(|db| &db.props_types);
        let style_cascade = build_cascade(&pf.path, ui_dir, &fncss_sheets, pf.ast.styles.as_deref())
            .map_err(|e| anyhow::anyhow!("in '{}': {e}", pf.path.display()))?;
        let style_theme = pf.ast.theme.as_deref();
        if let Some(theme_name) = style_theme
            && !style_cascade.themes.contains_key(theme_name)
        {
            anyhow::bail!(
                "in '{}': unknown theme `{theme_name}` declared via `@theme` — no matching `theme {theme_name} {{ ... }}` block in the cascade",
                pf.path.display(),
            );
        }
        let route_params = route_tree
            .as_ref()
            .and_then(|tree| tree.routes.iter().find(|r| r.file_path == pf.path))
            .map(|r| r.params.as_slice())
            .unwrap_or(&[]);

        let generated = codegen::generate_with_imports_and_route_params(
            &pf.ast,
            file_id,
            &resolved,
            component_name,
            resolved_state,
            own_props_type,
            &import_props,
            prop_fields,
            &import_has_slots,
            Some(&style_cascade),
            style_theme,
            route_params,
        );
        output.push_str(&generated);
        output.push('\n');
    }

    // Generate Native File-Based Routing (NFBR) code if routes directory is present
    let routes_dir = if ui_dir.file_name() == Some(std::ffi::OsStr::new("routes")) {
        Some(ui_dir.to_path_buf())
    } else if ui_dir.join("routes").is_dir() {
        Some(ui_dir.join("routes"))
    } else {
        None
    };

    if let Some(ref r_dir) = routes_dir {
        let route_tree = RouteTree::scan(r_dir)?;
        if !route_tree.routes.is_empty() || route_tree.fallback.is_some() {
            let router_code = router::generate_router_code(&route_tree);
            output.push_str(&router_code);
        }
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

    #[test]
    fn test_generate_font_loader() {
        let (dir, ui_dir) = test_dir();
        let fonts_dir = ui_dir.join("fonts");
        std::fs::create_dir_all(&fonts_dir).unwrap();
        std::fs::write(fonts_dir.join("Verdana.ttf"), b"fake font bytes").unwrap();
        std::fs::write(ui_dir.join("App.fui"), "<Text>hello</Text>").unwrap();
        std::fs::write(
            ui_dir.join("styles.fncss"),
            "@font-face { font-family: Verdana; src: url(\"./fonts/Verdana.ttf\"); }",
        )
        .unwrap();
        let out_file = dir.join("out.rs");

        generate_all(&ui_dir, &out_file).unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        assert!(
            content.contains("pub fn fncc_load_fonts(cx: &mut App)"),
            "missing fncc_load_fonts: {content}"
        );
        let file_name: String = ui_dir
            .join("./fonts/Verdana.ttf")
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '.' { c } else { '_' })
            .collect();
        assert!(content.contains(&format!(
            "include_bytes!(concat!(env!(\"OUT_DIR\"), \"/fncc_fonts/{file_name}\"))"
        )));

        let copied = out_file.parent().unwrap().join("fncc_fonts").join(&file_name);
        assert!(copied.exists(), "font not copied to {:?}", copied);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_generate_font_loader_no_fonts() {
        let (dir, ui_dir) = test_dir();
        std::fs::write(ui_dir.join("App.fui"), "<Text>hello</Text>").unwrap();
        let out_file = dir.join("out.rs");

        generate_all(&ui_dir, &out_file).unwrap();

        let content = std::fs::read_to_string(&out_file).unwrap();
        assert!(!content.contains("fncc_load_fonts"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_nfbr_full_end_to_end() {
        let (dir, ui_dir) = test_dir();
        let routes_dir = ui_dir.join("routes");
        std::fs::create_dir_all(&routes_dir).unwrap();

        // Top level layout.fui with RouterOutlet
        std::fs::write(
            routes_dir.join("layout.fui"),
            "<Stack><Text>Root Layout Header</Text><RouterOutlet /></Stack>",
        )
        .unwrap();

        // Index route
        std::fs::write(routes_dir.join("index.fui"), "<Text>Home Screen</Text>").unwrap();

        // Nested dashboard layout and screen
        let dash_dir = routes_dir.join("dashboard");
        std::fs::create_dir_all(&dash_dir).unwrap();
        std::fs::write(
            dash_dir.join("layout.fui"),
            "<Stack><Text>Dashboard Sidebar</Text><RouterOutlet /></Stack>",
        )
        .unwrap();
        std::fs::write(dash_dir.join("index.fui"), "<Text>Dashboard Main</Text>").unwrap();

        // Dynamic parameter route: users/[id].fui
        let users_dir = routes_dir.join("users");
        std::fs::create_dir_all(&users_dir).unwrap();
        std::fs::write(users_dir.join("[id].fui"), "<Text>{id}</Text>").unwrap();

        let out_file = dir.join("generated.rs");
        generate_all(&ui_dir, &out_file).unwrap();

        let code = std::fs::read_to_string(&out_file).unwrap();

        // Layout functions accept children: impl IntoElement
        assert!(code.contains("pub fn render_layout(children: impl IntoElement) -> impl IntoElement"));
        assert!(code.contains("pub fn render_dashboard_layout(children: impl IntoElement) -> impl IntoElement"));

        // Dynamic parameter screen accepts id: &str
        assert!(code.contains("pub fn render_users_id(id: &str) -> impl IntoElement"));

        // Generated Route enum
        assert!(code.contains("pub enum Route {"));
        assert!(code.contains("Index,"));
        assert!(code.contains("Dashboard,"));
        assert!(code.contains("UsersId {"));

        // Route::render cascades nested layouts
        assert!(code.contains("Route::Dashboard => render_layout(render_dashboard_layout(render_dashboard())).into_any_element()"));
        assert!(code.contains("Route::UsersId { id } => render_layout(render_users_id(id.as_str())).into_any_element()"));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
