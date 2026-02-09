use anchor_lang::prelude::*;
use anchor_spl::token::{self, Transfer};

declare_id!("11111111111111111111111111111111");

const WITHDRAW_FEE: u64 = 1000;

#[program]
pub mod direct_lamport_cpi_dos {
    use super::*;

    // Pattern 1: Bad - lamport mutation without including account in CPI
    pub fn withdraw_with_fee_bad(ctx: Context<WithdrawWithFee>, amount: u64) -> Result<()> {
        // Direct lamport mutations
        **ctx.accounts.vault.lamports.borrow_mut() -= WITHDRAW_FEE;
        **ctx.accounts.fee_collector.lamports.borrow_mut() += WITHDRAW_FEE;

        // CPI without including fee_collector
        token::transfer( // [direct_lamport_cpi_dos]
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.vault_token.to_account_info(),
                    to: ctx.accounts.user_token.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
            ),
            amount,
        )?;

        Ok(())
    }

    // Pattern 2: Good - lamport mutation with account included in remaining_accounts
    pub fn withdraw_with_fee_good(ctx: Context<WithdrawWithFee>, amount: u64) -> Result<()> {
        // Direct lamport mutations
        **ctx.accounts.vault.lamports.borrow_mut() -= WITHDRAW_FEE;
        **ctx.accounts.fee_collector.lamports.borrow_mut() += WITHDRAW_FEE;

        let remaining_accounts = vec![ctx.accounts.fee_collector.to_account_info()];

        // CPI with fee_collector in remaining_accounts
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.vault_token.to_account_info(),
                    to: ctx.accounts.user_token.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
            )
            .with_remaining_accounts(remaining_accounts),
            amount,
        )?; // [safe_lamport_cpi]

        Ok(())
    }

    // Pattern 3: Bad - multiple lamport mutations, one missing from CPI
    pub fn multiple_lamport_mutations_bad(
        ctx: Context<MultipleAccounts>,
        amount: u64,
    ) -> Result<()> {
        **ctx.accounts.vault.lamports.borrow_mut() -= WITHDRAW_FEE;
        **ctx.accounts.fee_collector.lamports.borrow_mut() += WITHDRAW_FEE;
        **ctx.accounts.treasury.lamports.borrow_mut() += WITHDRAW_FEE / 2;

        token::transfer( // [direct_lamport_cpi_dos]
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.vault_token.to_account_info(),
                    to: ctx.accounts.user_token.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
            )
            .with_remaining_accounts(vec![
                ctx.accounts.fee_collector.to_account_info(),
                // treasury is missing!
            ]),
            amount,
        )?;

        Ok(())
    }

    // Pattern 4: Good - all lamport-mutated accounts included
    pub fn multiple_lamport_mutations_good(
        ctx: Context<MultipleAccounts>,
        amount: u64,
    ) -> Result<()> {
        **ctx.accounts.vault.lamports.borrow_mut() -= WITHDRAW_FEE;
        **ctx.accounts.fee_collector.lamports.borrow_mut() += WITHDRAW_FEE;
        **ctx.accounts.treasury.lamports.borrow_mut() += WITHDRAW_FEE / 2;

        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.vault_token.to_account_info(),
                    to: ctx.accounts.user_token.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
            )
            .with_remaining_accounts(vec![
                ctx.accounts.fee_collector.to_account_info(),
                ctx.accounts.treasury.to_account_info(),
            ]),
            amount,
        )?; // [safe_lamport_cpi]

        Ok(())
    }

    // Pattern 5: Safe - lamport mutation but no CPI
    pub fn lamport_mutation_no_cpi(ctx: Context<WithdrawWithFee>) -> Result<()> {
        **ctx.accounts.vault.lamports.borrow_mut() -= WITHDRAW_FEE;
        **ctx.accounts.fee_collector.lamports.borrow_mut() += WITHDRAW_FEE;
        // No CPI call, so no issue
        Ok(()) // [safe_lamport_cpi]
    }

    // Pattern 6: Safe - CPI but no lamport mutations
    pub fn cpi_no_lamport_mutation(ctx: Context<WithdrawWithFee>, amount: u64) -> Result<()> {
        // No lamport mutations
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.vault_token.to_account_info(),
                    to: ctx.accounts.user_token.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
            ),
            amount,
        )?; // [safe_lamport_cpi]

        Ok(())
    }

    // Pattern 7: Bad - lamport mutation on account that's already in CPI but as different role
    pub fn lamport_mutation_account_in_cpi_bad(
        ctx: Context<WithdrawWithFee>,
        amount: u64,
    ) -> Result<()> {
        **ctx.accounts.vault.lamports.borrow_mut() -= WITHDRAW_FEE;
        **ctx.accounts.fee_collector.lamports.borrow_mut() += WITHDRAW_FEE;

        // vault is in CPI as authority, but fee_collector is not included
        token::transfer( // [direct_lamport_cpi_dos]
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.vault_token.to_account_info(),
                    to: ctx.accounts.user_token.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(), // vault is here
                },
            ),
            amount,
        )?;

        Ok(())
    }

    // Pattern 8: Good - lamport mutation on account that's already in CPI, but other mutated account also included
    pub fn lamport_mutation_account_in_cpi_good(
        ctx: Context<WithdrawWithFee>,
        amount: u64,
    ) -> Result<()> {
        **ctx.accounts.vault.lamports.borrow_mut() -= WITHDRAW_FEE;
        **ctx.accounts.fee_collector.lamports.borrow_mut() += WITHDRAW_FEE;

        // vault is in CPI as authority, fee_collector is in remaining_accounts
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.vault_token.to_account_info(),
                    to: ctx.accounts.user_token.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(), // vault is here
                },
            )
            .with_remaining_accounts(vec![ctx.accounts.fee_collector.to_account_info()]),
            amount,
        )?; // [safe_lamport_cpi]

        Ok(())
    }

    // Pattern 9: Bad - multiple CPI calls, lamport-mutated account missing from one CPI
    pub fn multiple_cpi_calls_bad(ctx: Context<MultipleAccounts>, amount: u64) -> Result<()> {
        **ctx.accounts.vault.lamports.borrow_mut() -= WITHDRAW_FEE;
        **ctx.accounts.fee_collector.lamports.borrow_mut() += WITHDRAW_FEE;
        **ctx.accounts.treasury.lamports.borrow_mut() += WITHDRAW_FEE / 2;

        // First CPI - includes fee_collector but not treasury
        token::transfer( // [direct_lamport_cpi_dos]
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.vault_token.to_account_info(),
                    to: ctx.accounts.user_token.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
            )
            .with_remaining_accounts(vec![
                ctx.accounts.fee_collector.to_account_info(),
                // treasury is missing!
            ]),
            amount,
        )?;

        // Second CPI - doesn't include any of the lamport-mutated accounts
        token::transfer( // [direct_lamport_cpi_dos]
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.vault_token.to_account_info(),
                    to: ctx.accounts.user_token.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
            ),
            amount,
        )?;

        Ok(())
    }

    // Pattern 10: Good - multiple CPI calls, all lamport-mutated accounts included in each CPI
    pub fn multiple_cpi_calls_good(ctx: Context<MultipleAccounts>, amount: u64) -> Result<()> {
        **ctx.accounts.vault.lamports.borrow_mut() -= WITHDRAW_FEE;
        **ctx.accounts.fee_collector.lamports.borrow_mut() += WITHDRAW_FEE;
        **ctx.accounts.treasury.lamports.borrow_mut() += WITHDRAW_FEE / 2;

        // First CPI - includes all lamport-mutated accounts
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.vault_token.to_account_info(),
                    to: ctx.accounts.user_token.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
            )
            .with_remaining_accounts(vec![
                ctx.accounts.fee_collector.to_account_info(),
                ctx.accounts.treasury.to_account_info(),
            ]),
            amount,
        )?; // [safe_lamport_cpi]

        // Second CPI - also includes all lamport-mutated accounts
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.vault_token.to_account_info(),
                    to: ctx.accounts.user_token.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
            )
            .with_remaining_accounts(vec![
                ctx.accounts.fee_collector.to_account_info(),
                ctx.accounts.treasury.to_account_info(),
            ]),
            amount,
        )?; // [safe_lamport_cpi]

        Ok(())
    }
}

#[derive(Accounts)]
pub struct WithdrawWithFee<'info> {
    #[account(mut)]
    pub vault: Signer<'info>,
    #[account(mut)]
    pub fee_collector: AccountInfo<'info>,
    #[account(mut)]
    pub vault_token: AccountInfo<'info>,
    #[account(mut)]
    pub user_token: AccountInfo<'info>,
    pub token_program: Program<'info, anchor_spl::token::Token>,
}

#[derive(Accounts)]
pub struct MultipleAccounts<'info> {
    #[account(mut)]
    pub vault: Signer<'info>,
    #[account(mut)]
    pub fee_collector: AccountInfo<'info>,
    #[account(mut)]
    pub treasury: AccountInfo<'info>,
    #[account(mut)]
    pub vault_token: AccountInfo<'info>,
    #[account(mut)]
    pub user_token: AccountInfo<'info>,
    pub token_program: Program<'info, anchor_spl::token::Token>,
}
