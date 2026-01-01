# `direct_lamport_cpi_dos`

### What it does
Detects when accounts with direct lamport mutations (via `lamports.borrow_mut()`) are not included in subsequent CPI calls.

### Why is this bad?
The Solana runtime performs balance checks on CPI calls using only the accounts involved in the CPI. When an account's lamports are directly mutated but the account is not included in the CPI, the runtime balance check will fail, causing a DoS error. All accounts whose lamports were changed directly must be included in subsequent CPIs as remaining accounts using `with_remaining_accounts`.
