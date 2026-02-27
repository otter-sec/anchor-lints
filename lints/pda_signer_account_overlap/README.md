# `pda_signer_account_overlap`

### What it does
Warns when a **mutable user-controlled account** (`UncheckedAccount` or `Option<UncheckedAccount>`) is passed into a CPI that also uses a **PDA as a signer** (e.g. `CpiContext::new_with_signer`). The lint only fires if both show up in the same CPI and the account is mutable.

### Why it matters
If the callee program expects an account to be uninitialized and then initializes it inside the CPI (e.g. with `invoke_signed`), an attacker could pass the PDA signer itself—or another account they control—into that slot. The callee might then initialize it with the program’s authority, which can lock lamports or break security assumptions. This pattern has appeared in real bugs (e.g. Meteora-style). The risk is highest when the callee creates or initializes accounts in the CPI.

### Example

**Bad:** A mutable `UncheckedAccount` and a PDA are both passed to a CPI that uses a signer. The user can supply any account (including the PDA) for `user_account`.

```rust
#[derive(Accounts)]
pub struct UnsafeAccountWithPdaSigner<'info> {
    #[account(mut)]
    pub user_account: UncheckedAccount<'info>,  // user-controlled

    #[account(seeds = [b"pool_authority", pool.key().as_ref()], bump)]
    pub pool_authority: AccountInfo<'info>,     // PDA used as signer

    pub pool: Account<'info, PoolState>,
    pub target_program: UncheckedAccount<'info>,
}

// In the instruction: both user_account and pool_authority are passed to CpiContext::new_with_signer(...)
// So lint warns: user-controlled account passed to CPI with PDA signer
```

**Good:** Either avoid passing mutable `UncheckedAccount` in the same CPI as the PDA signer, or add a constraint so the unchecked account cannot be the PDA (e.g. `constraint = user_account.key() != pool_authority.key()`). If the account is not mutable, the lint does not warn.
