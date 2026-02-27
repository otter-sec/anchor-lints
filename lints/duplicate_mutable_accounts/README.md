# `duplicate_mutable_accounts`

### What it does
Detects duplicate mutable account usage in Anchor functions, where the same account type is passed into multiple mutable parameters without constraint checks.

### Why is this bad?
Duplicate mutable accounts can lead to unexpected aliasing of mutable data, logical errors, and vulnerabilities like account state corruption in Solana smart contracts.

### Example

**Flagged:** two mutable accounts of the same type, no constraint
```rust
#[derive(Accounts)]
pub struct UnsafeAccounts<'info> {
    pub user_a: Account<'info, User>,  // [duplicate_mutable_accounts]
    pub user_b: Account<'info, User>,
}

pub fn update(ctx: Context<UnsafeAccounts>, a: u64, b: u64) -> Result<()> {
    ctx.accounts.user_a.data = a;
    ctx.accounts.user_b.data = b;
    Ok(())
}
```

**OK:** same-type mutable accounts with a constraint so they must be different
```rust
#[derive(Accounts)]
pub struct SafeAccounts<'info> {
    #[account(constraint = user_a.key() != user_b.key())]
    pub user_a: Account<'info, User>,
    pub user_b: Account<'info, User>,
}

pub fn update(ctx: Context<SafeAccounts>, a: u64, b: u64) -> Result<()> {
    ctx.accounts.user_a.data = a;
    ctx.accounts.user_b.data = b;
    Ok(())
}
```