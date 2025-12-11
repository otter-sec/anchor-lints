# `missing_signer_validation`

### What it does
Detects when a CPI requires a signer (e.g., `authority`, `owner`, `current_authority`, `from`) but the account passed to that signer position is not validated as a signer — meaning it is neither:
- declared as a signer (Signer<'info> or #[account(signer)]), nor
- invoked as a PDA signer using CpiContext::new_with_signer.

### Why is this bad?
If a signer-required CPI is called with an account that is not properly validated, an attacker could pass an arbitrary account and perform unauthorized actions such as transferring tokens, changing authorities, minting, burning, or moving SOL — leading to severe security vulnerabilities.

