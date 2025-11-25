use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod duplicate_mutable_accounts_tests {
    use super::*;

    // Unsafe: writes two mutable accounts with no guard.
    pub fn write_without_guard(
        ctx: Context<UnsafeDuplicateAccounts>,
        a: u64,
        b: u64,
    ) -> Result<()> {
        let user_a = &mut ctx.accounts.user_a;
        let user_b = &mut ctx.accounts.user_b;

        user_a.data = a;
        user_b.data = b;
        Ok(())
    }

    // Safe: explicit equality check prevents duplicates.
    pub fn write_with_if_guard(ctx: Context<IfGuardedAccounts>, a: u64, b: u64) -> Result<()> {
        if ctx.accounts.user_a.key() == ctx.accounts.user_b.key() {
            return err!(CustomError::DuplicateAccounts);
        }

        let user_a = &mut ctx.accounts.user_a;
        let user_b = &mut ctx.accounts.user_b;

        user_a.data = a;
        user_b.data = b;
        Ok(())
    }

    // Safe: uses `require!` to enforce distinct accounts.
    pub fn write_with_require_guard(
        ctx: Context<RequireGuardedAccounts>,
        a: u64,
        b: u64,
    ) -> Result<()> {
        require!(
            ctx.accounts.user_a.key() != ctx.accounts.user_b.key(),
            CustomError::DuplicateAccounts
        );

        let user_a = &mut ctx.accounts.user_a;
        let user_b = &mut ctx.accounts.user_b;

        user_a.data = a;
        user_b.data = b;
        Ok(())
    }

    // Safe: compile-time constraint keeps accounts unique.
    pub fn write_with_struct_constraint(
        ctx: Context<ConstraintGuardedAccounts>,
        a: u64,
        b: u64,
    ) -> Result<()> {
        let user_a = &mut ctx.accounts.user_a;
        let user_b = &mut ctx.accounts.user_b;

        user_a.data = a;
        user_b.data = b;
        Ok(())
    }

    // Safe: PDAs plus runtime check enforce separation.
    pub fn write_with_pda_runtime_guard(
        ctx: Context<PdaSeedCheckedAccounts>,
        a: u64,
        b: u64,
    ) -> Result<()> {
        if ctx.accounts.user_a_pda.key() == ctx.accounts.user_b_pda.key() {
            return err!(CustomError::DuplicateAccounts);
        }

        let user_a = &mut ctx.accounts.user_a_pda;
        let user_b = &mut ctx.accounts.user_b_pda;

        user_a.data = a;
        user_b.data = b;
        Ok(())
    }

    // Safe: compares a key against an AccountInfo reference.
    pub fn write_with_account_info_guard(
        ctx: Context<AccountInfoComparisonAccounts>,
        a: u64,
    ) -> Result<()> {
        if ctx.accounts.user_a.key() == ctx.accounts.user_b.to_account_info().key() {
            return err!(CustomError::DuplicateAccounts);
        }

        let user_a = &mut ctx.accounts.user_a;

        user_a.data = a;
        Ok(())
    }

    // Safe: second field is AccountInfo, not Account.
    pub fn write_with_mixed_accountinfo(ctx: Context<MixedTypeAccounts>, a: u64) -> Result<()> {
        let user_a = &mut ctx.accounts.user_a;

        user_a.data = a;
        Ok(())
    }

    // Safe: enforces uniqueness across three users.
    pub fn write_three_accounts_with_guards(
        ctx: Context<TripleCheckedAccounts>,
        a: u64,
        b: u64,
        c: u64,
    ) -> Result<()> {
        // a != b
        if ctx.accounts.user_a.key() == ctx.accounts.user_b.key() {
            return err!(CustomError::DuplicateAccounts);
        }
        // b != c
        require!(
            ctx.accounts.user_b.key() != ctx.accounts.user_c.key(),
            CustomError::DuplicateAccounts
        );

        let user_a = &mut ctx.accounts.user_a;
        let user_b = &mut ctx.accounts.user_b;
        let user_c = &mut ctx.accounts.user_c;

        user_a.data = a;
        user_b.data = b;
        user_c.data = c;
        Ok(())
    }

    // Safe: aborts via panic! if duplicates appear.
    pub fn write_with_panic_guard(
        ctx: Context<PanicGuardedAccounts>,
        a: u64,
        b: u64,
    ) -> Result<()> {
        if ctx.accounts.user_a.key() == ctx.accounts.user_b.key() {
            panic!("Accounts must be different");
        }

        let user_a = &mut ctx.accounts.user_a;
        let user_b = &mut ctx.accounts.user_b;

        user_a.data = a;
        user_b.data = b;
        Ok(())
    }

    // Safe: Account<'info, T> is inherently mutable
    #[allow(duplicate_mutable_accounts)]
    pub fn write_ignored_duplicate(
        ctx: Context<AllowAnnotatedAccounts>,
        a: u64,
        b: u64,
    ) -> Result<()> {
        let user_a = &mut ctx.accounts.user_a;
        let user_b = &mut ctx.accounts.user_b;

        user_a.data = a;
        user_b.data = b;
        Ok(())
    }

    // Safe: combined boolean ensures all pairs are distinct.
    pub fn write_three_accounts_single_guard(
        ctx: Context<TripleSinglePredicateAccounts>,
        a: u64,
        b: u64,
        c: u64,
    ) -> Result<()> {
        // a == b || b == c || a == c
        // if ctx.accounts.user_a.key() == ctx.accounts.user_b.key()
        //     || ctx.accounts.user_b.key() == ctx.accounts.user_c.key()
        //     || ctx.accounts.user_a.key() == ctx.accounts.user_c.key()
        // {
        //     return err!(CustomError::DuplicateAccounts);
        // }

        // !(a != b && b != c && c != a)
        require!(
            ctx.accounts.user_a.key() != ctx.accounts.user_b.key()
                && ctx.accounts.user_b.key() != ctx.accounts.user_c.key()
                && ctx.accounts.user_a.key() != ctx.accounts.user_c.key(),
            CustomError::DuplicateAccounts
        );
        let user_a = &mut ctx.accounts.user_a;
        let user_b = &mut ctx.accounts.user_b;
        let user_c = &mut ctx.accounts.user_c;

        user_a.data = a;
        user_b.data = b;
        user_c.data = c;
        Ok(())
    }
    // Safe: compound guard covers equality and inputs.
    pub fn write_with_if_and_threshold(
        ctx: Context<IfGuardedAccounts>,
        a: u64,
        b: u64,
    ) -> Result<()> {
        if ctx.accounts.user_a.key() == ctx.accounts.user_b.key() || a > 1 {
            return err!(CustomError::DuplicateAccounts);
        }

        let user_a = &mut ctx.accounts.user_a;
        let user_b = &mut ctx.accounts.user_b;

        user_a.data = a;
        user_b.data = b;
        Ok(())
    }

    // Safe: PDAs use distinct seeds so keys diverge.
    pub fn write_with_distinct_pda_seeds(
        ctx: Context<DistinctSeedAccounts>,
        a: u64,
        b: u64,
    ) -> Result<()> {
        // No check needed - different seeds guarantee different accounts
        let user_a = &mut ctx.accounts.user_a_pda;
        let user_b = &mut ctx.accounts.user_b_pda;

        user_a.data = a;
        user_b.data = b;
        Ok(())
    }

    // Unsafe: identical seeds allow aliasing.
    pub fn write_with_same_pda_seeds(ctx: Context<SameSeedAccounts>, a: u64, b: u64) -> Result<()> {
        // Same seeds mean they could be the same PDA - should trigger lint
        let user_a = &mut ctx.accounts.user_a_pda;
        let user_b = &mut ctx.accounts.user_b_pda;

        user_a.data = a;
        user_b.data = b;
        Ok(())
    }

    // Safe: seed ordering forces different addresses.
    pub fn write_with_reordered_seeds(
        ctx: Context<ReorderedSeedAccounts>,
        a: u64,
        b: u64,
    ) -> Result<()> {
        // Different order means different PDAs - should NOT trigger lint
        let user_a = &mut ctx.accounts.user_a_pda;
        let user_b = &mut ctx.accounts.user_b_pda;

        user_a.data = a;
        user_b.data = b;
        Ok(())
    }

    // Unsafe: mixes PDA and plain account, so keys may match.
    pub fn write_with_mixed_seeds(ctx: Context<MixedSeedAccounts>, a: u64, b: u64) -> Result<()> {
        // One PDA, one regular account - could be same - should trigger lint
        let user_a = &mut ctx.accounts.user_a_pda;
        let user_b = &mut ctx.accounts.user_b;

        user_a.data = a;
        user_b.data = b;
        Ok(())
    }

    // Unsafe: two accounts share seeds while third differs.
    pub fn write_three_accounts_mixed_seeds(
        ctx: Context<ThreeMixedSeedAccounts>,
        a: u64,
        b: u64,
        c: u64,
    ) -> Result<()> {
        // user_a_pda and user_b_pda have same seeds - should trigger for them
        // user_c_pda has different seeds - should NOT trigger for user_c_pda pairs
        let user_a = &mut ctx.accounts.user_a_pda;
        let user_b = &mut ctx.accounts.user_b_pda;
        let user_c = &mut ctx.accounts.user_c_pda;

        user_a.data = a;
        user_b.data = b;
        user_c.data = c;
        Ok(())
    }

    // Safe: same seeds but guarded by constraint.
    pub fn write_same_seeds_with_constraint(
        ctx: Context<SameSeedConstrainedAccounts>,
        a: u64,
        b: u64,
    ) -> Result<()> {
        // Same seeds but has constraint - should NOT trigger lint
        let user_a = &mut ctx.accounts.user_a_pda;
        let user_b = &mut ctx.accounts.user_b_pda;

        user_a.data = a;
        user_b.data = b;
        Ok(())
    }

    // Safe: PDAs derive under different programs.
    pub fn touch_program_separated_vaults(
        ctx: Context<ProgramSeparatedVaults>,
        a: u64,
    ) -> Result<()> {
        // Multiple AccountInfo with same seeds but different programs
        // This is safe - different programs mean different accounts
        let _ = ctx.accounts.from_vault_auth.key();
        let _ = ctx.accounts.to_vault_auth.key();
        Ok(())
    }

    // Safe: token accounts come from different mints.
    pub fn touch_token_accounts_different_mints(
        ctx: Context<TokenAccountsDifferentMints>,
        amount: u64,
    ) -> Result<()> {
        // Different mints mean different accounts - should NOT trigger
        let vault = &mut ctx.accounts.vault_token_account;
        let recipient = &mut ctx.accounts.recipient_token_account;

        let _ = vault.amount;
        let _ = recipient.amount;
        Ok(())
    }

    // Safe: PDAs have distinct seed prefixes.
    pub fn write_state_accounts_different_seeds(
        ctx: Context<StateAccountsDifferentSeeds>,
        a: u64,
        b: u64,
    ) -> Result<()> {
        // Different seeds mean different PDAs - should NOT trigger
        let earner = &mut ctx.accounts.earner_account;
        let manager = &mut ctx.accounts.earn_manager_account;

        earner.data = a;
        manager.data = b;
        Ok(())
    }

    // Unsafe: two mutable mints of same type without guard.
    pub fn touch_mutable_mint_pair(ctx: Context<MutableMintAccounts>, a: u64) -> Result<()> {
        // Multiple mutable mints of same type - should trigger
        let mint_a = &mut ctx.accounts.mint_a;
        let mint_b = &mut ctx.accounts.mint_b;
        let _ = mint_a.supply;
        let _ = mint_b.supply;
        Ok(())
    }

    // Safe: mixes Account and AccountInfo types.
    pub fn write_mixed_account_types(ctx: Context<MixedVaultAccounts>, a: u64) -> Result<()> {
        // Different types - should NOT trigger
        let global = &mut ctx.accounts.global_account;
        let vault = &ctx.accounts.vault;

        global.data = a;
        let _ = vault.key();
        Ok(())
    }

    // Safe: each account is tied to a different mint key.
    pub fn write_has_one_accounts(
        ctx: Context<HasOneDifferentiatedAccounts>,
        a: u64,
    ) -> Result<()> {
        // has_one constraints ensure different accounts - should NOT trigger
        let account_a = &mut ctx.accounts.account_a;
        let account_b = &mut ctx.accounts.account_b;

        account_a.data = a;
        account_b.data = a;
        Ok(())
    }
    // Unsafe: two token interfaces lack any constraint.
    pub fn touch_token_accounts_without_guards(
        ctx: Context<TokenAccountsWithoutConstraints>,
        amount: u64,
    ) -> Result<()> {
        // Multiple token accounts of same type - should trigger lint
        let from = &mut ctx.accounts.token_a_account;
        let to = &mut ctx.accounts.token_b_account;

        // In real code, this would transfer tokens
        // For test, just access the accounts
        let _ = from.amount;
        let _ = to.amount;
        Ok(())
    }

    // Unsafe: four mutable users with no protections.
    pub fn write_multiple_accounts_no_guard(
        ctx: Context<UnsafeMultipleAccounts>,
        a: u64,
        b: u64,
    ) -> Result<()> {
        let user_a = &mut ctx.accounts.user_a;
        let user_b = &mut ctx.accounts.user_b;
        let user_c = &mut ctx.accounts.user_c;
        let user_d = &mut ctx.accounts.user_d;

        user_a.data = a;
        user_b.data = b;
        Ok(())
    }

    // False positive case: Two accounts differentiated by has_one constraints on parent account
    pub fn touch_accounts_differentiated_by_has_one(
        ctx: Context<AccountsDifferentiatedByHasOne>,
    ) -> Result<()> {
        let _ = ctx.accounts.market_base_vault.amount;
        let _ = ctx.accounts.market_quote_vault.amount;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct UnsafeDuplicateAccounts<'info> {
    user_a: Account<'info, User>, // [duplicate_account]
    user_b: Account<'info, User>,
}

#[derive(Accounts)]
pub struct IfGuardedAccounts<'info> {
    user_a: Account<'info, User>, // [safe_account]
    user_b: Account<'info, User>,
}

#[derive(Accounts)]
pub struct RequireGuardedAccounts<'info> {
    user_a: Account<'info, User>, // [safe_account]
    user_b: Account<'info, User>,
}

#[derive(Accounts)]
pub struct ConstraintGuardedAccounts<'info> {
    #[account(constraint = user_a.key() != user_b.key())]
    user_a: Account<'info, User>, // [safe_account]
    user_b: Account<'info, User>,
}

#[derive(Accounts)]
pub struct PdaSeedCheckedAccounts<'info> {
    // [safe_account]
    #[account(mut, seeds = [b"user", user_a_pda.key().as_ref()], bump)]
    user_a_pda: Account<'info, User>,

    #[account(mut, seeds = [b"user", user_b_pda.key().as_ref()], bump)]
    user_b_pda: Account<'info, User>,

    #[account(mut)]
    payer: Signer<'info>,
}

#[derive(Accounts)]
pub struct AccountInfoComparisonAccounts<'info> {
    user_a: Account<'info, User>, // [safe_account]
    user_b: Account<'info, User>,
}

#[derive(Accounts)]
pub struct MixedTypeAccounts<'info> {
    user_a: Account<'info, User>, // [safe_account]
    user_b: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct TripleCheckedAccounts<'info> {
    user_a: Account<'info, User>, // [safe_account]
    user_b: Account<'info, User>,
    #[account(constraint = user_a.key() != user_c.key())] // a != c
    user_c: Account<'info, User>,
}

#[derive(Accounts)]
pub struct PanicGuardedAccounts<'info> {
    #[account(mut)]
    user_a: Account<'info, User>, // [safe_account]
    #[account(mut)]
    user_b: Account<'info, User>,
}

#[derive(Accounts)]
pub struct AllowAnnotatedAccounts<'info> {
    user_a: Account<'info, User>, // [safe_account]
    user_b: Account<'info, User>,
}

#[derive(Accounts)]
pub struct TripleSinglePredicateAccounts<'info> {
    user_a: Account<'info, User>, // [safe_account]
    user_b: Account<'info, User>,
    #[account(constraint = user_a.key() != user_c.key())]
    user_c: Account<'info, User>,
}

#[derive(Accounts)]
pub struct DistinctSeedAccounts<'info> {
    #[account(mut, seeds = [b"user_a", b"seed_a"], bump)]
    pub user_a_pda: Account<'info, User>, // [safe_account]

    #[account(mut, seeds = [b"user_b", b"seed_b"], bump)]
    pub user_b_pda: Account<'info, User>,
}

#[derive(Accounts)]
pub struct SameSeedAccounts<'info> {
    #[account(mut, seeds = [b"user", b"seed"], bump)]
    pub user_a_pda: Account<'info, User>, // [duplicate_account]

    #[account(mut, seeds = [b"user", b"seed"], bump)]
    pub user_b_pda: Account<'info, User>,
}

#[derive(Accounts)]
pub struct ReorderedSeedAccounts<'info> {
    #[account(mut, seeds = [b"user", b"seed"], bump)]
    pub user_a_pda: Account<'info, User>, // [safe_account]

    #[account(mut, seeds = [b"seed", b"user"], bump)]
    pub user_b_pda: Account<'info, User>,
}

#[derive(Accounts)]
pub struct MixedSeedAccounts<'info> {
    #[account(mut, seeds = [b"user", b"seed"], bump)]
    pub user_a_pda: Account<'info, User>, // [safe_account]

    #[account(mut)]
    pub user_b: Account<'info, User>, // No seeds
}

#[derive(Accounts)]
pub struct ThreeMixedSeedAccounts<'info> {
    #[account(mut, seeds = [b"user", b"seed"], bump)]
    pub user_a_pda: Account<'info, User>, // [duplicate_account]

    #[account(mut, seeds = [b"user", b"seed"], bump)]
    pub user_b_pda: Account<'info, User>,

    #[account(mut, seeds = [b"user", b"different"], bump)]
    pub user_c_pda: Account<'info, User>,
}

#[derive(Accounts)] // [account_struct][safe_account]
pub struct SameSeedConstrainedAccounts<'info> {
    #[account(mut, seeds = [b"user", b"seed"], bump)]
    pub user_a_pda: Account<'info, User>, // [safe_account]

    #[account(
        mut,
        seeds = [b"user", b"seed"],
        bump,
        constraint = user_a_pda.key() != user_b_pda.key()
    )]
    pub user_b_pda: Account<'info, User>,
}

// Real-world pattern: Swap-like with multiple AccountInfo, same seeds, different programs
#[derive(Accounts)]
pub struct ProgramSeparatedVaults<'info> {
    #[account(
        seeds = [b"vault"],
        seeds::program = program_a.key(),
        bump,
    )]
    pub from_vault_auth: AccountInfo<'info>, // [safe_account]

    #[account(
        seeds = [b"vault"],
        seeds::program = program_b.key(),
        bump,
    )]
    pub to_vault_auth: AccountInfo<'info>,

    pub program_a: Program<'info, System>,
    pub program_b: Program<'info, System>,
}

// Real-world pattern: Multiple token accounts with different mints
#[derive(Accounts)]
pub struct TokenAccountsDifferentMints<'info> {
    #[account(
        mut,
        associated_token::mint = m_mint,
        associated_token::authority = vault,
        associated_token::token_program = m_token_program,
    )]
    pub vault_token_account: InterfaceAccount<'info, TokenAccount>, // [safe_account]

    #[account(
        mut,
        token::mint = ext_mint,
        token::token_program = ext_token_program,
    )]
    pub recipient_token_account: InterfaceAccount<'info, TokenAccount>,

    pub m_mint: InterfaceAccount<'info, Mint>,
    pub ext_mint: InterfaceAccount<'info, Mint>,
    pub vault: AccountInfo<'info>,
    pub m_token_program: Interface<'info, TokenInterface>,
    pub ext_token_program: Interface<'info, TokenInterface>,
}

// Real-world pattern: Multiple state accounts with different seeds
#[derive(Accounts)]
pub struct StateAccountsDifferentSeeds<'info> {
    #[account(
        mut,
        seeds = [b"earner", user.key().as_ref()],
        bump,
    )]
    pub earner_account: Account<'info, User>, // [safe_account]

    #[account(
        mut,
        seeds = [b"manager", manager.key().as_ref()],
        bump,
    )]
    pub earn_manager_account: Account<'info, User>,

    pub user: AccountInfo<'info>,
    pub manager: AccountInfo<'info>,
}

// Real-world pattern: Multiple mutable mints
#[derive(Accounts)]
pub struct MutableMintAccounts<'info> {
    #[account(mut, mint::token_program = token_program)]
    pub mint_a: InterfaceAccount<'info, Mint>, // [duplicate_account]

    #[account(mut, mint::token_program = token_program)]
    pub mint_b: InterfaceAccount<'info, Mint>,

    pub token_program: Interface<'info, TokenInterface>,
}

// Real-world pattern: Mixed Account and AccountInfo
#[derive(Accounts)] // [account_struct][safe_account]
pub struct MixedVaultAccounts<'info> {
    #[account(
        mut,
        seeds = [b"global"],
        bump,
    )]
    pub global_account: Account<'info, User>, // [safe_account]

    #[account(
        seeds = [b"vault"],
        bump,
    )]
    pub vault: AccountInfo<'info>,
}

// Real-world pattern: Multiple accounts with different seeds (safe - different seeds ensure different accounts)
#[derive(Accounts)]
pub struct HasOneDifferentiatedAccounts<'info> {
    #[account(
        mut,
        seeds = [b"account", mint_a.key().as_ref()],
        bump,
    )]
    pub account_a: Account<'info, User>, // [safe_account]

    #[account(
        mut,
        seeds = [b"account", mint_b.key().as_ref()],
        bump,
    )]
    pub account_b: Account<'info, User>,

    pub mint_a: AccountInfo<'info>,
    pub mint_b: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct TokenAccountsWithoutConstraints<'info> {
    #[account(mut)]
    pub token_a_account: InterfaceAccount<'info, TokenAccount>, // [duplicate_account]
    #[account(mut)]
    pub token_b_account: InterfaceAccount<'info, TokenAccount>,

    #[account(mut, token::token_program = token_program_a, token::mint = token_mint_a)]
    pub token_a_vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::token_program = token_program_b, token::mint = token_mint_b)]
    pub token_b_vault: InterfaceAccount<'info, TokenAccount>,

    pub token_mint_a: InterfaceAccount<'info, Mint>,
    pub token_mint_b: InterfaceAccount<'info, Mint>,
    pub token_program_a: Interface<'info, TokenInterface>,
    pub token_program_b: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct UnsafeMultipleAccounts<'info> {
    user_a: Account<'info, User>, // [duplicate_account]
    user_b: Account<'info, User>,
    user_c: Account<'info, Wallet>, // [duplicate_account]
    user_d: Account<'info, Wallet>,
}

#[derive(Accounts)]
pub struct AccountInfoSameSeedsDifferentPrograms<'info> {
    #[account(
        mut,
        seeds = [b"vault"],
        seeds::program = program_a.key(),
        bump,
    )]
    pub from_vault_auth: AccountInfo<'info>, // [safe_account]

    #[account(
        mut,
        seeds = [b"vault"],
        seeds::program = program_b.key(),
        bump,
    )]
    pub to_vault_auth: AccountInfo<'info>, // [safe_account]

    pub program_a: Program<'info, System>,
    pub program_b: Program<'info, System>,
}

// False positive: Accounts differentiated by has_one constraints on parent account
#[derive(Accounts)]
pub struct AccountsDifferentiatedByHasOne<'info> {
    #[account(
        mut,
        has_one = market_base_vault,
        has_one = market_quote_vault,
    )]
    pub market: Account<'info, Market>,
    
    #[account(mut)]
    pub market_base_vault: Account<'info, anchor_spl::token::TokenAccount>,
    
    #[account(mut)]
    pub market_quote_vault: Account<'info, anchor_spl::token::TokenAccount>,
}

#[account]
pub struct Market {
    pub data: u64,
    pub market_base_vault: Pubkey,
    pub market_quote_vault: Pubkey,
}

#[account]
pub struct Wallet {
    data: u64,
}

#[account]
pub struct User {
    data: u64,
    m_vault_bump: u8,
    ext_mint_authority_bump: u8,
}

#[error_code]
pub enum CustomError {
    #[msg("Duplicate accounts are found")]
    DuplicateAccounts,
}
