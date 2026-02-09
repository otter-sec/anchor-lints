# `pda_signer_account_overlap`

### What it does
Detects when user-controlled accounts (`UncheckedAccount` or `Option<UncheckedAccount>`) are passed to CPIs that use PDAs as signers.

### Why is this bad?
This is especially risky when the callee expects the account to be uninitialized. An attacker could pass the PDA signer itself as the account, causing the PDA to be initialized and losing its lamports, leading to security vulnerabilities.
