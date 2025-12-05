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

    // Safe case 4: CPI call with expect() - should NOT trigger lint
    pub fn transfer_with_expect(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount).expect("Transfer failed"); // [safe_cpi_call]
        Ok(())
    }

    // Safe case 5: CPI call with match - should NOT trigger lint
    pub fn transfer_with_match(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        match system_program::transfer(cpi_ctx, amount) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        } // [safe_cpi_call]
    }

    // Safe case 6: CPI call with if let - should NOT trigger lint
    pub fn transfer_with_if_let(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        if let Err(e) = system_program::transfer(cpi_ctx, amount) {
            return Err(e);
        } // [safe_cpi_call]
        Ok(())
    }

    // Safe case 7: CPI call result assigned to variable and returned - should NOT trigger lint
    pub fn transfer_assigned_and_returned(
        ctx: Context<BasicTransfer>,
        amount: u64,
    ) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        let result = system_program::transfer(cpi_ctx, amount); // [safe_cpi_call]
        result
    }

    // Safe case 8: CPI call result used with map() - should NOT trigger lint
    pub fn transfer_with_map(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount).map(|_| ())?; // [safe_cpi_call]
        Ok(())
    }

    // Unsafe case 1: CPI call with unwrap_or_default() - silently ignores errors
    pub fn transfer_with_unwrap_or_default(
        ctx: Context<BasicTransfer>,
        amount: u64,
    ) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount).unwrap_or_default(); // [cpi_no_result]
        Ok(())
    }

    // Unsafe case 2: CPI call with unwrap_or(()) - silently ignores errors
    pub fn transfer_with_unwrap_or_unit(
        ctx: Context<BasicTransfer>,
        amount: u64,
    ) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount).unwrap_or(()); // [cpi_no_result]
        Ok(())
    }

    // Unsafe case 3: CPI call with unwrap_or_else(|_| ()) - silently ignores errors
    pub fn transfer_with_unwrap_or_else(
        ctx: Context<BasicTransfer>,
        amount: u64,
    ) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount).unwrap_or_else(|_| ()); // [cpi_no_result]
        Ok(())
    }

    // Unsafe case 4: CPI call with unwrap_or_else(|_| Default::default()) - silently ignores errors
    pub fn transfer_with_unwrap_or_else_default(
        ctx: Context<BasicTransfer>,
        amount: u64,
    ) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount).unwrap_or_else(|_| Default::default()); // [cpi_no_result]
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
