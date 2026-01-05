use anchor_lang::prelude::*;

declare_id!("11111111111111111111111111111111");

#[program]
pub mod overconstrained_seed_account {
    use super::*;

    // Pattern 1: Bad - seed account overconstrained as SystemAccount in non-init instruction
    pub fn withdraw(ctx: Context<Withdraw>) -> Result<()> {
        ctx.accounts.pool.amount -= 100;
        Ok(())
    }

    // Pattern 2: Good - seed account uses UncheckedAccount
    pub fn withdraw_good(ctx: Context<WithdrawGood>) -> Result<()> {
        ctx.accounts.pool.amount -= 100;
        Ok(())
    }

    // Pattern 3: Bad - multiple seed accounts, one overconstrained
    pub fn claim(ctx: Context<Claim>) -> Result<()> {
        ctx.accounts.pool.claimed = true;
        Ok(())
    }

    // Pattern 4: Good - all seed accounts use UncheckedAccount
    pub fn claim_good(ctx: Context<ClaimGood>) -> Result<()> {
        ctx.accounts.pool.claimed = true;
        Ok(())
    }

    // Pattern 5: Safe - init instruction (should not flag)
    pub fn init_pool(ctx: Context<InitPool>, amount: u64) -> Result<()> {
        ctx.accounts.pool.amount = amount;
        ctx.accounts.pool.creator = ctx.accounts.creator.key();
        Ok(())
    }

    // Pattern 6: Safe - seed account is mutable (needed for balance changes)
    pub fn withdraw_with_fee(ctx: Context<WithdrawWithFee>) -> Result<()> {
        **ctx.accounts.creator.lamports.borrow_mut() += 100;
        ctx.accounts.pool.amount -= 100;
        Ok(())
    }

    // Pattern 7: Safe - seed account is a signer (required for authorization)
    pub fn authorize(ctx: Context<Authorize>) -> Result<()> {
        ctx.accounts.pool.authority = ctx.accounts.creator.key();
        Ok(())
    }

    // Pattern 8: Safe - seed account is used outside of seeds (field access)
    pub fn update_creator(ctx: Context<UpdateCreator>) -> Result<()> {
        // Access creator account directly (not just .key())
        let _creator_info = ctx.accounts.creator.to_account_info();
        ctx.accounts.pool.creator = ctx.accounts.creator.key();
        Ok(())
    }

    // Pattern 9: Safe - seed account used as close target and has lamport mutation
    pub fn close_pool(ctx: Context<ClosePool>) -> Result<()> {
        let pool = ctx.accounts.pool.to_account_info();
        let creator = ctx.accounts.creator.to_account_info();
        **pool.lamports.borrow_mut() = 0;
        **creator.lamports.borrow_mut() = pool.lamports();
        Ok(())
    }

    // Pattern 10: Good - close with UncheckedAccount
    pub fn close_pool_good(ctx: Context<ClosePoolGood>) -> Result<()> {
        let pool = ctx.accounts.pool.to_account_info();
        let creator = ctx.accounts.creator.to_account_info();
        **pool.lamports.borrow_mut() = 0;
        **creator.lamports.borrow_mut() = pool.lamports();
        Ok(())
    }
}

// Bad: seed account overconstrained as SystemAccount
#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(
        seeds = [b"pool", creator.key().as_ref()],
        bump
    )]
    pub pool: Account<'info, Pool>, 
    pub creator: SystemAccount<'info>, // [overconstrained_seed_account]
}

// Good: seed account uses UncheckedAccount
#[derive(Accounts)]
pub struct WithdrawGood<'info> {
    #[account(
        seeds = [b"pool", creator.key().as_ref()],
        bump
    )]
    pub pool: Account<'info, Pool>,
    
    pub creator: UncheckedAccount<'info>, // [safe_seed_account]
}

// Bad: multiple seed accounts, one overconstrained
#[derive(Accounts)]
pub struct Claim<'info> {
    #[account(
        seeds = [b"pool", creator.key().as_ref(), token_mint.key().as_ref()],
        bump
    )]
    pub pool: Account<'info, Pool>, 
    pub creator: SystemAccount<'info>, // [overconstrained_seed_account]
    
    pub token_mint: UncheckedAccount<'info>, // [safe_seed_account]
}

// Good: all seed accounts use UncheckedAccount
#[derive(Accounts)]
pub struct ClaimGood<'info> {
    #[account(
        seeds = [b"pool", creator.key().as_ref(), token_mint.key().as_ref()],
        bump
    )]
    pub pool: Account<'info, Pool>,
    
    pub creator: UncheckedAccount<'info>, // [safe_seed_account]
    pub token_mint: UncheckedAccount<'info>, // [safe_seed_account]
}

// Safe: init instruction - should not flag
#[derive(Accounts)]
pub struct InitPool<'info> {
    #[account(
        init,
        seeds = [b"pool", creator.key().as_ref()],
        bump,
        payer = creator,
        space = 8 + 32 + 8
    )]
    pub pool: Account<'info, Pool>,
    
    #[account(mut)]
    pub creator: SystemAccount<'info>, // [safe_seed_account]
    
    pub system_program: Program<'info, System>,
}

// Safe: seed account is mutable (needed for balance changes)
#[derive(Accounts)]
pub struct WithdrawWithFee<'info> {
    #[account(
        seeds = [b"pool", creator.key().as_ref()],
        bump
    )]
    pub pool: Account<'info, Pool>,
    
    #[account(mut)]
    pub creator: SystemAccount<'info>, // [safe_seed_account]
}

// Safe: seed account is a signer (required for authorization)
#[derive(Accounts)]
pub struct Authorize<'info> {
    #[account(
        seeds = [b"pool", creator.key().as_ref()],
        bump
    )]
    pub pool: Account<'info, Pool>,
    
    #[account(signer)]
    pub creator: SystemAccount<'info>, // [safe_seed_account]
}

// Safe: seed account is used outside of seeds
#[derive(Accounts)]
pub struct UpdateCreator<'info> {
    #[account(
        seeds = [b"pool", creator.key().as_ref()],
        bump
    )]
    pub pool: Account<'info, Pool>,
    
    pub creator: SystemAccount<'info>, // [safe_seed_account] - used in function body
}

// Safe: seed account used as close target (close = creator)
#[derive(Accounts)]
pub struct ClosePool<'info> {
    #[account(
        mut,
        close = creator,
        seeds = [b"pool", creator.key().as_ref()],
        bump
    )]
    pub pool: Account<'info, Pool>,
    
    pub creator: SystemAccount<'info>, // [safe_seed_account] - used as close target
}

// Good: close with UncheckedAccount
#[derive(Accounts)]
pub struct ClosePoolGood<'info> {
    #[account(
        mut,
        close = creator,
        seeds = [b"pool", creator.key().as_ref()],
        bump
    )]
    pub pool: Account<'info, Pool>,
    
    pub creator: UncheckedAccount<'info>, // [safe_seed_account]
}

#[account]
pub struct Pool {
    pub amount: u64,
    pub creator: Pubkey,
    pub claimed: bool,
    pub authority: Pubkey,
}

