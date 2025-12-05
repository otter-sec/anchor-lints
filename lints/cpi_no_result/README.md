# `cpi_no_result`

### What it does
Detects CPI calls where the result is silently suppressed using methods like `unwrap_or_default()` or `unwrap_or(())`.

### Why is this bad?
Silent suppression methods hide CPI failures, allowing the program to continue execution even when critical operations failed, leading to silent failures, security vulnerabilities, potential fund loss, and state corruption.
