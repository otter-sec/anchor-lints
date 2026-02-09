use anchor_lang::prelude::*;

declare_id!("PdaS1gner1111111111111111111111111111111111");

#[program]
pub mod pda_signer_account_overlap_tests {
    use super::*;

    // Unsafe account (UncheckedAccount) and PDA signer in CPI context accounts
    pub fn test_unsafe_account_with_pda_signer(
        ctx: Context<UnsafeAccountWithPdaSigner>,
    ) -> Result<()> {
        // Create CPI accounts with unsafe account
        let cpi_accounts = SomeCpiAccounts {
            user_account: ctx.accounts.user_account.to_account_info(),
            pool_authority: ctx.accounts.pool_authority.to_account_info(),
        };

        // Create CPI context with PDA signer
        let pool_key = ctx.accounts.pool.key();
        let pool_key_bytes = pool_key.to_bytes();
        let (_, pool_authority_bump) =
            Pubkey::find_program_address(&[b"pool_authority", &pool_key_bytes], ctx.program_id);

        let bump_array = [pool_authority_bump];
        let seeds: &[&[u8]] = &[b"pool_authority", &pool_key_bytes, &bump_array];

        let signer_seeds: &[&[&[u8]]] = &[seeds];

        let _cpi_ctx = CpiContext::new_with_signer(  // [pda_signer_account_overlap]
            ctx.accounts.target_program.key(),
            cpi_accounts,
            signer_seeds,
        );

        Ok(())
    }

    // Test: Safe case - No unsafe account passed to CPI (only safe accounts)
    pub fn test_safe_no_unsafe_account(
        ctx: Context<SafeAccountsOnly>,
    ) -> Result<()> {
        // Create CPI accounts with only safe accounts (no UncheckedAccount)
        let cpi_accounts = SafeCpiAccounts {
            safe_account: ctx.accounts.safe_account.to_account_info(),
            pool_authority: ctx.accounts.pool_authority.to_account_info(),
        };

        // Create CPI context with PDA signer
        let pool_key = ctx.accounts.pool.key();
        let pool_key_bytes = pool_key.to_bytes();
        let (_, pool_authority_bump) =
            Pubkey::find_program_address(&[b"pool_authority", &pool_key_bytes], ctx.program_id);

        let bump_array = [pool_authority_bump];
        let seeds: &[&[u8]] = &[b"pool_authority", &pool_key_bytes, &bump_array];

        let signer_seeds: &[&[&[u8]]] = &[seeds];

        let _cpi_ctx = CpiContext::new_with_signer(  // [safe_pda_cpi]
            ctx.accounts.target_program.key(),
            cpi_accounts,
            signer_seeds,
        );

        // The lint should NOT detect this because there's no unsafe account (UncheckedAccount)
        Ok(())
    }

    // Test: Unsafe case - Option<UncheckedAccount> passed to CPI with PDA signer
    pub fn test_unsafe_option_account(
        ctx: Context<UnsafeOptionAccount>,
    ) -> Result<()> {
        // Create CPI accounts with Option<UncheckedAccount>
        let cpi_accounts = OptionCpiAccounts {
            optional_account: ctx.accounts.optional_account.as_ref().map(|acc| acc.to_account_info()),
            pool_authority: ctx.accounts.pool_authority.to_account_info(),
        };

        // Create CPI context with PDA signer
        let pool_key = ctx.accounts.pool.key();
        let pool_key_bytes = pool_key.to_bytes();
        let (_, pool_authority_bump) =
            Pubkey::find_program_address(&[b"pool_authority", &pool_key_bytes], ctx.program_id);

        let bump_array = [pool_authority_bump];
        let seeds: &[&[u8]] = &[b"pool_authority", &pool_key_bytes, &bump_array];

        let signer_seeds: &[&[&[u8]]] = &[seeds];

        let _cpi_ctx = CpiContext::new_with_signer(  // [pda_signer_account_overlap]
            ctx.accounts.target_program.key(),
            cpi_accounts,
            signer_seeds,
        );

        // The lint should detect: optional_account (Option<UncheckedAccount>) and pool_authority (PDA) in the context
        Ok(())
    }
}

#[derive(Accounts)]
pub struct SomeCpiAccounts<'info> {
    pub user_account: AccountInfo<'info>,
    pub pool_authority: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct SafeCpiAccounts<'info> {
    pub safe_account: AccountInfo<'info>,
    pub pool_authority: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct OptionCpiAccounts<'info> {
    pub optional_account: Option<AccountInfo<'info>>,
    pub pool_authority: AccountInfo<'info>,
}

// Accounts struct with unsafe account and PDA signer
#[derive(Accounts)]
pub struct UnsafeAccountWithPdaSigner<'info> {
    #[account(mut)]
    /// CHECK: User-controlled account
    pub user_account: UncheckedAccount<'info>, // Should be detected as unsafe

    #[account(
        seeds = [b"pool_authority", pool.key().as_ref()],
        bump
    )]
    pub pool_authority: AccountInfo<'info>, // Should be detected as PDA

    pub pool: Account<'info, PoolState>,
    /// CHECK: Target program
    pub target_program: UncheckedAccount<'info>,
}

// Accounts struct with only safe accounts (no UncheckedAccount)
#[derive(Accounts)]
pub struct SafeAccountsOnly<'info> {
    pub safe_account: Account<'info, SafeAccountData>, // Safe account, not UncheckedAccount

    #[account(
        seeds = [b"pool_authority", pool.key().as_ref()],
        bump
    )]
    pub pool_authority: AccountInfo<'info>, // Should be detected as PDA

    pub pool: Account<'info, PoolState>,
    /// CHECK: Target program
    pub target_program: UncheckedAccount<'info>,
}

// Accounts struct with Option<UncheckedAccount> (unsafe)
#[derive(Accounts)]
pub struct UnsafeOptionAccount<'info> {
    #[account(mut)]
    /// CHECK: Optional user-controlled account
    pub optional_account: Option<UncheckedAccount<'info>>, // Should be detected as unsafe

    #[account(
        seeds = [b"pool_authority", pool.key().as_ref()],
        bump
    )]
    pub pool_authority: AccountInfo<'info>, // Should be detected as PDA

    pub pool: Account<'info, PoolState>,
    /// CHECK: Target program
    pub target_program: UncheckedAccount<'info>,
}

#[account]
pub struct PoolState {
    pub data: u64,
}

#[account]
pub struct SafeAccountData {
    pub value: u64,
}
