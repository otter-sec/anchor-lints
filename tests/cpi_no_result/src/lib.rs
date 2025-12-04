use anchor_lang::prelude::*;
use anchor_lang::system_program::{self, Transfer};

declare_id!("11111111111111111111111111111112");

#[program]
pub mod cpi_no_result_tests {
    use super::*;

    // Safe case 1: CPI call with ? operator - should NOT trigger lint
    pub fn transfer_with_question_mark(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]
        Ok(())
    }

    // Safe case 2: CPI call with unwrap() - should NOT trigger lint
    pub fn transfer_with_unwrap(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount).unwrap(); // [safe_cpi_call]
        Ok(())
    }

    // Safe case 3: CPI call explicitly discarded with let _ = - should NOT trigger lint
    pub fn transfer_explicitly_discarded(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        let _ = system_program::transfer(cpi_ctx, amount); // [safe_cpi_call]
        Ok(())
    }

    // CPI call with unwrap_or_default() - silently ignores errors
    pub fn transfer_with_unwrap_or_default(
        ctx: Context<BasicTransfer>,
        amount: u64,
    ) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount).unwrap_or_default(); // [cpi_no_result]
        Ok(())
    }
}

fn build_transfer_context<'info>(
    ctx: &Context<BasicTransfer<'info>>,
) -> CpiContext<'info, 'info, 'info, 'info, Transfer<'info>> {
    let cpi_accounts = Transfer {
        from: ctx.accounts.from.to_account_info(),
        to: ctx.accounts.to.to_account_info(),
    };
    CpiContext::new(ctx.accounts.system_program.key(), cpi_accounts)
}

#[derive(Accounts)]
pub struct BasicTransfer<'info> {
    #[account(mut)]
    pub from: Signer<'info>,
    #[account(mut)]
    pub to: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct TokenTransfer<'info> {
    #[account(mut)]
    pub from: AccountInfo<'info>,
    #[account(mut)]
    pub to: AccountInfo<'info>,
    pub authority: Signer<'info>,
    pub token_program: Program<'info, anchor_spl::token::Token>,
}

#[error_code]
pub enum CustomError {
    #[msg("Invalid program")]
    InvalidProgram,
    #[msg("Transfer failed")]
    TransferFailed,
}
