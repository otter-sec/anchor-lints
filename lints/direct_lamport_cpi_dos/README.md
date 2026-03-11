# `direct_lamport_cpi_dos`

### What it does
Flags when an account’s lamports are mutated (e.g. `**ctx.accounts.fee_collector.lamports.borrow_mut() += x`) and that account is not included in a later CPI in the same function.

### Why it matters
The runtime checks balances for accounts in the CPI. If you changed an account’s lamports but don’t pass it in the CPI, the tx will abort. The runtime catches this too; the lint gives earlier feedback and a clearer message so you can fix it before running the failing path. Include every mutated account in the CPI, e.g. via `with_remaining_accounts`.

### Example

**Flagged:** lamport mutation then CPI without that account
```rust
**ctx.accounts.vault.lamports.borrow_mut() -= WITHDRAW_FEE;
**ctx.accounts.fee_collector.lamports.borrow_mut() += WITHDRAW_FEE;
// fee_collector not in CPI
token::transfer(
    CpiContext::new(ctx.accounts.token_program.key(), Transfer { ... }),
    amount,
)?;

**OK():** same mutations, account included in CPI
```rust
**ctx.accounts.vault.lamports.borrow_mut() -= WITHDRAW_FEE;
**ctx.accounts.fee_collector.lamports.borrow_mut() += WITHDRAW_FEE;

token::transfer(
    CpiContext::new(...).with_remaining_accounts(vec![ctx.accounts.fee_collector.to_account_info()]),
    amount,
)?;
```
