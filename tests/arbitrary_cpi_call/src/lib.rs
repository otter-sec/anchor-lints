use anchor_lang::prelude::*;
use anchor_lang::system_program::{self, Transfer};

declare_id!("Arb1tr4ryCpi11111111111111111111111111111111");

#[program]
pub mod arbitrary_cpi_call_tests {
    use super::*;

    // This CPI uses an unchecked program account and should trigger the lint.
    pub fn unchecked_cpi(ctx: Context<UncheckedCpi>, amount: u64) -> Result<()> {
        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(ctx.accounts.unchecked_program.key(), cpi_accounts);

        system_program::transfer(cpi_ctx, amount)?; // [arbitrary_cpi_call]
        Ok(())
    }

    // This CPI validates the program id before invoking and should be considered safe.
    pub fn validated_cpi(ctx: Context<ValidatedCpi>, amount: u64) -> Result<()> {
        require_keys_eq!(
            ctx.accounts.unchecked_program.key(),
            system_program::ID,
            CustomError::InvalidProgram
        );
        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(ctx.accounts.unchecked_program.key(), cpi_accounts);

        system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]

        Ok(())
    }

    // This CPI validates the program id before invoking and should be considered safe.
    pub fn validated_cpi_2(
        ctx: Context<ValidatedCpi>,
        target_program_id: Pubkey,
        amount: u64,
    ) -> Result<()> {
        require_keys_eq!(
            target_program_id,
            system_program::ID,
            CustomError::InvalidProgram
        );

        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(target_program_id, cpi_accounts);

        system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]
        Ok(())
    }

    // Case 0: Arbitrary CPI target passed as parameter - unsafe
    pub fn arbitrary_cpi_from_parameter(
        ctx: Context<BasicTransfer>,
        target_program_id: Pubkey,
        amount: u64,
    ) -> Result<()> {
        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(target_program_id, cpi_accounts);
        system_program::transfer(cpi_ctx, amount)?; // [arbitrary_cpi_call]
        Ok(())
    }

    // Case 1: Use constant program id (hardcoded) - safe
    pub fn constant_program_id(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        const SOME_PROGRAM_ID: Pubkey = Pubkey::new_from_array([42u8; 32]);
        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(SOME_PROGRAM_ID, cpi_accounts);
        system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]
        Ok(())
    }

    // Case 2: Use system program id from Anchor - safe
    pub fn system_program_id(ctx: Context<BasicTransfer>, amount: u64) -> Result<()> {
        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(system_program::ID, cpi_accounts);
        system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]
        Ok(())
    }

    // Case 3: Use program id from one of the ctx accounts - unsafe
    pub fn program_id_from_account(ctx: Context<AccountBasedTransfer>, amount: u64) -> Result<()> {
        let pr = ctx.accounts.unchecked_program.key();
        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        let mut cpi_ctx = CpiContext::new(pr, cpi_accounts);
        cpi_ctx.program_id = system_program::ID;
        system_program::transfer(cpi_ctx, amount)?; // [arbitrary_cpi_call]
        Ok(())
    }

    // Case 4: Validate user-provided program ID - safe
    pub fn validated_program_id(
        ctx: Context<BasicTransfer>,
        target_program_id: Pubkey,
        amount: u64,
    ) -> Result<()> {
        require!(
            target_program_id == system_program::ID,
            CustomError::InvalidProgram
        );

        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(target_program_id, cpi_accounts);
        system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]
        Ok(())
    }

    // Case 5: Program ID derived from account data - unsafe
    pub fn program_id_from_account_data(
        ctx: Context<DataBasedTransfer>,
        amount: u64,
    ) -> Result<()> {
        let bytes = ctx.accounts.inner.data.to_le_bytes();
        let pr = Pubkey::new_from_array(bytes.repeat(4).try_into().unwrap_or([0u8; 32]));

        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(pr, cpi_accounts);
        system_program::transfer(cpi_ctx, amount)?; // [arbitrary_cpi_call]
        Ok(())
    }

    // Case 6: Program ID computed from a hash - unsafe
    pub fn program_id_from_hash(
        ctx: Context<BasicTransfer>,
        user_seed: Pubkey,
        amount: u64,
    ) -> Result<()> {
        let mut hash_bytes = [0u8; 32];
        hash_bytes.copy_from_slice(&user_seed.to_bytes()[..32]);
        let derived_pr = Pubkey::new_from_array(hash_bytes);

        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(derived_pr, cpi_accounts);
        system_program::transfer(cpi_ctx, amount)?; // [arbitrary_cpi_call]
        Ok(())
    }

    // Case 7: Proper fix — explicit allowlist for CPI targets - safe
    pub fn allowlist_validation(
        ctx: Context<BasicTransfer>,
        target_program_id: Pubkey,
        amount: u64,
    ) -> Result<()> {
        let allowed_programs = vec![system_program::ID, crate::ID];

        if !allowed_programs.contains(&target_program_id) {
            return err!(CustomError::InvalidProgram);
        }

        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(target_program_id, cpi_accounts);
        system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]
        Ok(())
    }

    // Case 8: Program ID taken from PDA - unsafe
    pub fn program_id_from_pda(
        ctx: Context<BasicTransfer>,
        seed: Vec<u8>,
        amount: u64,
    ) -> Result<()> {
        let (derived_key, _) = Pubkey::find_program_address(&[&seed], &crate::ID);

        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(derived_key, cpi_accounts);
        system_program::transfer(cpi_ctx, amount)?; // [arbitrary_cpi_call]
        Ok(())
    }

    // Case 9: Program ID conditionally validated - unsafe (fallback path is unsafe)
    pub fn conditional_validation_unsafe(
        ctx: Context<BasicTransfer>,
        target_program_id: Pubkey,
        amount: u64,
    ) -> Result<()> {
        if target_program_id == crate::ID {
            let cpi_accounts = Transfer {
                from: ctx.accounts.from.to_account_info(),
                to: ctx.accounts.to.to_account_info(),
            };

            let cpi_ctx = CpiContext::new(target_program_id, cpi_accounts);
            system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]
        } else {
            // Fallback path still executes CPI (unsafe)
            let cpi_accounts = Transfer {
                from: ctx.accounts.from.to_account_info(),
                to: ctx.accounts.to.to_account_info(),
            };

            let cpi_ctx = CpiContext::new(target_program_id, cpi_accounts);
            system_program::transfer(cpi_ctx, amount)?; // [arbitrary_cpi_call]
        }
        Ok(())
    }

    // Case 10: Program ID loaded from account data - unsafe
    pub fn program_id_from_serialized(ctx: Context<DataBasedTransfer>, amount: u64) -> Result<()> {
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&ctx.accounts.inner.data.to_le_bytes());
        let pr = Pubkey::new_from_array(bytes);

        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(pr, cpi_accounts);
        system_program::transfer(cpi_ctx, amount)?; // [arbitrary_cpi_call]
        Ok(())
    }

    // Case 11: Validate user-provided program ID with require_keys_eq - safe
    pub fn validated_with_require_keys_eq(
        ctx: Context<BasicTransfer>,
        target_program_id: Pubkey,
        amount: u64,
    ) -> Result<()> {
        require_keys_eq!(target_program_id, system_program::ID);

        if target_program_id != crate::ID {
            return err!(CustomError::InvalidProgram);
        }

        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(target_program_id, cpi_accounts);
        system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]
        Ok(())
    }

    // Case 12: Direct invoke_signed with system_instruction::transfer - safe
    pub fn direct_invoke_signed_with_system_transfer(
        ctx: Context<DirectInvokeTransfer>,
        amount: u64,
    ) -> Result<()> {
        use anchor_lang::solana_program::program::invoke_signed;
        use anchor_lang::solana_program::system_instruction::transfer;

        // Create instruction (program_id is hardcoded to system_program::ID)
        let instruction = transfer(&ctx.accounts.from.key(), &ctx.accounts.to.key(), amount);

        // Prepare account infos
        let account_infos = vec![
            ctx.accounts.from.to_account_info(),
            ctx.accounts.to.to_account_info(),
        ];

        // Instruction has hardcoded system_program::ID - safe
        let signer_seeds: &[&[&[u8]]] = &[];
        invoke_signed(
            // [safe_cpi_call]
            &instruction,
            &account_infos,
            signer_seeds,
        )?;

        Ok(())
    }

    // Case 13: Direct invoke_signed with arbitrary instruction - unsafe
    pub fn direct_invoke_signed_with_arbitrary_instruction(
        ctx: Context<DirectInvokeTransfer>,
        amount: u64,
    ) -> Result<()> {
        use anchor_lang::solana_program::instruction::Instruction;
        use anchor_lang::solana_program::program::invoke_signed;

        // Create instruction with arbitrary program ID - unsafe
        let instruction = Instruction {
            program_id: ctx.accounts.unchecked_program.key(), // User-controlled program ID
            accounts: vec![],
            data: vec![],
        };

        // Prepare account infos
        let account_infos = vec![
            ctx.accounts.from.to_account_info(),
            ctx.accounts.to.to_account_info(),
        ];

        let signer_seeds: &[&[&[u8]]] = &[];
        invoke_signed( // [arbitrary_cpi_call]
            &instruction,
            &account_infos,
            signer_seeds,
        )?;

        require_keys_eq!(
            ctx.accounts.unchecked_program.key(),
            system_program::ID,
            CustomError::InvalidProgram
        );

        Ok(())
    }

    // Case 16: CpiBuilder pattern with validated account (like clayno-solana-staking)
    pub fn cpi_builder_with_validated_account(
        ctx: Context<CpiBuilderAccounts>,
        amount: u64,
    ) -> Result<()> {
        use anchor_spl::token::Token;

        // This is safe - token_program is validated by Anchor's account constraints
        let cpi_accounts = anchor_spl::token::Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        };

        anchor_spl::token::transfer(
            CpiContext::new(ctx.accounts.token_program.key(), cpi_accounts),
            amount,
        )?; // [safe_cpi_call]

        Ok(())
    }

    // Case 17: CpiBuilder pattern with unchecked account - unsafe
    pub fn cpi_builder_with_unchecked_account(
        ctx: Context<CpiBuilderUnchecked>,
        amount: u64,
    ) -> Result<()> {
        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        // Unsafe - unchecked_program is not validated
        let cpi_ctx = CpiContext::new(ctx.accounts.unchecked_program.key(), cpi_accounts);

        system_program::transfer(cpi_ctx, amount)?; // [arbitrary_cpi_call]

        Ok(())
    }

    // Case 18: Conditional token program selection (like raydium-clmm)
    pub fn conditional_token_program_safe(
        ctx: Context<ConditionalTokenProgram>,
        amount: u64,
    ) -> Result<()> {
        let mut token_program_info = ctx.accounts.token_program.to_account_info();

        // Validate owner before using
        if *ctx.accounts.from.owner == ctx.accounts.token_program_2022.key() {
            token_program_info = ctx.accounts.token_program_2022.to_account_info();
        }

        let cpi_accounts = anchor_spl::token::Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        };

        anchor_spl::token::transfer(
            CpiContext::new(token_program_info.key(), cpi_accounts),
            amount,
        )?; // [safe_cpi_call]

        Ok(())
    }

    // Case 19: Direct invoke_signed with hardcoded program ID (like damm-v2)
    pub fn direct_invoke_signed_hardcoded(
        ctx: Context<DirectInvokeTransfer>,
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

    // Case 20: Direct invoke_signed with program ID from account data - unsafe
    pub fn direct_invoke_signed_from_account_data(
        ctx: Context<DirectInvokeFromData>,
        amount: u64,
    ) -> Result<()> {
        use anchor_lang::solana_program::instruction::Instruction;
        use anchor_lang::solana_program::program::invoke_signed;

        // Read program ID from account data - unsafe
        let program_id = Pubkey::try_from(&ctx.accounts.config_account.data.borrow()[0..32])
            .map_err(|_| CustomError::InvalidData)?;

        let instruction = Instruction {
            program_id,
            accounts: vec![],
            data: vec![],
        };

        let account_infos = vec![ctx.accounts.config_account.to_account_info()];
        let signer_seeds: &[&[&[u8]]] = &[];
        invoke_signed(&instruction, &account_infos, signer_seeds)?; // [arbitrary_cpi_call]

        Ok(())
    }

    // Case 21: CPI with program ID from PDA derivation - potentially unsafe
    pub fn cpi_with_pda_derived_program(
        ctx: Context<PdaDerivedProgram>,
        amount: u64,
    ) -> Result<()> {
        let (program_pda, _bump) = Pubkey::find_program_address(
            &[b"program", ctx.accounts.authority.key.as_ref()],
            &ctx.program_id,
        );

        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        // Unsafe - PDA-derived program ID could be manipulated
        let cpi_ctx = CpiContext::new(program_pda, cpi_accounts);
        system_program::transfer(cpi_ctx, amount)?; // [arbitrary_cpi_call]

        Ok(())
    }

    // Case 22: CPI with program ID validated against whitelist - safe
    pub fn cpi_with_whitelist_validation(
        ctx: Context<WhitelistValidation>,
        amount: u64,
    ) -> Result<()> {
        const ALLOWED_PROGRAMS: [Pubkey; 2] = [system_program::ID, anchor_spl::token::ID];

        require!(
            ALLOWED_PROGRAMS.contains(&ctx.accounts.target_program.key()),
            CustomError::InvalidProgram
        );

        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(ctx.accounts.target_program.key(), cpi_accounts);

        system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]

        Ok(())
    }

    // Case 23: CPI with program ID from account but validated - safe
    pub fn cpi_with_account_validation(ctx: Context<AccountValidation>, amount: u64) -> Result<()> {
        // Validate the program account owner
        require_keys_eq!(
            *ctx.accounts.target_program.owner,
            system_program::ID,
            CustomError::InvalidProgram
        );

        // Validate the program key matches expected
        require_keys_eq!(
            ctx.accounts.target_program.key(),
            system_program::ID,
            CustomError::InvalidProgram
        );

        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(ctx.accounts.target_program.key(), cpi_accounts);

        system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]

        Ok(())
    }

    // Case 24: Nested CPI call with validated program - safe
    pub fn nested_cpi_with_validation(ctx: Context<NestedCpiAccounts>, amount: u64) -> Result<()> {
        // First validate
        require_keys_eq!(
            ctx.accounts.target_program.key(),
            system_program::ID,
            CustomError::InvalidProgram
        );

        // Then make CPI call
        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(ctx.accounts.target_program.key(), cpi_accounts);

        system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]

        // Nested CPI with same validated program
        let nested_cpi_accounts = Transfer {
            from: ctx.accounts.to.to_account_info(),
            to: ctx.accounts.from.to_account_info(),
        };

        let nested_cpi_ctx =
            CpiContext::new(ctx.accounts.target_program.key(), nested_cpi_accounts);

        system_program::transfer(nested_cpi_ctx, amount)?; // [safe_cpi_call]

        Ok(())
    }

    // Case 25: Helper function with individual account parameters - unsafe
    pub fn helper_with_individual_accounts(ctx: Context<UncheckedCpi>, amount: u64) -> Result<()> {
        // Direct call to helper with individual accounts - unsafe
        cpi_call_with_account(
            &ctx.accounts.from,
            &ctx.accounts.to,
            &ctx.accounts.unchecked_program,
            amount,
        )?;
        Ok(())
    }

    // Case 26: Helper function with accounts struct - unsafe
    pub fn helper_with_accounts_struct(ctx: Context<UncheckedCpi>, amount: u64) -> Result<()> {
        // Direct call to helper with accounts struct - unsafe
        cpi_call_with_accounts(&ctx.accounts, amount)?;
        Ok(())
    }

    // Case 27: Helper function with validation then CPI - safe
    pub fn helper_with_validation_then_cpi(
        mut ctx: Context<UncheckedCpi>,
        amount: u64,
    ) -> Result<()> {
        // Validate first
        checked_cpi_call_with_accounts(&mut ctx.accounts)?;
        // Then make CPI call - safe because validation happened
        cpi_call_with_accounts(&mut ctx.accounts, amount)?; // [safe_cpi_call]
        Ok(())
    }

    // Case 28: Helper function with validation in nested helper - safe
    pub fn helper_with_nested_validation(
        mut ctx: Context<UncheckedCpi>,
        amount: u64,
    ) -> Result<()> {
        // This calls checked_cpi_call_with_accounts which validates, then makes CPI
        // Should be safe because validation happens in the helper chain
        nested_cpi_in_helper_function(ctx, amount)?; // [safe_cpi_call]
        Ok(())
    }

    // Case 29: Helper function with context accounts pattern - safe
    pub fn helper_with_context_accounts_pattern(
        mut ctx: Context<UncheckedCpi>,
        amount: u64,
    ) -> Result<()> {
        // This validates in helper then makes CPI - safe
        nested_cpi_in_helper_with_context_accounts(ctx, amount)?; // [safe_cpi_call]
        Ok(())
    }

    // Case 30: Helper function without validation - unsafe
    pub fn helper_without_validation(mut ctx: Context<UncheckedCpi>, amount: u64) -> Result<()> {
        // No validation before CPI - unsafe
        cpi_call_with_accounts(&mut ctx.accounts, amount)?;
        Ok(())
    }

    // Case 31: Helper function with individual accounts and validation - safe
    pub fn helper_with_individual_accounts_validated(
        mut ctx: Context<UncheckedCpi>,
        amount: u64,
    ) -> Result<()> {
        // Validate first
        checked_cpi_call_with_account(&mut ctx.accounts.unchecked_program)?;
        // Then make CPI call with individual accounts - safe because validation happened
        cpi_call_with_account_safe(
            &ctx.accounts.from,
            &ctx.accounts.to,
            &ctx.accounts.unchecked_program,
            amount,
        )?;
        Ok(())
    }

    // Case 32: Nested helper function with individual accounts pattern - safe
    pub fn nested_helper_with_individual_accounts(
        mut ctx: Context<UncheckedCpi>,
        amount: u64,
    ) -> Result<()> {
        // This calls a helper that validates then makes CPI with individual accounts
        // Should be safe because validation happens in the helper chain
        nested_cpi_call_with_individual_accounts(
            &ctx.accounts.from,
            &ctx.accounts.to,
            &mut ctx.accounts.unchecked_program,
            amount,
        )?;
        Ok(())
    }

    // Case 33: Nested helper function with individual accounts pattern - safe
    pub fn nested_helper_with_function_param_account(
        mut ctx: Context<UncheckedCpi>,
        program: Pubkey,
        amount: u64,
    ) -> Result<()> {
        nested_cpi_call_with_function_param_account(
            &ctx.accounts.from,
            &ctx.accounts.to,
            program,
            amount,
        )?;
        Ok(())
    }
}

pub fn cpi_call_with_account<'info>(
    from: &Signer<'info>,
    to: &UncheckedAccount<'info>,
    program: &UncheckedAccount<'info>,
    amount: u64,
) -> Result<()> {
    let cpi_accounts = Transfer {
        from: from.to_account_info(),
        to: to.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(program.key(), cpi_accounts);
    system_program::transfer(cpi_ctx, amount)?; // [arbitrary_cpi_call]
    Ok(())
}

/// Helper function that takes accounts struct and makes a CPI call
/// This should be flagged as unsafe when called without validation
pub fn cpi_call_with_accounts(ctx_accounts: &UncheckedCpi, amount: u64) -> Result<()> {
    cpi_call_with_account(
        &ctx_accounts.from,
        &ctx_accounts.to,
        &ctx_accounts.unchecked_program,
        amount,
    )?;
    Ok(())
}

/// Helper function that validates the program account
/// This is safe and should be used before making CPI calls
pub fn checked_cpi_call_with_accounts(acc_mut: &mut UncheckedCpi) -> Result<()> {
    checked_cpi_call_with_account(&mut acc_mut.unchecked_program)?;
    Ok(())
}

pub fn checked_cpi_call_with_account(program_account: &UncheckedAccount) -> Result<()> {
    require_keys_eq!(
        program_account.key(),
        system_program::ID,
        CustomError::InvalidProgram
    );
    Ok(())
}

/// Nested helper function that validates then makes CPI call
/// This should be safe because validation happens before CPI
pub fn nested_cpi_in_helper_function(mut ctx: Context<UncheckedCpi>, amount: u64) -> Result<()> {
    checked_cpi_call_with_accounts(&mut ctx.accounts)?;
    cpi_call_with_accounts(&mut ctx.accounts, amount)?;
    Ok(())
}

/// Nested helper function with context accounts pattern
/// Validates in helper then makes CPI - should be safe
pub fn nested_cpi_in_helper_with_context_accounts(
    mut ctx: Context<UncheckedCpi>,
    amount: u64,
) -> Result<()> {
    checked_cpi_call_with_accounts(&mut ctx.accounts)?;
    cpi_call_with_accounts(&mut ctx.accounts, amount)?;
    Ok(())
}

pub fn cpi_call_with_account_safe<'info>(
    from: &Signer<'info>,
    to: &UncheckedAccount<'info>,
    program: &UncheckedAccount<'info>,
    amount: u64,
) -> Result<()> {
    let cpi_accounts = Transfer {
        from: from.to_account_info(),
        to: to.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(program.key(), cpi_accounts);
    system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]
    Ok(())
}

pub fn nested_cpi_call_with_individual_accounts<'info>(
    from: &Signer<'info>,
    to: &UncheckedAccount<'info>,
    program: &mut UncheckedAccount<'info>,
    amount: u64,
) -> Result<()> {
    // Validate first
    checked_cpi_call_with_account(program)?;
    // Then make CPI call with individual accounts - safe because validation happened
    cpi_call_with_account_safe(from, to, program, amount)?;
    Ok(())
}

pub fn checked_cpi_call_with_function_param_account(program: Pubkey) -> Result<()> {
    require_keys_eq!(program, system_program::ID, CustomError::InvalidProgram);
    Ok(())
}

pub fn cpi_call_with_function_param_account<'info>(
    from: &Signer<'info>,
    to: &UncheckedAccount<'info>,
    program: Pubkey,
    amount: u64,
) -> Result<()> {
    let cpi_accounts = Transfer {
        from: from.to_account_info(),
        to: to.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(program, cpi_accounts);
    system_program::transfer(cpi_ctx, amount)?; // [safe_cpi_call]
    Ok(())
}

pub fn nested_cpi_call_with_function_param_account<'info>(
    from: &Signer<'info>,
    to: &UncheckedAccount<'info>,
    program: Pubkey,
    amount: u64,
) -> Result<()> {
    checked_cpi_call_with_function_param_account(program)?;
    cpi_call_with_function_param_account(from, to, program, amount)?;
    Ok(())
}

#[derive(Accounts)]
pub struct UncheckedCpi<'info> {
    #[account(mut)]
    pub from: Signer<'info>,
    #[account(mut)]
    /// CHECK: test fixture
    pub to: UncheckedAccount<'info>,
    /// CHECK: test fixture
    pub unchecked_program: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct ValidatedCpi<'info> {
    #[account(mut)]
    pub from: Signer<'info>,
    #[account(mut)]
    /// CHECK: test fixture
    pub to: UncheckedAccount<'info>,
    /// CHECK: test fixture
    pub unchecked_program: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct BasicTransfer<'info> {
    #[account(mut)]
    pub from: Signer<'info>,
    #[account(mut)]
    /// CHECK: test fixture
    pub to: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct AccountBasedTransfer<'info> {
    #[account(mut)]
    pub from: Signer<'info>,
    #[account(mut)]
    /// CHECK: test fixture
    pub to: UncheckedAccount<'info>,
    /// CHECK: test fixture
    pub unchecked_program: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct DataBasedTransfer<'info> {
    #[account(mut)]
    pub from: Signer<'info>,
    #[account(mut)]
    /// CHECK: test fixture
    pub to: UncheckedAccount<'info>,
    #[account(mut)]
    pub inner: Account<'info, InnerAccount>,
}

#[derive(Accounts)]
pub struct DirectInvokeTransfer<'info> {
    #[account(mut)]
    pub from: Signer<'info>,
    #[account(mut)]
    /// CHECK: test fixture
    pub to: UncheckedAccount<'info>,
    /// CHECK: test fixture
    pub unchecked_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CpiBuilderAccounts<'info> {
    #[account(mut)]
    pub from: Signer<'info>,
    #[account(mut)]
    pub to: AccountInfo<'info>,
    pub authority: Signer<'info>,
    pub token_program: Program<'info, anchor_spl::token::Token>, // Validated by Anchor
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CpiBuilderUnchecked<'info> {
    #[account(mut)]
    pub from: Signer<'info>,
    #[account(mut)]
    pub to: AccountInfo<'info>,
    /// CHECK: Unchecked program account
    pub unchecked_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ConditionalTokenProgram<'info> {
    #[account(mut)]
    pub from: AccountInfo<'info>,
    #[account(mut)]
    pub to: AccountInfo<'info>,
    pub authority: Signer<'info>,
    pub token_program: Program<'info, anchor_spl::token::Token>,
    pub token_program_2022: Program<'info, anchor_spl::token_2022::Token2022>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ConditionalTokenProgramUnchecked<'info> {
    #[account(mut)]
    pub from: AccountInfo<'info>,
    #[account(mut)]
    pub to: AccountInfo<'info>,
    pub authority: Signer<'info>,
    /// CHECK: Unchecked program account
    pub unchecked_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct DirectInvokeFromData<'info> {
    /// CHECK: Config account containing program ID
    pub config_account: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct PdaDerivedProgram<'info> {
    #[account(mut)]
    pub from: Signer<'info>,
    #[account(mut)]
    pub to: AccountInfo<'info>,
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct WhitelistValidation<'info> {
    #[account(mut)]
    pub from: Signer<'info>,
    #[account(mut)]
    pub to: AccountInfo<'info>,
    /// CHECK: Target program to validate against whitelist
    pub target_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AccountValidation<'info> {
    #[account(mut)]
    pub from: Signer<'info>,
    #[account(mut)]
    pub to: AccountInfo<'info>,
    /// CHECK: Target program to validate
    pub target_program: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct NestedCpiAccounts<'info> {
    #[account(mut)]
    pub from: Signer<'info>,
    #[account(mut)]
    pub to: AccountInfo<'info>,
    /// CHECK: Target program to validate
    pub target_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[account]
pub struct InnerAccount {
    pub data: u64,
}

#[error_code]
pub enum CustomError {
    #[msg("Invalid program")]
    InvalidProgram,
    #[msg("Invalid data")]
    InvalidData,
}
