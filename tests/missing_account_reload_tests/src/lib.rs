use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

declare_id!("11111111111111111111111111111111");

#[program]
pub mod missing_account_reload_tests {
    use super::*;

    // Pattern 1: Basic CPI + account access without reload (UNSAFE)
    pub fn sol_transfer(ctx: Context<SolTransfer>, amount: u64) -> Result<()> {
        let program_id = ctx.accounts.system_program.key();

        let cpi_context = CpiContext::new(
            program_id,
            Transfer {
                from: ctx.accounts.sender.to_account_info(),
                to: ctx.accounts.recipient.to_account_info(),
            },
        );

        transfer(cpi_context, amount)?; // [cpi_call]
        let _data = ctx.accounts.sender.data; // [unsafe_account_accessed]
        let _sender_data = ctx.accounts.sender_state.data; // [safe_account_accessed]
        Ok(())
    }

    // Pattern 2: CPI with signer seeds + account access (SAFE) - account not involved in CPI
    pub fn sol_transfer_with_seeds(ctx: Context<SolTransfer1>, amount: u64) -> Result<()> {
        let from_pubkey = ctx.accounts.pda_account.to_account_info();
        let to_pubkey = ctx.accounts.recipient.to_account_info();
        let program_id = ctx.accounts.system_program.key();

        let seed = to_pubkey.key();
        let bump_seed = ctx.bumps.pda_account;
        let signer_seeds: &[&[&[u8]]] = &[&[b"pda", seed.as_ref(), &[bump_seed]]];

        let cpi_context = CpiContext::new(
            program_id,
            Transfer {
                from: from_pubkey,
                to: to_pubkey,
            },
        )
        .with_signer(signer_seeds);

        transfer(cpi_context, amount)?;
        ctx.accounts.pda_account.reload()?;
        let _data = &ctx.accounts.inner.data; // [safe_account_accessed]
        Ok(())
    }

    // Pattern 3: CPI + account access through mutable reference (UNSAFE)
    pub fn invoke_with_mutable_ref(ctx: Context<SolTransfer1>, amount: u64) -> Result<()> {
        let from_pubkey = ctx.accounts.pda_account.to_account_info();
        let to_pubkey = ctx.accounts.recipient.to_account_info();
        let program_id = ctx.accounts.system_program.key();
        let seed = to_pubkey.key();

        let cpi_accounts = Transfer {
            from: from_pubkey,
            to: to_pubkey,
        };

        let bump_seed = ctx.bumps.pda_account;
        let signer_seeds: &[&[&[u8]]] = &[&[b"pda", seed.as_ref(), &[bump_seed]]];

        let cpi_context = CpiContext::new(program_id, cpi_accounts).with_signer(signer_seeds);

        transfer(cpi_context, amount)?; // [cpi_call]
        let user_acc = &mut ctx.accounts.pda_account;
        let _ = user_acc.data; // [unsafe_account_accessed]
        Ok(())
    }

    // Pattern 4: CPI + reload + account access (SAFE - should NOT trigger)
    pub fn invoke_with_reload(ctx: Context<SolTransfer2>, amount: u64) -> Result<()> {
        let from_pubkey = ctx.accounts.pda_account.to_account_info();
        let to_pubkey = ctx.accounts.recipient.to_account_info();
        let program_id = ctx.accounts.system_program.key();
        let seed = to_pubkey.key();

        let cpi_accounts = Transfer {
            from: from_pubkey,
            to: to_pubkey,
        };

        let bump_seed = ctx.bumps.pda_account;
        let signer_seeds: &[&[&[u8]]] = &[&[b"pda", seed.as_ref(), &[bump_seed]]];
        let cpi_context = CpiContext::new(program_id, cpi_accounts).with_signer(signer_seeds);

        transfer(cpi_context, amount)?;

        // Reload before access - this is SAFE
        ctx.accounts.pda_account.reload()?;
        let _updated_balance = ctx.accounts.pda_account.data; // [safe_account_accessed]
        Ok(())
    }

    // Pattern 5: CPI + nested function access (UNSAFE)
    pub fn invoke_with_nested_access(mut ctx: Context<SolTransfer2>, amount: u64) -> Result<()> {
        let from_pubkey = ctx.accounts.pda_account.to_account_info();
        let to_pubkey = ctx.accounts.recipient.to_account_info();
        let program_id = ctx.accounts.system_program.key();
        let seed = to_pubkey.key();

        let cpi_accounts = Transfer {
            from: from_pubkey,
            to: to_pubkey,
        };

        let bump_seed = ctx.bumps.pda_account;
        let signer_seeds: &[&[&[u8]]] = &[&[b"pda", seed.as_ref(), &[bump_seed]]];
        let cpi_context = CpiContext::new(program_id, cpi_accounts).with_signer(signer_seeds);

        transfer(cpi_context, amount)?; // [cpi_call]

        // Access through nested function - should trigger lint
        access_account_data_from_ctx(&mut ctx)?;
        Ok(())
    }

    // Pattern 6: CPI + nested function with reload (UNSAFE)
    pub fn invoke_with_nested_reload(mut ctx: Context<SolTransfer2>, amount: u64) -> Result<()> {
        let from_pubkey = ctx.accounts.pda_account.to_account_info();
        let to_pubkey = ctx.accounts.recipient.to_account_info();
        let program_id = ctx.accounts.system_program.key();
        let seed = to_pubkey.key();

        let cpi_accounts = Transfer {
            from: from_pubkey,
            to: to_pubkey,
        };

        let bump_seed = ctx.bumps.pda_account;
        let signer_seeds: &[&[&[u8]]] = &[&[b"pda", seed.as_ref(), &[bump_seed]]];
        let cpi_context = CpiContext::new(program_id, cpi_accounts).with_signer(signer_seeds);

        transfer(cpi_context, amount)?; // [cpi_call]

        // Reload through nested function - should be UNSAFE
        reload_access_account_from_ctx(&mut ctx)?;
        Ok(())
    }

    // Pattern 7: Multiple account access after CPI (UNSAFE)
    pub fn invoke_with_multi_access(ctx: Context<SolTransfer2>, amount: u64) -> Result<()> {
        let from_pubkey = ctx.accounts.pda_account.to_account_info();
        let to_pubkey = ctx.accounts.recipient.to_account_info();
        let program_id = ctx.accounts.system_program.key();
        let seed = to_pubkey.key();

        let cpi_accounts = Transfer {
            from: from_pubkey,
            to: to_pubkey,
        };

        let bump_seed = ctx.bumps.pda_account;
        let signer_seeds: &[&[&[u8]]] = &[&[b"pda", seed.as_ref(), &[bump_seed]]];
        let cpi_context = CpiContext::new(program_id, cpi_accounts).with_signer(signer_seeds);

        transfer(cpi_context, amount)?; // [cpi_call]

        // Multiple account accesses without reload
        let _data1 = ctx.accounts.pda_account.data; // [unsafe_account_accessed]
        let _data2 = ctx.accounts.pda_account_1.data; // [safe_account_accessed]
        Ok(())
    }

    // Pattern 8: Multiple account access + reload nested functions
    pub fn invoke_with_data_access_and_reload(
        mut ctx: Context<SolTransfer2>,
        amount: u64,
    ) -> Result<()> {
        transfer( // [cpi_call]
            CpiContext::new(
                ctx.accounts.system_program.key(),
                Transfer {
                    from: ctx.accounts.pda_account.to_account_info(),
                    to: ctx.accounts.recipient.to_account_info(),
                },
            ),
            amount,
        )?;

        access_account_data_from_ctx(&mut ctx)?;
        reload_access_account_from_ctx(&mut ctx)?;
        let _updated_balance = ctx.accounts.pda_account.data; // [safe_account_accessed]
        Ok(())
    }

    // Pattern 9: CPI call in a nested function with accounts as arguments (UNSAFE)
    pub fn invoke_cpi_in_nested_function(
        mut ctx: Context<SolTransfer2>,
        amount: u64,
    ) -> Result<()> {
        cpi_call_ctx(&mut ctx, amount)?;
        let _data = ctx.accounts.pda_account.data; // [unsafe_account_accessed]
        Ok(())
    }

    // Pattern 10: CPI call with individual account as arguments (UNSAFE)
    pub fn invoke_cpi_with_individual_account(
        ctx: Context<SolTransfer2>,
        amount: u64,
    ) -> Result<()> {
        cpi_call_ctx_with_individual_account(
            &mut ctx.accounts.pda_account,
            &mut ctx.accounts.recipient,
            amount,
        )?;
        let _data = ctx.accounts.pda_account.data; // [unsafe_account_accessed]
        Ok(())
    }

    // Pattern 11: Mutliple CPI calls & reloads in a nested function (UNSAFE)
    pub fn invoke_multiple_cpi_calls_and_reloads(
        mut ctx: Context<SolTransfer2>,
        amount: u64,
    ) -> Result<()> {
        multiple_cpi_calls_and_reloads(&mut ctx, amount)?;
        let _data = ctx.accounts.pda_account.data; // [safe_account_accessed]
        Ok(())
    }

    // Pattern 12: CPI context created in helper, CPI invoked later (should lint)
    pub fn invoke_with_split_context(mut ctx: Context<SolTransfer2>, amount: u64) -> Result<()> {
        let cpi_ctx = build_transfer_context(&mut ctx.accounts)?;
        transfer(cpi_ctx, amount)?; // [cpi_call]
                                    // Access without reload after CPI (should be flagged)
        let _data = ctx.accounts.pda_account.data; // [unsafe_account_accessed]
        Ok(())
    }

    // Pattern 13: CPI context + CPI call + Reload + data access in nested function (should lint)
    pub fn invoke_with_nested_function(mut ctx: Context<SolTransfer2>, amount: u64) -> Result<()> {
        nested_cpi_context_creation_and_call(&mut ctx, amount)?;
        Ok(())
    }

    // Pattern 14: Layered helpers; ensures CPI contexts built/used across functions
    pub fn invoke_with_complex_helpers(
        mut ctx: Context<SolTransfer2>,
        amount: u64,
    ) -> Result<()> {
        transfer_helper(&mut ctx, amount)?;
        ctx.accounts.pda_account.reload()?;
        check_balance(&mut ctx, amount)?;
        Ok(())
    }
}

// Helper functions for nested access patterns
fn access_account_data_from_ctx(ctx_a: &mut Context<SolTransfer2>) -> Result<()> {
    access_account_data_from_accounts(&mut ctx_a.accounts)?;
    Ok(())
}

fn access_account_data_from_accounts(accounts: &mut SolTransfer2) -> Result<()> {
    access_account_data_from_account(&mut accounts.pda_account)?;
    Ok(())
}

fn access_account_data_from_account(account: &mut Account<'_, UserState>) -> Result<()> {
    let _updated_balance = account.data; // [unsafe_account_accessed]
    Ok(())
}

fn reload_access_account_from_ctx(ctx_a: &mut Context<SolTransfer2>) -> Result<()> {
    reload_access_account_from_accounts(&mut ctx_a.accounts)?;
    Ok(())
}

// Unsafe account access before reload
fn reload_access_account_from_accounts(accounts: &mut SolTransfer2) -> Result<()> {
    let _updated_balance = accounts.pda_account.data; // [unsafe_account_accessed]
    accounts.pda_account.reload()?;
    Ok(())
}

fn cpi_call_ctx(ctx_a: &mut Context<SolTransfer2>, amount: u64) -> Result<()> {
    let program_id = ctx_a.accounts.system_program.key();
    let seed = ctx_a.accounts.recipient.key();

    let cpi_accounts = Transfer {
        from: ctx_a.accounts.pda_account.to_account_info(),
        to: ctx_a.accounts.recipient.to_account_info(),
    };

    let bump_seed = ctx_a.bumps.pda_account;
    let signer_seeds: &[&[&[u8]]] = &[&[b"pda", seed.as_ref(), &[bump_seed]]];
    let cpi_context = CpiContext::new(program_id, cpi_accounts).with_signer(signer_seeds);

    transfer(cpi_context, amount)?; // [cpi_call]
    Ok(())
}

fn cpi_call_ctx_with_individual_account<'a>(
    pda_account: &mut Account<'a, UserState>,
    recipient: &mut SystemAccount<'a>,
    amount: u64,
) -> Result<()> {
    let from_pubkey = pda_account.to_account_info();
    let to_pubkey = recipient.to_account_info();
    let cpi_accounts = Transfer {
        from: from_pubkey,
        to: to_pubkey,
    };
    let cpi_context = CpiContext::new(system_program::ID, cpi_accounts);
    transfer(cpi_context, amount)?; // [cpi_call]
    Ok(())
}

fn multiple_cpi_calls_and_reloads(ctx: &mut Context<SolTransfer2>, amount: u64) -> Result<()> {
    cpi_call_ctx(ctx, amount)?;
    reload_access_account_from_ctx(ctx)?;
    cpi_call_ctx(ctx, amount)?;
    reload_access_account_from_ctx(ctx)?;
    Ok(())
}

fn build_transfer_context<'info>(
    accounts: &mut SolTransfer2<'info>,
) -> Result<CpiContext<'info, 'info, 'info, 'info, Transfer<'info>>> {
    let program_id = accounts.system_program.key();
    let cpi_accounts = Transfer {
        from: accounts.pda_account.to_account_info(),
        to: accounts.recipient.to_account_info(),
    };
    Ok(CpiContext::new(program_id, cpi_accounts))
}

fn nested_cpi_context_creation_and_call(
    ctx: &mut Context<SolTransfer2>,
    amount: u64,
) -> Result<()> {
    let cpi_ctx = build_transfer_context(&mut ctx.accounts)?;
    transfer(cpi_ctx, amount)?; // [cpi_call]
    reload_access_account_from_ctx(ctx)?;
    let _data = ctx.accounts.pda_account.data; // [safe_account_accessed]
    Ok(())
}

fn check_balance(ctx: &mut Context<SolTransfer2>, amount: u64) -> Result<()> {
    let balance = ctx.accounts.pda_account.data; // [safe_account_accessed]
    if balance < amount {
        return err!(CustomError::InsufficientBalance);
    }
    Ok(())
}

fn transfer_funds(ctx: &mut Context<SolTransfer2>, amount: u64) -> Result<()> {
    let cpi_ctx = build_transfer_context(&mut ctx.accounts)?;
    transfer(cpi_ctx, amount)?; // [cpi_call]
    Ok(())
}

fn transfer_helper(ctx: &mut Context<SolTransfer2>, amount: u64) -> Result<()> {
    check_balance(ctx, amount)?;
    transfer_funds(ctx, amount)?;
    access_account_data_from_ctx(ctx)?;
    let _ = ctx.accounts.inner.data; // [safe_account_accessed]
    Ok(())
}

// Account structs
#[derive(Accounts)]
pub struct SolTransfer<'info> {
    #[account(mut)]
    sender: Account<'info, UserState>,
    #[account(mut)]
    recipient: SystemAccount<'info>,
    #[account(mut)]
    pub sender_state: Account<'info, UserState>,
    system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SolTransfer1<'info> {
    #[account(
        mut,
        seeds = [b"pda", recipient.key().as_ref()],
        bump,
    )]
    pub pda_account: Account<'info, UserState>,
    #[account(mut)]
    recipient: SystemAccount<'info>,
    #[account(mut)]
    pub inner: Account<'info, InnerAccount>,
    system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SolTransfer2<'info> {
    #[account(
        mut,
        seeds = [b"pda", recipient.key().as_ref()],
        bump,
    )]
    pub pda_account: Account<'info, UserState>,
    pub pda_account_1: Account<'info, UserState>,
    #[account(mut)]
    pub recipient: SystemAccount<'info>,
    #[account(mut)]
    pub inner: Account<'info, InnerAccount>,
    pub system_program: Program<'info, System>,
}

#[account]
pub struct UserState {
    pub data: u64,
}

#[account]
pub struct InnerAccount {
    pub data: u64,
}


#[error_code]
pub enum CustomError {
    #[msg("balance is less than amount")]
    InsufficientBalance,
}