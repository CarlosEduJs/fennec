use anyhow::{anyhow, Context, Result};
use chrono::Local;
use clap::{Parser, Subcommand};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use toml_edit::{value, DocumentMut};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "manage release crates", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// add new file in .changes/
    Change {
        /// crate name (eg: fennec-runtime)
        #[arg(short, long)]
        krate: Option<String>,
        /// patch, minor, major
        #[arg(short, long)]
        bump: Option<String>,
        /// description for changelog
        #[arg(short, long)]
        message: Option<String>,
    },
    /// this checks the .changes/ directory
    Check,
    /// bump versions of crates based on .changes/ and update CHANGELOG.md
    Bump {
        /// dry run mode, does not modify files
        #[arg(long)]
        dry_run: bool,
    },
    /// publish crates to crates.io in the correct order
    Publish {
        /// dry run mode, does not publish crates
        #[arg(long)]
        dry_run: bool,
        /// actually execute the publish commands
        #[arg(long)]
        execute: bool,
    },
    /// gera o relatorio de release formatado em tabela e lista Markdown
    Plan {
        /// salva o relatorio Markdown em um arquivo
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// cria ou atualiza o Release PR com tabela de versao e lista de notas
    ReleasePr {
        /// branch base (padrao: main)
        #[arg(short, long, default_value = "main")]
        base: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum BumpType {
    Patch = 1,
    Minor = 2,
    Major = 3,
}

impl BumpType {
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "patch" => Ok(BumpType::Patch),
            "minor" => Ok(BumpType::Minor),
            "major" => Ok(BumpType::Major),
            _ => Err(anyhow!("Tipo de bump inválido: '{s}'. Use patch, minor ou major.")),
        }
    }

    fn apply(&self, version: &Version) -> Version {
        let mut v = version.clone();
        match self {
            BumpType::Patch => {
                v.patch += 1;
            }
            BumpType::Minor => {
                v.minor += 1;
                v.patch = 0;
            }
            BumpType::Major => {
                v.major += 1;
                v.minor = 0;
                v.patch = 0;
            }
        }
        v
    }
}

#[derive(Debug)]
struct ChangeFile {
    path: PathBuf,
    crate_bumps: HashMap<String, BumpType>,
    summary: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = find_workspace_root()?;

    match cli.command {
        Commands::Change { krate, bump, message } => run_change(&root, krate, bump, message)?,
        Commands::Check => run_check(&root)?,
        Commands::Bump { dry_run } => run_bump(&root, dry_run)?,
        Commands::Publish { dry_run, execute } => run_publish(&root, dry_run, execute)?,
        Commands::Plan { output } => run_plan(&root, output)?,
        Commands::ReleasePr { base } => run_release_pr(&root, &base)?,
    }

    Ok(())
}

fn find_workspace_root() -> Result<PathBuf> {
    let mut current = std::env::current_dir()?;
    loop {
        if current.join("Cargo.toml").exists() && current.join(".changes").exists() {
            return Ok(current);
        }
        if !current.pop() {
            break;
        }
    }
    let pwd = std::env::current_dir()?;
    if pwd.join("Cargo.toml").exists() {
        Ok(pwd)
    } else {
        Err(anyhow!("no Cargo.toml found in current directory or any parent directories"))
    }
}

fn get_workspace_crates(root: &Path) -> Result<HashMap<String, PathBuf>> {
    let root_toml_path = root.join("Cargo.toml");
    let content = fs::read_to_string(&root_toml_path)?;
    let doc: DocumentMut = content.parse()?;

    let mut crates = HashMap::new();

    if let Some(workspace) = doc.get("workspace").and_then(|w| w.as_table()) {
        if let Some(members) = workspace.get("members").and_then(|m| m.as_array()) {
            for member in members {
                if let Some(rel_path) = member.as_str() {
                    if rel_path == "xtask" {
                        continue;
                    }
                    let crate_toml = root.join(rel_path).join("Cargo.toml");
                    if crate_toml.exists() {
                        let c_content = fs::read_to_string(&crate_toml)?;
                        let c_doc: DocumentMut = c_content.parse()?;
                        if let Some(name) = c_doc.get("package").and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
                            crates.insert(name.to_string(), root.join(rel_path));
                        }
                    }
                }
            }
        }
    }

    Ok(crates)
}

fn run_change(root: &Path, krate: Option<String>, bump: Option<String>, message: Option<String>) -> Result<()> {
    let changes_dir = root.join(".changes");
    if !changes_dir.exists() {
        fs::create_dir_all(&changes_dir)?;
    }

    let available_crates = get_workspace_crates(root)?;
    let selected_crate = match krate {
        Some(k) => {
            if !available_crates.contains_key(&k) {
                return Err(anyhow!("Crate '{k}' not found in the workspace. Crates available: {:?}", available_crates.keys().collect::<Vec<_>>()));
            }
            k
        }
        None => {
            println!("Crates available:");
            let list: Vec<_> = available_crates.keys().collect();
            for (i, name) in list.iter().enumerate() {
                println!("  [{}] {}", i + 1, name);
            }
            print!("Select the crate number: ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let idx: usize = input.trim().parse().map_err(|_| anyhow!("Invalid option"))?;
            if idx == 0 || idx > list.len() {
                return Err(anyhow!("Selection out of range."));
            }
            list[idx - 1].to_string()
        }
    };

    let selected_bump = match bump {
        Some(b) => BumpType::from_str(&b)?,
        None => {
            println!("Type of change (bump):");
            println!("  [1] patch  (fix bugs / small changes)");
            println!("  [2] minor  (new features, backward compatible)");
            println!("  [3] major  (breaking changes)");
            print!("Option (1-3): ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            match input.trim() {
                "1" | "patch" => BumpType::Patch,
                "2" | "minor" => BumpType::Minor,
                "3" | "major" => BumpType::Major,
                _ => return Err(anyhow!("Invalid option.")),
            }
        }
    };

    let summary = match message {
        Some(m) => m,
        None => {
            print!("Describe the changes briefly for the CHANGELOG: ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let trimmed = input.trim().to_string();
            if trimmed.is_empty() {
                return Err(anyhow!("the description cannot be empty."));
            }
            trimmed
        }
    };

    let timestamp = Local::now().format("%Y%m%d-%H%M%S");
    let filename = format!("{selected_crate}-{timestamp}.md");
    let filepath = changes_dir.join(&filename);

    let bump_str = match selected_bump {
        BumpType::Patch => "patch",
        BumpType::Minor => "minor",
        BumpType::Major => "major",
    };

    let file_content = format!(
        "---\n{selected_crate}: {bump_str}\n---\n\n- {summary}\n"
    );

    fs::write(&filepath, file_content)?;
    println!("Change registered successfully in: .changes/{filename}");

    Ok(())
}

fn parse_change_files(root: &Path) -> Result<Vec<ChangeFile>> {
    let changes_dir = root.join(".changes");
    if !changes_dir.exists() {
        return Ok(Vec::new());
    }

    let mut change_files = Vec::new();

    for entry in WalkDir::new(&changes_dir).min_depth(1).max_depth(1) {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().map_or(false, |ext| ext == "md") {
            let filename = path.file_name().unwrap().to_string_lossy();
            if filename == "README.md" {
                continue;
            }

            let content = fs::read_to_string(path)?;
            if let Some((frontmatter, body)) = split_frontmatter(&content) {
                let crate_bumps: HashMap<String, String> = serde_yaml::from_str(frontmatter)
                    .with_context(|| format!("err analyze YAML frontmatter in {}", path.display()))?;

                let mut parsed_bumps = HashMap::new();
                for (krate, b_str) in crate_bumps {
                    let b_type = BumpType::from_str(&b_str)?;
                    parsed_bumps.insert(krate, b_type);
                }

                change_files.push(ChangeFile {
                    path: path.to_path_buf(),
                    crate_bumps: parsed_bumps,
                    summary: body.trim().to_string(),
                });
            }
        }
    }

    Ok(change_files)
}

fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let rest = &trimmed[3..];
    if let Some(end_idx) = rest.find("\n---") {
        let frontmatter = &rest[..end_idx];
        let body = &rest[end_idx + 4..];
        Some((frontmatter, body))
    } else {
        None
    }
}

fn run_check(root: &Path) -> Result<()> {
    let available_crates = get_workspace_crates(root)?;
    let change_files = parse_change_files(root)?;

    if change_files.is_empty() {
        println!("⚠️   No change files found in .changes/");
        return Ok(());
    }

    let mut has_error = false;
    for cf in &change_files {
        println!("🔍 Verifying: {}", cf.path.display());
        for (krate, _bump) in &cf.crate_bumps {
            if !available_crates.contains_key(krate) {
                println!("❌ err in {}: Crate '{}' not in workspace.", cf.path.display(), krate);
                has_error = true;
            }
        }
        if cf.summary.is_empty() {
            println!("❌ Error in {}: Description of the change is empty.", cf.path.display());
            has_error = true;
        }
    }

    if has_error {
        Err(anyhow!("Validation of .changes/ failed."))
    } else {
        println!("✅ All {} change files are valid!", change_files.len());
        Ok(())
    }
}

fn run_bump(root: &Path, dry_run: bool) -> Result<()> {
    let available_crates = get_workspace_crates(root)?;
    let change_files = parse_change_files(root)?;

    if change_files.is_empty() {
        println!("ℹ️  No pending changes in .changes/");
        return Ok(());
    }

    // Determine the highest bump type for each crate and collect summaries
    let mut highest_bumps: HashMap<String, BumpType> = HashMap::new();
    let mut crate_summaries: HashMap<String, Vec<String>> = HashMap::new();

    for cf in &change_files {
        for (krate, bump_type) in &cf.crate_bumps {
            if !available_crates.contains_key(krate) {
                continue;
            }
            highest_bumps
                .entry(krate.clone())
                .and_modify(|existing| {
                    if *bump_type > *existing {
                        *existing = *bump_type;
                    }
                })
                .or_insert(*bump_type);

            if !cf.summary.is_empty() {
                crate_summaries
                    .entry(krate.clone())
                    .or_default()
                    .push(cf.summary.clone());
            }
        }
    }

    let today = Local::now().format("%Y-%m-%d").to_string();
    let mut new_versions: HashMap<String, String> = HashMap::new();

    println!("processing bumps:");

    for (krate, crate_dir) in &available_crates {
        if let Some(&bump_type) = highest_bumps.get(krate) {
            let cargo_toml_path = crate_dir.join("Cargo.toml");
            let content = fs::read_to_string(&cargo_toml_path)?;
            let mut doc: DocumentMut = content.parse()?;

            let current_version_str = doc["package"]["version"]
                .as_str()
                .ok_or_else(|| anyhow!("Version not found in {}", cargo_toml_path.display()))?;

            let current_version = Version::parse(current_version_str)?;
            let new_version = bump_type.apply(&current_version);
            let new_version_str = new_version.to_string();

            new_versions.insert(krate.clone(), new_version_str.clone());

            println!(
                "  📦 {krate}: {current_version_str} -> {new_version_str} ({:?})",
                bump_type
            );

            if !dry_run {
                // update Cargo.toml version
                doc["package"]["version"] = value(&new_version_str);
                fs::write(&cargo_toml_path, doc.to_string())?;

                // update CHANGELOG.md
                let summaries = crate_summaries.get(krate).cloned().unwrap_or_default();
                update_crate_changelog(crate_dir, krate, &new_version_str, &today, &summaries)?;
            }
        }
    }


    // update references of inter-crate dependencies in the workspace
    if !new_versions.is_empty() {
        println!("🔗 updating references of inter-crate dependencies...");
        for (_krate, crate_dir) in &available_crates {
            let cargo_toml_path = crate_dir.join("Cargo.toml");
            let content = fs::read_to_string(&cargo_toml_path)?;
            let mut doc: DocumentMut = content.parse()?;
            let mut modified = false;

            for dep_section in ["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(deps) = doc.get_mut(dep_section).and_then(|d| d.as_table_like_mut()) {
                    for (dep_name, dep_item) in deps.iter_mut() {
                        if let Some(new_v) = new_versions.get(dep_name.get()) {
                            if let Some(table) = dep_item.as_inline_table_mut() {
                                if table.contains_key("path") {
                                    table.insert("version", new_v.as_str().into());
                                    modified = true;
                                }
                            } else if let Some(table) = dep_item.as_table_mut() {
                                if table.contains_key("path") {
                                    table.insert("version", value(new_v.as_str()));
                                    modified = true;
                                }
                            }
                        }
                    }
                }
            }

            if modified && !dry_run {
                fs::write(&cargo_toml_path, doc.to_string())?;
            }
        }
    }

    // Apagar arquivos .changes/ processados
    if !dry_run {
        for cf in &change_files {
            fs::remove_file(&cf.path)?;
        }
        println!("🗑️  Processed files in .changes/ have been removed.");
    }

    if dry_run {
        println!("🔍 Simulation mode (dry-run). No changes were saved to disk.");
    } else {
        println!("✨ Bump completed successfully!");
    }

    Ok(())
}

fn update_crate_changelog(
    crate_dir: &Path,
    krate_name: &str,
    new_version: &str,
    date: &str,
    summaries: &[String],
) -> Result<()> {
    let changelog_path = crate_dir.join("CHANGELOG.md");
    let mut old_content = if changelog_path.exists() {
        fs::read_to_string(&changelog_path)?
    } else {
        format!("# Changelog - {krate_name}\n\nAll notable changes to this project will be documented in this file.\n\n")
    };

    let mut entry = format!("## [{new_version}] - {date}\n\n");
    for summary in summaries {
        entry.push_str(summary);
        if !summary.ends_with('\n') {
            entry.push('\n');
        }
    }
    entry.push('\n');

    // Insert the new entry after the first "## " header or at the end if not found
    if let Some(pos) = old_content.find("\n## ") {
        old_content.insert_str(pos + 1, &entry);
    } else {
        old_content.push_str(&entry);
    }

    fs::write(changelog_path, old_content)?;
    Ok(())
}

fn get_latest_changelog_notes(crate_dir: &Path, version: &str) -> String {
    let changelog_path = crate_dir.join("CHANGELOG.md");
    if !changelog_path.exists() {
        return format!("Release of crate version {version}");
    }
    let content = match fs::read_to_string(&changelog_path) {
        Ok(c) => c,
        Err(_) => return format!("Release of crate version {version}"),
    };

    let target_header = format!("## [{version}]");
    if let Some(start_idx) = content.find(&target_header) {
        let rest = &content[start_idx..];
        let body_start = rest.find('\n').map(|i| i + 1).unwrap_or(0);
        let body = &rest[body_start..];
        if let Some(next_header_idx) = body.find("\n## ") {
            body[..next_header_idx].trim().to_string()
        } else {
            body.trim().to_string()
        }
    } else {
        format!("Release of crate version {version}")
    }
}

fn create_tag_and_github_release(krate: &str, version: &str, crate_dir: &Path, execute: bool, dry_run: bool) -> Result<()> {
    let tag = format!("{krate}-v{version}");
    let notes = get_latest_changelog_notes(crate_dir, version);

    println!("  🏷️  Git Tag & GitHub Release: {tag}");

    if execute && !dry_run {
        let tag_status = std::process::Command::new("git")
            .args(["tag", "-a", &tag, "-m", &format!("Release {tag}")])
            .status();

        match tag_status {
            Ok(s) if s.success() => println!("    ✅ Local Git tag created: {tag}"),
            _ => println!("    ℹ️  Git tag {tag} already exists or could not be created locally."),
        }

        let push_status = std::process::Command::new("git")
            .args(["push", "origin", &tag])
            .status();

        match push_status {
            Ok(s) if s.success() => println!("    ✅ Pushed Git tag to remote (origin {tag})"),
            _ => println!("    ℹ️  Could not push tag {tag} to remote or tag already pushed."),
        }

        let gh_status = std::process::Command::new("gh")
            .args([
                "release",
                "create",
                &tag,
                "--title",
                &format!("{krate} v{version}"),
                "--notes",
                &notes,
            ])
            .status();

        match gh_status {
            Ok(s) if s.success() => println!("    🎉 GitHub Release created successfully for {tag}!"),
            _ => println!("    ℹ️  GitHub Release creation skipped or gh CLI unavailable."),
        }
    } else {
        println!("  [SIMULATION] git tag -a {tag} -m \"Release {tag}\"");
        println!("  [SIMULATION] git push origin {tag}");
        println!("  [SIMULATION] gh release create {tag} --title \"{krate} v{version}\" --notes \"...\"");
    }

    Ok(())
}

fn run_publish(root: &Path, dry_run: bool, execute: bool) -> Result<()> {
    let available_crates = get_workspace_crates(root)?;

    let order = ["fennec-macros", "fennec-core", "fennec-runtime", "fennec"];

    println!("Order of publication & releases:");

    for krate in &order {
        if let Some(crate_dir) = available_crates.get(*krate) {
            let cargo_toml_path = crate_dir.join("Cargo.toml");
            let content = fs::read_to_string(&cargo_toml_path)?;
            let doc: DocumentMut = content.parse()?;
            let version = doc["package"]["version"].as_str().unwrap_or("0.0.0");

            println!("\n📦 Processing crate: {krate} v{version}");

            if execute && !dry_run {
                let status = std::process::Command::new("cargo")
                    .arg("publish")
                    .arg("--package")
                    .arg(krate)
                    .current_dir(crate_dir)
                    .status()?;

                if !status.success() {
                    return Err(anyhow!("Failed to publish crate '{krate}' to crates.io"));
                }

                println!("  ⏳ Waiting for crates.io index propagation (10s)...");
                std::thread::sleep(std::time::Duration::from_secs(10));
            } else {
                println!("  [SIMULATION] cargo publish --package {krate}");
            }

            create_tag_and_github_release(krate, version, crate_dir, execute, dry_run)?;
        }
    }

    if !execute || dry_run {
        println!("\nℹ️  Simulation mode (dry-run). No crates published, tags created, or releases dispatched.");
    } else {
        println!("\n🎉 All crates published and GitHub Releases created successfully!");
    }

    Ok(())
}

fn generate_release_plan_markdown(root: &Path) -> Result<Option<String>> {
    let available_crates = get_workspace_crates(root)?;
    let change_files = parse_change_files(root)?;

    if change_files.is_empty() {
        return Ok(None);
    }

    let mut highest_bumps: HashMap<String, BumpType> = HashMap::new();
    let mut crate_summaries: HashMap<String, Vec<String>> = HashMap::new();

    for cf in &change_files {
        for (krate, bump_type) in &cf.crate_bumps {
            if !available_crates.contains_key(krate) {
                continue;
            }
            highest_bumps
                .entry(krate.clone())
                .and_modify(|existing| {
                    if *bump_type > *existing {
                        *existing = *bump_type;
                    }
                })
                .or_insert(*bump_type);

            if !cf.summary.is_empty() {
                crate_summaries
                    .entry(krate.clone())
                    .or_default()
                    .push(cf.summary.clone());
            }
        }
    }

    if highest_bumps.is_empty() {
        return Ok(None);
    }

    let mut md = String::new();
    md.push_str("# 📦 Release Packages\n\n");
    md.push_str("This PR accumulates the version updates and release notes of the crates based on the pending changes in `.changes/`.\n\n");


    md.push_str("### 📋 Summary of Version Bumps\n\n");
    md.push_str("| Crate | Version | New Version | Type |\n");
    md.push_str("| :--- | :---: | :---: | :---: |\n");

    let mut details_section = String::new();

    let mut sorted_crates: Vec<_> = highest_bumps.keys().cloned().collect();
    sorted_crates.sort();

    for krate in &sorted_crates {
        let bump_type = highest_bumps[krate];
        if let Some(crate_dir) = available_crates.get(krate) {
            let cargo_toml_path = crate_dir.join("Cargo.toml");
            let content = fs::read_to_string(&cargo_toml_path)?;
            let doc: DocumentMut = content.parse()?;
            let current_version_str = doc["package"]["version"]
                .as_str()
                .unwrap_or("0.0.0");
            let current_version = Version::parse(current_version_str)?;
            let new_version = bump_type.apply(&current_version);
            let bump_str = match bump_type {
                BumpType::Patch => "patch",
                BumpType::Minor => "minor",
                BumpType::Major => "major",
            };

            md.push_str(&format!(
                "| 📦 `{krate}` | `{current_version_str}` | `{new_version}` | `{bump_str}` |\n"
            ));

            let summaries = crate_summaries.get(krate).cloned().unwrap_or_default();
            details_section.push_str(&format!(
                "<details open>\n<summary><b>{krate}</b> (v{current_version_str} ➔ <code>v{new_version}</code>)</summary>\n\n"
            ));
            for summary in summaries {
                let clean_line = summary.trim_start_matches("- ").trim();
                details_section.push_str(&format!("- {clean_line}\n"));
            }
            details_section.push_str("\n</details>\n\n");
        }
    }

    md.push_str("\n---\n\n");
    md.push_str("### 📝 Details of Changes by Crate\n\n");
    md.push_str(&details_section);
    md.push_str("---\n\n");
    md.push_str("> **Instruction for the Maintainer**: When performing the **Merge** of this PR into the `main` branch, the packages will be published on **Crates.io**, with the automatic creation of **Git Tags** and **GitHub Releases**.\n");

    Ok(Some(md))
}

fn run_plan(root: &Path, output: Option<PathBuf>) -> Result<()> {
    let report = generate_release_plan_markdown(root)?;
    match report {
        Some(md) => {
            if let Some(out_path) = output {
                fs::write(&out_path, &md)?;
                println!("Report generated: {}", out_path.display());
            } else {
                println!("{md}");
            }
        }
        None => println!("ℹ️  No pending changes in .changes/"),
    }
    Ok(())
}

fn run_release_pr(root: &Path, base: &str) -> Result<()> {
    let report = generate_release_plan_markdown(root)?;
    let body = match report {
        Some(b) => b,
        None => {
            println!("ℹ️  No pending changes in .changes/ to generate a Release PR.");
            return Ok(());
        }
    };

    println!("📄 Report generated successfully! Applying version bumps...");

    run_bump(root, false)?;

    let pr_body_file = root.join(".changes_pr_body.md");
    fs::write(&pr_body_file, &body)?;
    
    println!("\n✨ PR Body generated:\n");
    println!("{body}");

    let pr_title = "Version Packages";

    let list_output = std::process::Command::new("gh")
        .args(["pr", "list", "--base", base, "--json", "number,title", "--jq", ".[] | select(.title | contains(\"version packages\")) | .number"])
        .output();

    if let Ok(out) = list_output {
        let pr_num = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !pr_num.is_empty() {
            println!("Update release PR #{pr_num}...");
            let edit_status = std::process::Command::new("gh")
                .args(["pr", "edit", &pr_num, "--title", pr_title, "--body-file", pr_body_file.to_str().unwrap()])
                .status();
            let _ = fs::remove_file(&pr_body_file);
            if edit_status.map_or(false, |s| s.success()) {
                println!("🎉 Release PR updated successfully!");
                return Ok(());
            }
        }
    }


    println!("🆕 Creating new Release PR on GitHub...");
    let create_status = std::process::Command::new("gh")
        .args([
            "pr",
            "create",
            "--title",
            pr_title,
            "--body-file",
            pr_body_file.to_str().unwrap(),
            "--base",
            base,
        ])
        .status();

    let _ = fs::remove_file(&pr_body_file);

    if create_status.map_or(false, |s| s.success()) {
        println!("🎉 Release PR created successfully!");
    } else {
        println!("Files bumpeds in local.");
    }

    Ok(())
}
