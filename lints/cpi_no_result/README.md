# `cpi_no_result`

### What it does
Detects Cross-Program Invocation (CPI) calls where the result is not properly handled.

### Why is this bad?
CPI calls can fail for various reasons (insufficient funds, invalid accounts, program errors, etc.). If the result is not checked, the program may continue execution even when the CPI failed, leading to silent failures, security vulnerabilities, and state corruption.
