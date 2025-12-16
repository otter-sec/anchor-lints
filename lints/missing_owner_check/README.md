# `missing_owner_check`

### What it does
Detects accounts (`UncheckedAccount` or `AccountInfo`) that have their data accessed but lack owner validation.

### Why is this bad?
Missing owner validation allows attackers to pass accounts owned by unexpected programs, leading to reading or modifying data from wrong accounts, security vulnerabilities, and state corruption.

