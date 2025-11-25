use anyhow::{Context, Result};
use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Command,
};
use tokio::fs;

#[tokio::test]
async fn missing_account_reload_tests() -> Result<()> {
    run_missing_account_reload_tests().await
}

#[tokio::test]
async fn duplicate_mutable_accounts_tests() -> Result<()> {
    run_duplicate_mutable_accounts_tests().await
}

async fn run_missing_account_reload_tests() -> Result<()> {
    let lint_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_program = lint_root.join("tests/missing_account_reload_tests");
    let span_re =
        Regex::new(r#"-->[ ]*([^\s]+\.rs):(\d+)"#).context("Failed to compile span regex")?;

    let allowed_lints = [
        "unsafe_account_accessed",
        "cpi_call",
        "safe_account_accessed",
    ];

    let expected = collect_expected_markers(&test_program, &allowed_lints).await?;

    let out = run_dylint_command(&lint_root, &test_program, "missing_account_reload")?;

    #[derive(Debug)]
    enum OutputTypes {
        DataAccess,
        CpiCall,
    }

    let mut actual: HashMap<String, HashSet<(String, usize)>> = HashMap::new();
    let mut previous_line: Option<OutputTypes> = None;

    let lint_heading = "warning: accessing an account after a CPI without calling `reload()`";

    // Parse `cargo dylint` output
    for line in out.lines() {
        match line {
            x if x == lint_heading => {
                previous_line = Some(OutputTypes::DataAccess);
                continue;
            }
            "note: CPI is here" => {
                previous_line = Some(OutputTypes::CpiCall);
                continue;
            }
            _ => {}
        }

        if let Some(kind) = previous_line.take() {
            if let Some(cap) = span_re.captures(line) {
                let file = cap.get(1).unwrap().as_str().to_string();
                let line_no: usize = cap
                    .get(2)
                    .unwrap()
                    .as_str()
                    .parse()
                    .context("Invalid line number")?;

                match kind {
                    OutputTypes::DataAccess => {
                        actual
                            .entry("data_access".into())
                            .or_default()
                            .insert((file, line_no));
                    }
                    OutputTypes::CpiCall => {
                        actual
                            .entry("cpi_call".into())
                            .or_default()
                            .insert((file, line_no));
                    }
                }
            }
        }
    }

    if expected.is_empty() && !actual.is_empty() {
        anyhow::bail!("No expected lints found, but actual results were produced.");
    }

    // Match expected vs actual
    for (lint, expected_spans) in expected {
        let key = match lint.as_str() {
            "unsafe_account_accessed" => "data_access",
            "cpi_call" => "cpi_call",
            "safe_account_accessed" => "data_access",
            _ => anyhow::bail!("Invalid lint name: {}", lint),
        }
        .to_string();

        let expected_set: HashSet<_> = expected_spans.into_iter().collect();
        let actual_set = actual.get(&key).cloned().unwrap_or_default();
        let actual_safe_set = actual
            .get("safe_account_accessed")
            .cloned()
            .unwrap_or_default();

        if lint == "safe_account_accessed" {
            if !actual_safe_set.is_empty() {
                anyhow::bail!("Unexpected warnings for `{}`:\n{:#?}", lint, actual_set);
            }
            continue;
        }

        let missing: Vec<_> = expected_set.difference(&actual_set).cloned().collect();
        let unexpected: Vec<_> = actual_set.difference(&expected_set).cloned().collect();

        if !missing.is_empty() || !unexpected.is_empty() {
            anyhow::bail!(
                "Lint `{}` mismatch\nMissing: {:#?}\nUnexpected: {:#?}",
                lint,
                missing,
                unexpected
            );
        }
    }

    Ok(())
}

async fn run_duplicate_mutable_accounts_tests() -> Result<()> {
    let lint_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_program = lint_root.join("tests/duplicate_mutable_accounts");
    let span_re =
        Regex::new(r#"-->[ ]*([^\s]+\.rs):(\d+)"#).context("Failed to compile span regex")?;

    let allowed_lints = ["duplicate_account", "safe_account"];

    let expected = collect_expected_markers(&test_program, &allowed_lints).await?;

    let out = run_dylint_command(&lint_root, &test_program, "duplicate_mutable_accounts")?;

    let mut actual: HashMap<String, HashSet<(String, usize)>> = HashMap::new();
    let mut previous_line_lint_warn: bool = false;

    let duplicate_mutable_account_regex =
        Regex::new(r#"help: `(\w+)` and `(\w+)` may refer to the same account\."#)
            .context("Failed to compile duplicate mutable account regex")?;

    // Parse `cargo dylint` output
    for line in out.lines() {
        if duplicate_mutable_account_regex.is_match(line) {
            previous_line_lint_warn = true;
            continue;
        }

        if previous_line_lint_warn {
            if let Some(cap) = span_re.captures(line) {
                let file = cap.get(1).unwrap().as_str().to_string();
                let line_no: usize = cap
                    .get(2)
                    .unwrap()
                    .as_str()
                    .parse()
                    .context("Invalid line number")?;
                actual
                    .entry("duplicate_account".into())
                    .or_default()
                    .insert((file, line_no));
                previous_line_lint_warn = false; // reset the flag
            }
        } else {
            previous_line_lint_warn = false;
        }
    }
    if expected.is_empty() && !actual.is_empty() {
        anyhow::bail!("No expected lints found, but actual results were produced.");
    }

    let actual_dups: HashSet<_> = actual.get("duplicate_account").cloned().unwrap_or_default();

    let expected_dups: HashSet<_> = expected
        .get("duplicate_account")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let expected_safe: HashSet<_> = expected
        .get("safe_account")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let missing = expected_dups
        .difference(&actual_dups)
        .cloned()
        .collect::<Vec<_>>();

    if !missing.is_empty() {
        anyhow::bail!(
            "Missing expected duplicate-account warnings: {:#?}",
            missing
        );
    }

    let unexpected = actual_dups
        .intersection(&expected_safe)
        .cloned()
        .collect::<Vec<_>>();

    if !unexpected.is_empty() {
        anyhow::bail!(
            "Unexpected duplicate-account warnings (false positives): {:#?}",
            unexpected
        );
    }

    Ok(())
}

// Recursively find all .rs files
fn find_rust_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            files.extend(find_rust_files(&path)?);
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            files.push(path);
        }
    }

    Ok(files)
}

async fn collect_expected_markers(
    test_program: &Path,
    allowed_lints: &[&str],
) -> Result<HashMap<String, Vec<(String, usize)>>> {
    let src_root = test_program.join("src");
    let marker_regex = Regex::new(r#"\[(\w+)\]"#).context("Failed to compile marker regex")?;
    let mut expected: HashMap<String, Vec<(String, usize)>> = HashMap::new();

    let rust_files =
        find_rust_files(&src_root).context("Failed to scan Rust files in test program")?;

    for file in rust_files {
        let content = fs::read_to_string(&file)
            .await
            .with_context(|| format!("Failed to read file: {}", file.display()))?;

        for (idx, line) in content.lines().enumerate() {
            for caps in marker_regex.captures_iter(line) {
                let lint_name = caps.get(1).unwrap().as_str().to_string();

                if allowed_lints.contains(&lint_name.as_str()) {
                    let relative = file
                        .strip_prefix(test_program)
                        .context("Failed to compute relative path")?
                        .to_string_lossy()
                        .to_string();

                    expected
                        .entry(lint_name)
                        .or_default()
                        .push((relative, idx + 1));
                }
            }
        }
    }

    Ok(expected)
}

fn run_dylint_command(lint_root: &Path, test_program: &Path, lint_name: &str) -> Result<String> {
    let output = Command::new("cargo")
        .arg("dylint")
        .arg("--path")
        .arg(lint_root.join("lints"))
        .current_dir(test_program)
        .arg("--pattern")
        .arg(lint_name)
        .output()
        .with_context(|| "Failed to run `cargo dylint`. Is dylint installed?")?;

    Ok(format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}
