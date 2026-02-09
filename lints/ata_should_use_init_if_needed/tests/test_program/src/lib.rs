use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
    token_interface::{Mint as InterfaceMint, TokenAccount as InterfaceTokenAccount, TokenInterface},
};

declare_id!("11111111111111111111111111111111");

#[program]
pub mod ata_should_use_init_if_needed_tests {
    use super::*;

    // BAD: Uses `init` instead of `init_if_needed` for ATA
    pub fn deposit_with_init(
        ctx: Context<DepositWithInit>,
        amount: u64,
    ) -> Result<()> {
        msg!("Depositing {}", amount);
        Ok(())
    }

    // GOOD: Uses `init_if_needed` for ATA (commented out to avoid compilation errors in test)
    pub fn deposit_with_init_if_needed(
        ctx: Context<DepositWithInitIfNeeded>,
        amount: u64,
    ) -> Result<()> {
        msg!("Depositing {}", amount);
        Ok(())
    }

    // BAD: Uses `init` with InterfaceAccount TokenAccount
    pub fn deposit_interface_account(
        ctx: Context<DepositInterfaceAccount>,
        amount: u64,
    ) -> Result<()> {
        msg!("Depositing {}", amount);
        Ok(())
    }

    // GOOD: Uses `init_if_needed` with InterfaceAccount TokenAccount (commented out to avoid compilation errors)
    pub fn deposit_interface_account_safe(
        ctx: Context<DepositInterfaceAccountSafe>,
        amount: u64,
    ) -> Result<()> {
        msg!("Depositing {}", amount);
        Ok(())
    }

    // GOOD: Regular account with `init` (not an ATA) - should not trigger
    pub fn init_regular_account(
        ctx: Context<InitRegularAccount>,
        value: u64,
    ) -> Result<()> {
        ctx.accounts.account.value = value;
        Ok(())
    }

    // GOOD: ATA without `init` constraint - should not trigger
    pub fn use_existing_ata(
        ctx: Context<UseExistingAta>,
    ) -> Result<()> {
        msg!("Using existing ATA");
        Ok(())
    }
}

// BAD: Uses `init` with associated_token constraints
#[derive(Accounts)]
pub struct DepositWithInit<'info> {
    #[account(
        init,
        associated_token::authority = user,
        associated_token::mint = mint,
        associated_token::token_program = token_program,
        payer = user
    )]
    pub user_token_account: Account<'info, TokenAccount>, // [ata_should_use_init_if_needed]

    #[account(mut)]
    pub user: Signer<'info>,

    pub mint: Account<'info, Mint>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

// GOOD: Uses `init_if_needed` with associated_token constraints (commented out to avoid compilation errors)
#[derive(Accounts)]
pub struct DepositWithInitIfNeeded<'info> {
    #[account(
        init_if_needed,
        associated_token::authority = user,
        associated_token::mint = mint,
        associated_token::token_program = token_program,
        payer = user
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub mint: Account<'info, Mint>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

// BAD: Uses `init` with InterfaceAccount TokenAccount
#[derive(Accounts)]
pub struct DepositInterfaceAccount<'info> {
    #[account(
        init,  
        associated_token::authority = user,
        associated_token::mint = mint,
        associated_token::token_program = token_program,
        payer = user
    )]
    pub user_token_account: InterfaceAccount<'info, InterfaceTokenAccount>, // [ata_should_use_init_if_needed]

    #[account(mut)]
    pub user: Signer<'info>,

    pub mint: InterfaceAccount<'info, InterfaceMint>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

// GOOD: Uses `init_if_needed` with InterfaceAccount TokenAccount (commented out to avoid compilation errors)
#[derive(Accounts)]
pub struct DepositInterfaceAccountSafe<'info> {
    #[account(
        init_if_needed,
        associated_token::authority = user,
        associated_token::mint = mint,
        associated_token::token_program = token_program,
        payer = user
    )]
    pub user_token_account: InterfaceAccount<'info, InterfaceTokenAccount>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub mint: InterfaceAccount<'info, InterfaceMint>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

// GOOD: Regular account (not ATA) with `init` - should not trigger
#[account]
pub struct RegularAccount {
    pub value: u64,
}

#[derive(Accounts)]
pub struct InitRegularAccount<'info> {
    #[account(
        init,
        payer = user,
        space = 8 + 8
    )]
    pub account: Account<'info, RegularAccount>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub system_program: Program<'info, System>,
}

// GOOD: ATA without `init` constraint - should not trigger
#[derive(Accounts)]
pub struct UseExistingAta<'info> {
    #[account(
        associated_token::authority = user,
        associated_token::mint = mint,
        associated_token::token_program = token_program,
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    pub user: Signer<'info>,
    pub mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
}

