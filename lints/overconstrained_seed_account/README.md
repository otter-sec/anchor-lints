# `overconstrained_seed_account`

### What it does
Detects when a seed account used in PDA derivation is overconstrained as `SystemAccount` in non-initialization instructions.

### Why is this bad?
If a seed account's ownership changes after pool creation (e.g., becomes a token account or mint), future instructions will fail forever because `SystemAccount` enforces `owner == system_program`. This can permanently lock funds in the protocol.

### Example

```rust
// BAD: Using `SystemAccount` for a seed in a non-init instruction is the bad pattern:
#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(seeds = [b"pool", creator.key().as_ref()], bump)]
    pub pool: Account<'info, Pool>,
    pub creator: SystemAccount<'info>,  // use UncheckedAccount
}
```

Use `UncheckedAccount<'info>` (or another appropriate type) for the seed in non-init instructions so that ownership changes do not break the instruction.
