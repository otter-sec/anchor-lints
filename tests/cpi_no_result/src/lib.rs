use anchor_lang::prelude::*;
use anchor_lang::system_program::{self, Transfer};

declare_id!("11111111111111111111111111111112");

#[program]
pub mod cpi_no_result_tests {
    use super::*;

    // Case 1: system_program::transfer without result handling - should trigger lint
    pub fn transfer_without_result(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        let _ = system_program::transfer(cpi_ctx, amount); // [cpi_no_result]
        Ok(())
    }

    // Case 2: system_program::transfer with ? operator - should NOT trigger lint
    pub fn transfer_with_question_mark(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]
        Ok(())
    }

    // Case 3: system_program::transfer with unwrap() - should NOT trigger lint
    pub fn transfer_with_unwrap(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount).unwrap(); // [safe_cpi_call]
        Ok(())
    }

    // Case 4: system_program::transfer with expect() - should NOT trigger lint
    pub fn transfer_with_expect(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount).expect("Transfer failed"); // [safe_cpi_call]
        Ok(())
    }

    // Case 5: system_program::transfer with match - should NOT trigger lint
    pub fn transfer_with_match(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        match system_program::transfer(cpi_ctx, amount) {
            // [safe_cpi_call]
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }

    // Case 6: system_program::transfer with if let - should NOT trigger lint
    pub fn transfer_with_if_let(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        if let Err(e) = system_program::transfer(cpi_ctx, amount) {
            // [safe_cpi_call]
            return Err(e);
        }
        Ok(())
    }

    // Case 7: system_program::transfer assigned to variable but not used - should trigger lint
    pub fn transfer_assigned_not_used(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        let _result = system_program::transfer(cpi_ctx, amount); // [cpi_no_result]
        Ok(())
    }

    // Case 8: system_program::transfer assigned and used - should NOT trigger lint
    pub fn transfer_assigned_and_used(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        let result = system_program::transfer(cpi_ctx, amount); // [safe_cpi_call]
        result?;
        Ok(())
    }

    // Case 9: Direct invoke without result handling - should trigger lint
    pub fn direct_invoke_without_result(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        use anchor_lang::solana_program::program::invoke;
        use anchor_lang::solana_program::system_instruction::transfer;

        let instruction = transfer(&ctx.accounts.from.key(), &ctx.accounts.to.key(), amount);

        let account_infos = vec![
            ctx.accounts.from.to_account_info(),
            ctx.accounts.to.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ];

        let _ = invoke(&instruction, &account_infos); // [cpi_no_result]
        Ok(())
    }

    // Case 10: Direct invoke with ? operator - should NOT trigger lint
    pub fn direct_invoke_with_question_mark(
        ctx: Context<BasicTransfer>,
        amount: u64,
    ) -> Result<()> {
        use anchor_lang::solana_program::program::invoke;
        use anchor_lang::solana_program::system_instruction::transfer;

        let instruction = transfer(&ctx.accounts.from.key(), &ctx.accounts.to.key(), amount);

        let account_infos = vec![
            ctx.accounts.from.to_account_info(),
            ctx.accounts.to.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ];

        invoke(&instruction, &account_infos)?; // [safe_cpi_call]
        Ok(())
    }

    // Case 11: Direct invoke_signed without result handling - should trigger lint
    pub fn direct_invoke_signed_without_result(
        ctx: Context<BasicTransfer>,
        amount: u64,
    ) -> Result<()> {
        use anchor_lang::solana_program::program::invoke_signed;
        use anchor_lang::solana_program::system_instruction::transfer;

        let instruction = transfer(&ctx.accounts.from.key(), &ctx.accounts.to.key(), amount);

        let account_infos = vec![
            ctx.accounts.from.to_account_info(),
            ctx.accounts.to.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ];

        let signer_seeds: &[&[&[u8]]] = &[];
        let _ = invoke_signed(&instruction, &account_infos, signer_seeds); // [cpi_no_result]
        Ok(())
    }

    // Case 12: Direct invoke_signed with ? operator - should NOT trigger lint
    pub fn direct_invoke_signed_with_question_mark(
        ctx: Context<BasicTransfer>,
        amount: u64,
    ) -> Result<()> {
        use anchor_lang::solana_program::program::invoke_signed;
        use anchor_lang::solana_program::system_instruction::transfer;

        let instruction = transfer(&ctx.accounts.from.key(), &ctx.accounts.to.key(), amount);

        let account_infos = vec![
            ctx.accounts.from.to_account_info(),
            ctx.accounts.to.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ];

        let signer_seeds: &[&[&[u8]]] = &[];
        invoke_signed(&instruction, &account_infos, signer_seeds)?; // [safe_cpi_call]
        Ok(())
    }

    // Case 13: anchor_spl::token::transfer without result handling - should trigger lint
    pub fn token_transfer_without_result(ctx: Context<TokenTransfer>, amount: u64) -> Result<()> {
        use anchor_spl::token::{self, Token};

        let cpi_accounts = token::Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.key(), cpi_accounts);
        let _ = token::transfer(cpi_ctx, amount); // [cpi_no_result]
        Ok(())
    }

    // Case 14: anchor_spl::token::transfer with ? operator - should NOT trigger lint
    pub fn token_transfer_with_question_mark(
        ctx: Context<TokenTransfer>,
        amount: u64,
    ) -> Result<()> {
        use anchor_spl::token::{self, Token};

        let cpi_accounts = token::Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.key(), cpi_accounts);
        token::transfer(cpi_ctx, amount)?; // [safe_cpi_call]
        Ok(())
    }

    // Case 15: Multiple CPI calls, one without result handling - should trigger lint
    pub fn multiple_cpis_one_without_result(
        ctx: Context<BasicTransfer>,
        amount: u64,
    ) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]

        // Second CPI without result handling
        let cpi_ctx2 = build_transfer_context(&ctx);
        let _ = system_program::transfer(cpi_ctx2, amount); // [cpi_no_result]
        Ok(())
    }

    // Case 16: CPI call in if statement without result handling - should trigger lint
    pub fn cpi_in_if_without_result(
        ctx: Context<BasicTransfer>,
        amount: u64,
        should_transfer: bool,
    ) -> Result<()> {
        if should_transfer {
            let cpi_ctx = build_transfer_context(&ctx);
            let _ = system_program::transfer(cpi_ctx, amount); // [cpi_no_result]
        }
        Ok(())
    }

    // Case 17: CPI call in if statement with result handling - should NOT trigger lint
    pub fn cpi_in_if_with_result(
        ctx: Context<BasicTransfer>,
        amount: u64,
        should_transfer: bool,
    ) -> Result<()> {
        if should_transfer {
            let cpi_ctx = build_transfer_context(&ctx);
            system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]
        }
        Ok(())
    }

    // Case 18: CPI call in match without result handling - should trigger lint
    pub fn cpi_in_match_without_result(
        ctx: Context<BasicTransfer>,
        amount: u64,
        mode: u8,
    ) -> Result<()> {
        match mode {
            0 => {
                let cpi_ctx = build_transfer_context(&ctx);
                let _ = system_program::transfer(cpi_ctx, amount); // [cpi_no_result]
                Ok(())
            }
            _ => Ok(()),
        }
    }

    // Case 19: CPI call in match with result handling - should NOT trigger lint
    pub fn cpi_in_match_with_result(
        ctx: Context<BasicTransfer>,
        amount: u64,
        mode: u8,
    ) -> Result<()> {
        match mode {
            0 => {
                let cpi_ctx = build_transfer_context(&ctx);
                system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]
                Ok(())
            }
            _ => Ok(()),
        }
    }

    // Case 20: CPI call with let Ok pattern - should NOT trigger lint
    pub fn cpi_with_let_ok(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        if let Ok(_) = system_program::transfer(cpi_ctx, amount) { // [safe_cpi_call]
            // Success
        }
        Ok(())
    }

    // Case 21: CPI call with let Err pattern - should NOT trigger lint
    pub fn cpi_with_let_err(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        if let Err(e) = system_program::transfer(cpi_ctx, amount) {
            // [safe_cpi_call]
            return Err(e);
        }
        Ok(())
    }

    // Case 22: Nested CPI call without result handling - should trigger lint
    pub fn nested_cpi_without_result(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        helper_cpi_without_result(ctx, amount)
    }

    // Case 23: CPI call with map_err and ? operator - should NOT trigger lint
    pub fn cpi_with_map_err_then_question_mark(
        ctx: Context<BasicTransfer>,
        amount: u64,
    ) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount)
            .map_err(|_| error!(CustomError::TransferFailed))?; // [safe_cpi_call]
        Ok(())
    }

    // Case 24: CPI call with map_err and drop ? operator - should trigger lint
    pub fn cpi_with_map_err_then_ignored(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        let _ = system_program::transfer(cpi_ctx, amount) // [cpi_no_result]
            .map_err(|_| error!(CustomError::TransferFailed));
        Ok(())
    }

    // Case 25: CPI call with map and ? operator - should NOT trigger lint
    pub fn cpi_with_map_then_question_mark(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount).map(|_| ())?; // [safe_cpi_call]
        Ok(())
    }

    // Case 26: CPI call with map but no ? operator - should trigger lint
    pub fn cpi_with_map_then_ignored(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        let _ = system_program::transfer(cpi_ctx, amount).map(|_| ()); // [cpi_no_result]
        Ok(())
    }

    // Case 27: CPI call with and_then and ? operator - should NOT trigger lint
    pub fn cpi_with_and_then_question_mark(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount).and_then(|_| Ok(()))?; // [safe_cpi_call]
        Ok(())
    }

    // Case 28: CPI call with and_then but no ? operator - should trigger lint
    pub fn cpi_with_and_then_ignored(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        let _ = system_program::transfer(cpi_ctx, amount).and_then(|_| Ok(())); // [cpi_no_result]
        Ok(())
    }

    // Case 29: CPI call with or_else and ? operator - should NOT trigger lint
    pub fn cpi_with_or_else_question_mark(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount).or_else(|e| Err(e))?; // [safe_cpi_call]
        Ok(())
    }

    // Case 30: CPI call with or_else but no ? operator - should trigger lint
    pub fn cpi_with_or_else_ignored(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        let _ = system_program::transfer(cpi_ctx, amount).or_else(|e| Err(e)); // [cpi_no_result]
        Ok(())
    }

    // Case 31: CPI call early return without result handling - should not trigger lint
    pub fn cpi_early_return_with_result(
        ctx: Context<BasicTransfer>,
        amount: u64,
        should_transfer: bool,
    ) -> Result<()> {
        if should_transfer {
            let cpi_ctx = build_transfer_context(&ctx);
            let result = system_program::transfer(cpi_ctx, amount); // [safe_cpi_call]
            if result.is_err() {
                return result;
            }
        }
        Ok(())
    }

    // Case 32: CPI call early return with Ok pattern - should not trigger lint
    pub fn cpi_early_return_with_ok(
        ctx: Context<BasicTransfer>,
        amount: u64,
        should_transfer: bool,
    ) -> Result<()> {
        if should_transfer {
            let cpi_ctx = build_transfer_context(&ctx);
            let Ok(_) = system_program::transfer(cpi_ctx, amount) else { // [safe_cpi_call]
                return err!(CustomError::TransferFailed);
            };
        }
        Ok(())
    }

    // Case 33: CPI call early return with Ok pattern - should not trigger lint
    pub fn cpi_early_return_with_ok_and_err(
        ctx: Context<BasicTransfer>,
        amount: u64,
        should_transfer: bool,
    ) -> Result<()> {
        if should_transfer {
            let cpi_ctx = build_transfer_context(&ctx);
            if let Some(_) = system_program::transfer(cpi_ctx, amount).ok() { // [safe_cpi_call]
                return Ok(());
            } else {
                return err!(CustomError::TransferFailed);
            }
        }
        Ok(())
    }

    // Case 34: CPI call early return with unwrap_or_default - should trigger lint
    pub fn cpi_early_return_with_unwrap_or_default(
        ctx: Context<BasicTransfer>,
        amount: u64,
        should_transfer: bool,
    ) -> Result<()> {
        if should_transfer {
            let cpi_ctx = build_transfer_context(&ctx);
            system_program::transfer(cpi_ctx, amount).unwrap_or_default(); // [cpi_no_result]
        }
        Ok(())
    }

    // Case 35: CPI call with implicit return - should NOT trigger lint (false positive fix)
    pub fn cpi_with_implicit_return(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        if amount == 0 {
            return Ok(());
        }
        let cpi_ctx = build_transfer_context(&ctx);
        system_program::transfer(cpi_ctx, amount) // [safe_cpi_call]
    }

    // Case 36: CPI call with builder method chain (.with_signer) - should NOT trigger lint (false positive fix)
    pub fn cpi_with_builder_method_chain(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&ctx);
        let signer_seeds: &[&[&[u8]]] = &[&[b"test"]];

        // This pattern matches MarginFi: builder method chain then CPI call with ?
        system_program::transfer(cpi_ctx.with_signer(signer_seeds), amount)?; // [safe_cpi_call] - .with_signer() is not a CPI call, actual CPI handles result
        Ok(())
    }
}

// Helper function for nested CPI test
fn helper_cpi_without_result(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
    let cpi_ctx = build_transfer_context(&ctx);
    let _ = system_program::transfer(cpi_ctx, amount); // [cpi_no_result]
    Ok(())
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
