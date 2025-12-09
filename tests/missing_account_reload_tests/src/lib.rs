use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    program::{invoke, invoke_signed},
    system_instruction,
};

use anchor_lang::system_program::{transfer, Transfer};

declare_id!("11111111111111111111111111111111");

#[program]
pub mod missing_account_reload_tests {
    use super::*;

    // Pattern 1: Basic CPI + account access without reload (UNSAFE)
    pub fn sol_transfer(ctx: Context<SolTransfer>, amount: u64) -> Result<()> {
        // Use a CPI that mutates account data (allocate) so the lint should fire
        cpi_mutating_allocate(
            &ctx.accounts.sender,
            &ctx.accounts.system_program,
            amount,
            &[],
        )?;
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

        cpi_mutating_allocate(
            &ctx.accounts.pda_account,
            &ctx.accounts.system_program,
            amount,
            signer_seeds,
        )?;
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

        cpi_mutating_allocate(
            &ctx.accounts.pda_account,
            &ctx.accounts.system_program,
            amount,
            signer_seeds,
        )?;
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

        cpi_mutating_allocate(
            &ctx.accounts.pda_account,
            &ctx.accounts.system_program,
            amount,
            signer_seeds,
        )?;

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

        cpi_mutating_allocate(
            &ctx.accounts.pda_account,
            &ctx.accounts.system_program,
            amount,
            signer_seeds,
        )?;

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

        cpi_mutating_allocate(
            &ctx.accounts.pda_account,
            &ctx.accounts.system_program,
            amount,
            signer_seeds,
        )?;

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

        cpi_mutating_allocate(
            &ctx.accounts.pda_account,
            &ctx.accounts.system_program,
            amount,
            signer_seeds,
        )?;

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
        cpi_mutating_allocate(
            &ctx.accounts.pda_account,
            &ctx.accounts.system_program,
            amount,
            &[],
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
            &ctx.accounts.system_program,
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
        cpi_mutating_allocate(
            &ctx.accounts.pda_account,
            &ctx.accounts.system_program,
            amount,
            &[],
        )?;
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
    pub fn invoke_with_complex_helpers(mut ctx: Context<SolTransfer2>, amount: u64) -> Result<()> {
        transfer_helper(&mut ctx, amount)?;
        ctx.accounts.pda_account.reload()?;
        check_balance(&mut ctx, amount)?;
        Ok(())
    }

    // Pattern 15: CPI call using invoke_signed directly in main function (should lint)
    pub fn invoke_with_direct_invoke_signed(ctx: Context<SolTransfer2>, amount: u64) -> Result<()> {
        // Access account before CPI
        let _initial_data = ctx.accounts.pda_account.data; // [safe_account_accessed]

        let seed = ctx.accounts.recipient.key();
        let bump_seed = ctx.bumps.pda_account;
        let signer_seeds: &[&[&[u8]]] = &[&[b"pda", seed.as_ref(), &[bump_seed]]];
        cpi_mutating_allocate(
            &ctx.accounts.pda_account,
            &ctx.accounts.system_program,
            amount,
            signer_seeds,
        )?;

        // Access account after CPI without reload - should trigger lint
        let _final_data = ctx.accounts.pda_account.data; // [unsafe_account_accessed]

        Ok(())
    }

    // Pattern 16: CPI call in a helper function (should lint)
    pub fn invoke_with_helper_cpi(ctx: Context<SolTransfer2>, amount: u64) -> Result<()> {
        // Access account before CPI
        let _initial_data = ctx.accounts.pda_account.data; // [safe_account_accessed]

        // Call helper function that makes CPI (similar to transfer_from_pool)
        transfer_from_pool_helper(
            &mut ctx.accounts.pda_account,
            &mut ctx.accounts.recipient,
            &ctx.accounts.system_program,
            amount,
        )?;

        // Access account after CPI without reload - should trigger lint
        let _final_data = ctx.accounts.pda_account.data; // [unsafe_account_accessed]

        Ok(())
    }

    // Pattern 17: CPI call with self implementation - safe
    pub fn invoke_with_self_implementation_safe(ctx: Context<SolTransfer3>, amount: u64) -> Result<()> {
        ctx.accounts.cpi_call_safe(amount)?;
        let _final_data = ctx.accounts.pda_account.data; // [safe_account_accessed]
        Ok(())
    }

    // Pattern 18: CPI call with self implementation - unsafe
    pub fn invoke_with_self_implementation_unsafe(ctx: Context<SolTransfer3>, amount: u64) -> Result<()> {
        ctx.accounts.cpi_call_unsafe(amount)?;
        let _final_data = ctx.accounts.pda_account.data; // [unsafe_account_accessed]
        Ok(())
    }
}
pub fn cpi_call_safe(ctx_a: &mut Context<SolTransfer3>, amount: u64) -> Result<()> {
    let from_pubkey = ctx_a.accounts.pda_account.to_account_info();
    let to_pubkey = ctx_a.accounts.recipient.to_account_info();
    let program_id = ctx_a.accounts.system_program.key();
    let seed = to_pubkey.key();

    let cpi_accounts = Transfer {
        from: from_pubkey,
        to: to_pubkey,
    };

    let bump_seed = 0;
    let signer_seeds: &[&[&[u8]]] = &[&[b"pda", seed.as_ref(), &[bump_seed]]];
    let cpi_context = CpiContext::new(program_id, cpi_accounts).with_signer(signer_seeds);

    transfer(cpi_context, amount)?;
    Ok(())
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
    let seed = ctx_a.accounts.recipient.key();

    let cpi_accounts = Transfer {
        from: ctx_a.accounts.pda_account.to_account_info(),
        to: ctx_a.accounts.recipient.to_account_info(),
    };

    let bump_seed = ctx_a.bumps.pda_account;
    let signer_seeds: &[&[&[u8]]] = &[&[b"pda", seed.as_ref(), &[bump_seed]]];
    cpi_mutating_allocate(
        &ctx_a.accounts.pda_account,
        &ctx_a.accounts.system_program,
        amount,
        signer_seeds,
    )?;
    Ok(())
}

fn cpi_call_ctx_with_individual_account<'a>(
    pda_account: &mut Account<'a, UserState>,
    recipient: &mut SystemAccount<'a>,
    system_program: &Program<'a, System>,
    amount: u64,
) -> Result<()> {
    cpi_mutating_allocate(pda_account, system_program, amount, &[])?;
    Ok(())
}

fn multiple_cpi_calls_and_reloads(ctx: &mut Context<SolTransfer2>, amount: u64) -> Result<()> {
    cpi_call_ctx(ctx, amount)?;
    reload_access_account_from_ctx(ctx)?;
    cpi_call_ctx(ctx, amount)?;
    reload_access_account_from_ctx(ctx)?;
    Ok(())
}

#[allow(dead_code)]
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
    let _ = build_transfer_context(&mut ctx.accounts)?;
    cpi_mutating_allocate(
        &ctx.accounts.pda_account,
        &ctx.accounts.system_program,
        amount,
        &[],
    )?;
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
    let _ = build_transfer_context(&mut ctx.accounts)?;
    cpi_mutating_allocate(
        &ctx.accounts.pda_account,
        &ctx.accounts.system_program,
        amount,
        &[],
    )?;
    Ok(())
}

fn transfer_helper(ctx: &mut Context<SolTransfer2>, amount: u64) -> Result<()> {
    check_balance(ctx, amount)?;
    transfer_funds(ctx, amount)?;
    access_account_data_from_ctx(ctx)?;
    let _ = ctx.accounts.inner.data; // [safe_account_accessed]
    Ok(())
}

fn transfer_from_pool_helper<'info>(
    from_account: &mut Account<'info, UserState>,
    to_account: &mut SystemAccount<'info>,
    system_program: &Program<'info, System>,
    amount: u64,
) -> Result<()> {
    let _ = to_account; // recipient not mutated; keep for signature parity
    cpi_mutating_allocate(from_account, system_program, amount, &[])?;

    Ok(())
}

// Helper CPI that mutates account data by allocating more space.
fn cpi_mutating_allocate<'info>(
    account: &Account<'info, UserState>,
    system_program: &Program<'info, System>,
    extra_space: u64,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    let new_space = account.to_account_info().data_len() as u64 + extra_space;
    let ix = system_instruction::allocate(&account.key(), new_space);
    let account_infos = vec![account.to_account_info(), system_program.to_account_info()];

    if signer_seeds.is_empty() {
        invoke(&ix, &account_infos)?;
    } else {
        invoke_signed(&ix, &account_infos, signer_seeds)?; // [cpi_call]
    }
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

#[derive(Accounts)]
pub struct SolTransfer3<'info> {
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

impl<'info> SolTransfer3<'info> {
    pub fn cpi_call_safe(&mut self, amount: u64) -> Result<()> {
        let from_pubkey = self.pda_account.to_account_info();
        let to_pubkey = self.recipient.to_account_info();
        let program_id = self.system_program.key();
        let seed = to_pubkey.key();

        let cpi_accounts = Transfer {
            from: from_pubkey,
            to: to_pubkey,
        };

        let bump_seed = 0;
        let signer_seeds: &[&[&[u8]]] = &[&[b"pda", seed.as_ref(), &[bump_seed]]];
        let _ = CpiContext::new(program_id, cpi_accounts);

        cpi_mutating_allocate(
            &self.pda_account,
            &self.system_program,
            amount,
            signer_seeds,
        )?;
        self.pda_account.reload()?;
        Ok(())
    }
    pub fn cpi_call_unsafe(&mut self, amount: u64) -> Result<()> {
        let from_pubkey = self.pda_account.to_account_info();
        let to_pubkey = self.recipient.to_account_info();
        let program_id = self.system_program.key();
        let seed = to_pubkey.key();

        let cpi_accounts = Transfer {
            from: from_pubkey,
            to: to_pubkey,
        };

        let bump_seed = 0;
        let signer_seeds: &[&[&[u8]]] = &[&[b"pda", seed.as_ref(), &[bump_seed]]];
        let _ = CpiContext::new(program_id, cpi_accounts);

        cpi_mutating_allocate(
            &self.pda_account,
            &self.system_program,
            amount,
            signer_seeds,
        )?;
        Ok(())
    }
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
