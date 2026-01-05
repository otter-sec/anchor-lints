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

async fn run_arbitrary_cpi_call_tests() -> Result<()> {
    let lint_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_program = lint_root.join("tests/arbitrary_cpi_call");
    let span_re =
        Regex::new(r#"-->[ ]*([^\s]+\.rs):(\d+)"#).context("Failed to compile span regex")?;

    let allowed_lints = ["arbitrary_cpi_call", "safe_cpi_call"];

    let expected = collect_expected_markers(&test_program, &allowed_lints).await?;

    let out = run_dylint_command(&lint_root, &test_program, "arbitrary_cpi_call")?;

    let lint_heading = "warning: arbitrary CPI detected — program id appears user-controlled";

    let mut actual: HashSet<(String, usize)> = HashSet::new();
    let mut capture_span = false;

    for line in out.lines() {
        if line == lint_heading {
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

    let expected_warns: HashSet<_> = expected
        .get("arbitrary_cpi_call")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let expected_safe: HashSet<_> = expected
        .get("safe_cpi_call")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let missing: Vec<_> = expected_warns.difference(&actual).cloned().collect();
    if !missing.is_empty() {
        anyhow::bail!("Missing expected arbitrary CPI warnings: {:#?}", missing);
    }

    let unexpected: Vec<_> = actual.intersection(&expected_safe).cloned().collect();
    if !unexpected.is_empty() {
        anyhow::bail!(
            "Unexpected arbitrary CPI warnings (false positives): {:#?}",
            unexpected
        );
    }

    Ok(())
}

async fn run_cpi_no_result_tests() -> Result<()> {
    let lint_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_program = lint_root.join("tests/cpi_no_result");
    let span_re =
        Regex::new(r#"-->[ ]*([^\s]+\.rs):(\d+)"#).context("Failed to compile span regex")?;

    let allowed_lints = ["cpi_no_result", "safe_cpi_call"];

    let expected = collect_expected_markers(&test_program, &allowed_lints).await?;

    let out = run_dylint_command(&lint_root, &test_program, "cpi_no_result")?;

    let lint_heading = "CPI call result seems to be silently suppressed. Use `?` operator or explicit error handling instead.";

    let mut actual: HashSet<(String, usize)> = HashSet::new();
    let mut capture_span = false;

    for line in out.lines() {
        if line.contains(lint_heading) {
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

    let expected_warns: HashSet<_> = expected
        .get("cpi_no_result")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let expected_safe: HashSet<_> = expected
        .get("safe_cpi_call")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let missing: Vec<_> = expected_warns.difference(&actual).cloned().collect();
    if !missing.is_empty() {
        anyhow::bail!("Missing expected CPI no result warnings: {:#?}", missing);
    }

    let unexpected: Vec<_> = actual.intersection(&expected_safe).cloned().collect();
    if !unexpected.is_empty() {
        anyhow::bail!(
            "Unexpected CPI no result warnings (false positives): {:#?}",
            unexpected
        );
    }

    Ok(())
}

async fn run_pda_signer_account_overlap_tests() -> Result<()> {
    let lint_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_program = lint_root.join("tests/pda_signer_account_overlap");
    let span_re =
        Regex::new(r#"-->[ ]*([^\s]+\.rs):(\d+)"#).context("Failed to compile span regex")?;

    let allowed_lints = ["pda_signer_account_overlap", "safe_pda_cpi"];

    let expected = collect_expected_markers(&test_program, &allowed_lints).await?;

    let out = run_dylint_command(&lint_root, &test_program, "pda_signer_account_overlap")?;

    let lint_heading = "warning: user-controlled account passed to CPI with PDA signer";

    let mut actual: HashSet<(String, usize)> = HashSet::new();
    let mut capture_span = false;

    for line in out.lines() {
        if line.contains(lint_heading) {
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

    let expected_warns: HashSet<_> = expected
        .get("pda_signer_account_overlap")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let expected_safe: HashSet<_> = expected
        .get("safe_pda_cpi")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let missing: Vec<_> = expected_warns.difference(&actual).cloned().collect();
    if !missing.is_empty() {
        anyhow::bail!(
            "Missing expected PDA signer account overlap warnings: {:#?}",
            missing
        );
    }

    let unexpected: Vec<_> = actual.intersection(&expected_safe).cloned().collect();
    if !unexpected.is_empty() {
        anyhow::bail!(
            "Unexpected PDA signer account overlap warnings (false positives): {:#?}",
            unexpected
        );
    }

    Ok(())
}

async fn run_missing_signer_validation_tests() -> Result<()> {
    let lint_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_program = lint_root.join("tests/missing_signer_validation");
    let span_re =
        Regex::new(r#"-->[ ]*([^\s]+\.rs):(\d+)"#).context("Failed to compile span regex")?;

    let allowed_lints = ["missing_signer_validation", "safe_signer_validation"];

    let expected = collect_expected_markers(&test_program, &allowed_lints).await?;

    let out = run_dylint_command(&lint_root, &test_program, "missing_signer_validation")?;

    let lint_heading = "warning: account";

    let mut actual: HashSet<(String, usize)> = HashSet::new();
    let mut capture_span = false;

    for line in out.lines() {
        if line.contains(lint_heading)
            && line.contains("is used as a signer but lacks signer validation")
        {
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

    let expected_warns: HashSet<_> = expected
        .get("missing_signer_validation")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let expected_safe: HashSet<_> = expected
        .get("safe_signer_validation")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let missing: Vec<_> = expected_warns.difference(&actual).cloned().collect();
    if !missing.is_empty() {
        anyhow::bail!(
            "Missing expected missing signer validation warnings: {:#?}",
            missing
        );
    }

    let unexpected: Vec<_> = actual.intersection(&expected_safe).cloned().collect();
    if !unexpected.is_empty() {
        anyhow::bail!(
            "Unexpected missing signer validation warnings (false positives): {:#?}",
            unexpected
        );
    }

    Ok(())
}

async fn run_missing_owner_check_tests() -> Result<()> {
    let lint_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_program = lint_root.join("tests/missing_owner_check");
    let span_re =
        Regex::new(r#"-->[ ]*([^\s]+\.rs):(\d+)"#).context("Failed to compile span regex")?;

    let allowed_lints = ["missing_owner_check", "safe_owner_check"];

    let expected = collect_expected_markers(&test_program, &allowed_lints).await?;

    let out = run_dylint_command(&lint_root, &test_program, "missing_owner_check")?;

    let lint_heading = "warning: account";

    let mut actual: HashSet<(String, usize)> = HashSet::new();
    let mut capture_span = false;

    for line in out.lines() {
        if line.contains(lint_heading)
            && line.contains("has its data accessed but no owner validation detected")
        {
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

    let expected_warns: HashSet<_> = expected
        .get("missing_owner_check")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let expected_safe: HashSet<_> = expected
        .get("safe_owner_check")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let missing: Vec<_> = expected_warns.difference(&actual).cloned().collect();
    if !missing.is_empty() {
        anyhow::bail!(
            "Missing expected missing owner check warnings: {:#?}",
            missing
        );
    }

    let unexpected: Vec<_> = actual.intersection(&expected_safe).cloned().collect();
    if !unexpected.is_empty() {
        anyhow::bail!(
            "Unexpected missing owner check warnings (false positives): {:#?}",
            unexpected
        );
    }

    Ok(())
}

async fn run_missing_account_field_init_tests() -> Result<()> {
    let lint_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_program = lint_root.join("tests/missing_account_field_init");
    let span_re =
        Regex::new(r#"-->[ ]*([^\s]+\.rs):(\d+)"#).context("Failed to compile span regex")?;

    let allowed_lints = ["missing_account_field_init", "safe_account_field_init"];

    let expected = collect_expected_markers(&test_program, &allowed_lints).await?;

    let out = run_dylint_command(&lint_root, &test_program, "missing_account_field_init")?;

    let lint_heading = "warning: account";

    let mut actual: HashSet<(String, usize)> = HashSet::new();
    let mut capture_span = false;

    for line in out.lines() {
        if line.contains(lint_heading)
            && line.contains("is initialized but the following fields are never assigned")
        {
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

    let expected_warns: HashSet<_> = expected
        .get("missing_account_field_init")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let expected_safe: HashSet<_> = expected
        .get("safe_account_field_init")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let missing: Vec<_> = expected_warns.difference(&actual).cloned().collect();
    if !missing.is_empty() {
        anyhow::bail!(
            "Missing expected missing_account_field_init warnings: {:#?}",
            missing
        );
    }

    let unexpected: Vec<_> = actual.intersection(&expected_safe).cloned().collect();
    if !unexpected.is_empty() {
        anyhow::bail!(
            "Unexpected missing_account_field_init warnings (false positives): {:#?}",
            unexpected
        );
    }

    Ok(())
}

async fn run_ata_should_use_init_if_needed_tests() -> Result<()> {
    let lint_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_program = lint_root.join("tests/ata_should_use_init_if_needed");
    let span_re =
        Regex::new(r#"-->[ ]*([^\s]+\.rs):(\d+)"#).context("Failed to compile span regex")?;

    let allowed_lints = ["ata_should_use_init_if_needed"];

    let expected = collect_expected_markers(&test_program, &allowed_lints).await?;

    let out = run_dylint_command(&lint_root, &test_program, "ata_should_use_init_if_needed")?;

    let lint_heading = "warning: Associated Token Account";

    let mut actual: HashSet<(String, usize)> = HashSet::new();
    let mut capture_span = false;

    for line in out.lines() {
        if line.contains(lint_heading) {
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

    let expected_warns: HashSet<_> = expected
        .get("ata_should_use_init_if_needed")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let missing: Vec<_> = expected_warns.difference(&actual).cloned().collect();
    if !missing.is_empty() {
        anyhow::bail!(
            "Missing expected ata_should_use_init_if_needed warnings: {:#?}",
            missing
        );
    }

    let unexpected: Vec<_> = actual.difference(&expected_warns).cloned().collect();
    if !unexpected.is_empty() {
        anyhow::bail!(
            "Unexpected ata_should_use_init_if_needed warnings (false positives): {:#?}",
            unexpected
        );
    }

    Ok(())
}

async fn run_direct_lamport_cpi_dos_tests() -> Result<()> {
    let lint_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_program = lint_root.join("tests/direct_lamport_cpi_dos");
    let span_re =
        Regex::new(r#"-->[ ]*([^\s]+\.rs):(\d+)"#).context("Failed to compile span regex")?;

    let allowed_lints = ["direct_lamport_cpi_dos", "safe_lamport_cpi"];

    let expected = collect_expected_markers(&test_program, &allowed_lints).await?;

    let out = run_dylint_command(&lint_root, &test_program, "direct_lamport_cpi_dos")?;

    let lint_heading = "warning: account";

    let mut actual: HashSet<(String, usize)> = HashSet::new();
    let mut capture_span = false;

    for line in out.lines() {
        if line.contains(lint_heading)
            && line
                .contains("had its lamports directly mutated but is not included in this CPI call")
        {
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

    let expected_warns: HashSet<_> = expected
        .get("direct_lamport_cpi_dos")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let expected_safe: HashSet<_> = expected
        .get("safe_lamport_cpi")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let missing: Vec<_> = expected_warns.difference(&actual).cloned().collect();
    if !missing.is_empty() {
        anyhow::bail!(
            "Missing expected direct_lamport_cpi_dos warnings: {:#?}",
            missing
        );
    }

    let unexpected: Vec<_> = actual.intersection(&expected_safe).cloned().collect();
    if !unexpected.is_empty() {
        anyhow::bail!(
            "Unexpected direct_lamport_cpi_dos warnings (false positives): {:#?}",
            unexpected
        );
    }

    Ok(())
}

async fn run_overconstrained_seed_account_tests() -> Result<()> {
    let lint_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_program = lint_root.join("tests/overconstrained_seed_account");
    let span_re =
        Regex::new(r#"-->[ ]*([^\s]+\.rs):(\d+)"#).context("Failed to compile span regex")?;

    let allowed_lints = ["overconstrained_seed_account", "safe_seed_account"];

    let expected = collect_expected_markers(&test_program, &allowed_lints).await?;

    let out = run_dylint_command(&lint_root, &test_program, "overconstrained_seed_account")?;

    let lint_heading = "warning: seed-only account";

    let mut actual: HashSet<(String, usize)> = HashSet::new();
    let mut capture_span = false;

    for line in out.lines() {
        if line.contains(lint_heading) && line.contains("is overconstrained as `SystemAccount`") {
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

    let expected_warns: HashSet<_> = expected
        .get("overconstrained_seed_account")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let expected_safe: HashSet<_> = expected
        .get("safe_seed_account")
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let missing: Vec<_> = expected_warns.difference(&actual).cloned().collect();
    if !missing.is_empty() {
        anyhow::bail!(
            "Missing expected overconstrained_seed_account warnings: {:#?}",
            missing
        );
    }

    let unexpected: Vec<_> = actual.intersection(&expected_safe).cloned().collect();
    if !unexpected.is_empty() {
        anyhow::bail!(
            "Unexpected overconstrained_seed_account warnings (false positives): {:#?}",
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
