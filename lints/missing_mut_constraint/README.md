# `missing_mut_constraint`

### What it does
Detects when an account is mutated in the instruction body but not declared with `#[account(mut)]` in the Anchor accounts struct.

### Why is this bad?
Mutating an account without the `mut` constraint can cause the runtime to reject the transaction or behave unexpectedly, because the account was not marked as writable. 

### Example

**Bad:** account mutated without `#[account(mut)]`
```rust
#[derive(Accounts)]
pub struct Update<'info> {
    pub vault: Account<'info, Vault>,  // missing #[account(mut)]
}

pub fn update(ctx: Context<Update>) -> Result<()> {
    ctx.accounts.vault.amount += 1;  // mutation
    Ok(())
}
```

**Good:** account has `#[account(mut)]` when mutated
```rust
#[derive(Accounts)]
pub struct Update<'info> {
    #[account(mut)]
    pub vault: Account<'info, Vault>,
}

pub fn update(ctx: Context<Update>) -> Result<()> {
    ctx.accounts.vault.amount += 1;
    Ok(())
}
```