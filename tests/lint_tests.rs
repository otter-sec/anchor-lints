use anyhow::{Context, Result};
use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Command,
};
use tokio::fs;

struct DylintOutput {
    combined: String,
    stderr: String,
}

macro_rules! bail_with_stderr {
    ($stderr:expr, $($arg:tt)*) => {{
        let message = format!($($arg)*);
        let message = if $stderr.trim().is_empty() {
            message
        } else {
            format!("--- dylint stderr ---\n{}\n\n{message}\n", $stderr)
        };
        anyhow::bail!(message);
    }};
}

#[tokio::test]
async fn missing_account_reload_tests() -> Result<()> {
    run_missing_account_reload_tests().await
}

#[tokio::test]
async fn duplicate_mutable_accounts_tests() -> Result<()> {
    run_duplicate_mutable_accounts_tests().await
}

#[tokio::test]
async fn arbitrary_cpi_call_tests() -> Result<()> {
    run_arbitrary_cpi_call_tests().await
}

#[tokio::test]
async fn cpi_no_result_tests() -> Result<()> {
    run_cpi_no_result_tests().await
}

#[tokio::test]
async fn pda_signer_account_overlap_tests() -> Result<()> {
    run_pda_signer_account_overlap_tests().await
}

#[tokio::test]
async fn missing_signer_validation_tests() -> Result<()> {
    run_missing_signer_validation_tests().await
}

#[tokio::test]
async fn missing_owner_check_tests() -> Result<()> {
    run_missing_owner_check_tests().await
}

#[tokio::test]
async fn missing_account_field_init_tests() -> Result<()> {
    run_missing_account_field_init_tests().await
}

#[tokio::test]
async fn ata_should_use_init_if_needed_tests() -> Result<()> {
    run_ata_should_use_init_if_needed_tests().await
}

#[tokio::test]
async fn direct_lamport_cpi_dos_tests() -> Result<()> {
    run_direct_lamport_cpi_dos_tests().await
}

#[tokio::test]
async fn overconstrained_seed_account_tests() -> Result<()> {
    run_overconstrained_seed_account_tests().await
}

#[tokio::test]
async fn unsafe_pyth_price_account_tests() -> Result<()> {
    run_unsafe_pyth_price_account_tests().await
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
    let stderr = out.stderr.clone();
    let out = out.combined;

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

        if let Some(kind) = previous_line.take()
            && let Some(cap) = span_re.captures(line)
        {
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

    if expected.is_empty() && !actual.is_empty() {
        bail_with_stderr!(
            stderr,
            "No expected lints found, but actual results were produced."
        );
    }

    // Match expected vs actual
    for (lint, expected_spans) in expected {
        let key = match lint.as_str() {
            "unsafe_account_accessed" => "data_access",
            "cpi_call" => "cpi_call",
            "safe_account_accessed" => "data_access",
            _ => bail_with_stderr!(stderr, "Invalid lint name: {}", lint),
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
                bail_with_stderr!(
                    stderr,
                    "Unexpected warnings for `{}`:\n{:#?}",
                    lint,
                    actual_set
                );
            }
            continue;
        }

        let missing: Vec<_> = expected_set.difference(&actual_set).cloned().collect();
        let unexpected: Vec<_> = actual_set.difference(&expected_set).cloned().collect();

        if !missing.is_empty() || !unexpected.is_empty() {
            bail_with_stderr!(
                stderr,
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
    let stderr = out.stderr.clone();
    let out = out.combined;

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
        bail_with_stderr!(
            stderr,
            "No expected lints found, but actual results were produced."
        );
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
        bail_with_stderr!(
            stderr,
            "Missing expected duplicate-account warnings: {:#?}",
            missing
        );
    }

    let unexpected = actual_dups
        .intersection(&expected_safe)
        .cloned()
        .collect::<Vec<_>>();

    if !unexpected.is_empty() {
        bail_with_stderr!(
            stderr,
            "Unexpected duplicate-account warnings (false positives): {:#?}",
            unexpected
        );
    }

    Ok(())
}

async fn run_arbitrary_cpi_call_tests() -> Result<()> {
    run_standard_lint_test(
        "arbitrary_cpi_call",
        &["arbitrary_cpi_call", "safe_cpi_call"],
        "warning: arbitrary CPI detected — program id appears user-controlled",
        None,
        "arbitrary CPI",
    )
    .await
}

async fn run_cpi_no_result_tests() -> Result<()> {
    run_standard_lint_test(
        "cpi_no_result",
        &["cpi_no_result", "safe_cpi_call"],
        "CPI call result seems to be silently suppressed. Use `?` operator or explicit error handling instead.",
        None,
        "CPI no result",
    )
    .await
}

async fn run_pda_signer_account_overlap_tests() -> Result<()> {
    run_standard_lint_test(
        "pda_signer_account_overlap",
        &["pda_signer_account_overlap", "safe_pda_cpi"],
        "warning: user-controlled account passed to CPI with PDA signer",
        None,
        "PDA signer account overlap",
    )
    .await
}

async fn run_missing_signer_validation_tests() -> Result<()> {
    run_standard_lint_test(
        "missing_signer_validation",
        &["missing_signer_validation", "safe_signer_validation"],
        "warning: account",
        Some("is used as a signer but lacks signer validation"),
        "missing signer validation",
    )
    .await
}

async fn run_missing_owner_check_tests() -> Result<()> {
    run_standard_lint_test(
        "missing_owner_check",
        &["missing_owner_check", "safe_owner_check"],
        "warning: account",
        Some("has its data accessed but no owner validation detected"),
        "missing owner check",
    )
    .await
}

async fn run_missing_account_field_init_tests() -> Result<()> {
    run_standard_lint_test(
        "missing_account_field_init",
        &["missing_account_field_init", "safe_account_field_init"],
        "warning: account",
        Some("is initialized but the following fields are never assigned"),
        "missing_account_field_init",
    )
    .await
}

async fn run_ata_should_use_init_if_needed_tests() -> Result<()> {
    run_standard_lint_test(
        "ata_should_use_init_if_needed",
        &["ata_should_use_init_if_needed"],
        "warning: Associated Token Account",
        None,
        "ata_should_use_init_if_needed",
    )
    .await
}

async fn run_direct_lamport_cpi_dos_tests() -> Result<()> {
    run_standard_lint_test(
        "direct_lamport_cpi_dos",
        &["direct_lamport_cpi_dos", "safe_lamport_cpi"],
        "warning: account",
        Some("had its lamports directly mutated but is not included in this CPI call"),
        "direct_lamport_cpi_dos",
    )
    .await
}

async fn run_overconstrained_seed_account_tests() -> Result<()> {
    run_standard_lint_test(
        "overconstrained_seed_account",
        &["overconstrained_seed_account", "safe_seed_account"],
        "warning: seed-only account",
        Some("is overconstrained as `SystemAccount`"),
        "overconstrained_seed_account",
    )
    .await
}

async fn run_unsafe_pyth_price_account_tests() -> Result<()> {
    run_standard_lint_test(
        "unsafe_pyth_price_account",
        &["unsafe_account_accessed", "safe_account_accessed"],
        "warning: Pyth PriceUpdateV2 account",
        None,
        "unsafe_pyth_price_account",
    )
    .await
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

/// Generic helper function for standard lint tests that follow the common pattern:
async fn run_standard_lint_test(
    lint_name: &str,
    allowed_lints: &[&str],
    lint_heading: &str,
    additional_text: Option<&str>,
    lint_display_name: &str,
) -> Result<()> {
    let lint_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_program = lint_root.join(format!("tests/{}", lint_name));
    let span_re =
        Regex::new(r#"-->[ ]*([^\s]+\.rs):(\d+)"#).context("Failed to compile span regex")?;

    let expected = collect_expected_markers(&test_program, allowed_lints).await?;
    let out = run_dylint_command(&lint_root, &test_program, lint_name)?;
    let stderr = out.stderr.clone();
    let out = out.combined;

    let mut actual: HashSet<(String, usize)> = HashSet::new();
    let mut capture_span = false;

    for line in out.lines() {
        let matches_heading = if let Some(additional) = additional_text {
            line.contains(lint_heading) && line.contains(additional)
        } else {
            line == lint_heading || line.contains(lint_heading)
        };

        if matches_heading {
            capture_span = true;
            continue;
        }

        if capture_span {
            if let Some(cap) = span_re.captures(line) {
                let file = cap.get(1).unwrap().as_str().to_string();
                let line_no: usize = cap
                    .get(2)
                    .unwrap()
                    .as_str()
                    .parse()
                    .context("Invalid line number")?;
                actual.insert((file, line_no));
            }
            capture_span = false;
        }
    }

    let warn_key = allowed_lints[0];
    let safe_key = allowed_lints.get(1).copied();

    let expected_warns: HashSet<_> = expected
        .get(warn_key)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();

    if let Some(safe) = safe_key {
        let expected_safe: HashSet<_> = expected
            .get(safe)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect();

        let missing: Vec<_> = expected_warns.difference(&actual).cloned().collect();
        if !missing.is_empty() {
            bail_with_stderr!(
                stderr,
                "Missing expected {} warnings: {:#?}",
                lint_display_name,
                missing
            );
        }

        let unexpected: Vec<_> = actual.intersection(&expected_safe).cloned().collect();
        if !unexpected.is_empty() {
            bail_with_stderr!(
                stderr,
                "Unexpected {} warnings (false positives): {:#?}",
                lint_display_name,
                unexpected
            );
        }
    } else {
        // No safe variant, compare against expected_warns directly
        let missing: Vec<_> = expected_warns.difference(&actual).cloned().collect();
        if !missing.is_empty() {
            bail_with_stderr!(
                stderr,
                "Missing expected {} warnings: {:#?}",
                lint_display_name,
                missing
            );
        }

        let unexpected: Vec<_> = actual.difference(&expected_warns).cloned().collect();
        if !unexpected.is_empty() {
            bail_with_stderr!(
                stderr,
                "Unexpected {} warnings (false positives): {:#?}",
                lint_display_name,
                unexpected
            );
        }
    }

    Ok(())
}

fn run_dylint_command(lint_root: &Path, test_program: &Path, lint_name: &str) -> Result<DylintOutput> {
    let output = Command::new("cargo")
        .arg("dylint")
        .arg("--path")
        .arg(lint_root.join("lints"))
        .current_dir(test_program)
        .arg("--pattern")
        .arg(lint_name)
        .output()
        .with_context(|| "Failed to run `cargo dylint`. Is dylint installed?")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{}\n{}", stdout, stderr);

    Ok(DylintOutput { combined, stderr })
}
