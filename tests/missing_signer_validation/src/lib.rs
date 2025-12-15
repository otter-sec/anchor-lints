use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::solana_program::program::invoke;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::system_program::{self, Transfer};
use anchor_spl::associated_token::{self, Create};
use anchor_spl::token::{
    self, Approve, Burn, CloseAccount, FreezeAccount, Mint, MintTo, Revoke, SetAuthority, 
    SyncNative, ThawAccount, Token, TokenAccount, Transfer as SplTransfer,
};
use anchor_spl::token_2022::{Token2022, spl_token_2022};

declare_id!("M1ss1ngS1gn3r111111111111111111111111111111");

#[program]
pub mod missing_signer_validation_tests {
    use super::*;

    // Case 1: system_program::transfer with account missing #[account(signer)]
    pub fn transfer_missing_signer(ctx: Context<TransferMissingSigner>, amount: u64) -> Result<()> {
        let cpi_accounts = Transfer { // [missing_signer_validation]
            from: ctx.accounts.from.to_account_info(), 
            to: ctx.accounts.to.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.system_program.key(), cpi_accounts);
        system_program::transfer(cpi_ctx, amount)?;
        Ok(())
    }

    // Case 2: system_program::transfer with signer checked
    pub fn transfer_with_signer_attribute(
        ctx: Context<TransferWithSigner>,
        amount: u64,
    ) -> Result<()> {
        system_program::transfer( // [safe_signer_validation]
            CpiContext::new(
                ctx.accounts.system_program.key(),
                Transfer {
                    from: ctx.accounts.from.to_account_info(),
                    to: ctx.accounts.to.to_account_info(),
                },
            ),
            amount,
        )?;
        Ok(())
    }

    // Case 3: system_program::transfer with signer not checked
    pub fn transfer_with_signer_account(
        ctx: Context<MultipleAccountsOneMissing>,
        amount: u64,
    ) -> Result<()> {
        system_program::transfer( // [safe_signer_validation]
            CpiContext::new(
                ctx.accounts.system_program.key(),
                Transfer {
                    from: ctx.accounts.authority.to_account_info(),
                    to: ctx.accounts.to.to_account_info(),
                },
            ),
            amount,
        )?;
        Ok(())
    }

    // Case 4: spl_token_2022::instruction::transfer_checked
    pub fn transfer_checked_cpi(ctx: Context<TestAccounts>) -> Result<()> {
        let ix = spl_token_2022::instruction::transfer_checked(
            ctx.accounts.token_program.key,
            ctx.accounts.from.key,
            ctx.accounts.mint.key,
            ctx.accounts.to.key,
            ctx.accounts.authority.key, // [missing_signer_validation]
            &[],
            100,
            6,
        )?;

        anchor_lang::solana_program::program::invoke(
            &ix,
            &[
                ctx.accounts.from.to_account_info(),
                ctx.accounts.mint.to_account_info(),
                ctx.accounts.to.to_account_info(),
                ctx.accounts.authority.to_account_info(),
                ctx.accounts.token_program.to_account_info(),
            ],
        )?;

        Ok(())
    }

    // Case 5: spl_token_2022::instruction::transfer_checked with signer
    pub fn transfer_checked_cpi_with_signer_attribute(
        ctx: Context<TransferCheckedCpiWithSigner>,
    ) -> Result<()> {
        let ix = spl_token_2022::instruction::transfer_checked(
            ctx.accounts.token_program.key,
            ctx.accounts.from.key,
            ctx.accounts.mint.key,
            ctx.accounts.to.key,
            ctx.accounts.authority.key, // [safe_signer_validation]
            &[],
            100,
            6,
        )?;

        anchor_lang::solana_program::program::invoke(
            &ix,
            &[
                ctx.accounts.from.to_account_info(),
                ctx.accounts.mint.to_account_info(),
                ctx.accounts.to.to_account_info(),
                ctx.accounts.authority.to_account_info(),
                ctx.accounts.token_program.to_account_info(),
            ],
        )?;

        Ok(())
    }
    // Case 6: spl_token_2022::instruction::transfer_checked
    pub fn transfer_checked_cpi_with_signer(ctx: Context<TestAccounts>) -> Result<()> {
        let ix = spl_token_2022::instruction::transfer(
            ctx.accounts.token_program.key,
            ctx.accounts.from.key,
            ctx.accounts.to.key,
            ctx.accounts.authority.key, // [missing_signer_validation]
            &[],
            100,
        )?;

        anchor_lang::solana_program::program::invoke(
            &ix,
            &[
                ctx.accounts.from.to_account_info(),
                ctx.accounts.mint.to_account_info(),
                ctx.accounts.to.to_account_info(),
                ctx.accounts.authority.to_account_info(),
                ctx.accounts.token_program.to_account_info(),
            ],
        )?;

        Ok(())
    }

    // Case 7: system_program::transfer with account missing #[account(signer)]
    pub fn spl_transfer_missing_signer(
        ctx: Context<TransferMissingSigner>,
        amount: u64,
    ) -> Result<()> {
        let cpi_accounts = SplTransfer {
            // [safe_signer_validation]
            authority: ctx.accounts.authority.to_account_info(),
            from: ctx.accounts.from.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.system_program.key(), cpi_accounts);
        token::transfer(cpi_ctx, amount)?;
        Ok(())
    }

    // Case 8: Missing signer on payer => should trigger missing_signer_validation
    pub fn create_ata_missing_signer(ctx: Context<CreateAtaMissingSigner>) -> Result<()> {
        let cpi_ctx = CpiContext::new(
            ctx.accounts.associated_token_program.key(),
            Create {  // [missing_signer_validation]
                payer: ctx.accounts.payer.to_account_info(),
                associated_token: ctx.accounts.ata.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
                token_program: ctx.accounts.token_program.to_account_info(),
            },
        );
        associated_token::create(cpi_ctx)?;
        Ok(())
    }

    // Case 9: Payer is signer => no error
    pub fn create_ata_with_signer(ctx: Context<CreateAtaWithSigner>) -> Result<()> {
        let cpi_ctx = CpiContext::new(
            ctx.accounts.associated_token_program.key(),
            Create {
                // [safe_signer_validation]
                payer: ctx.accounts.payer.to_account_info(),
                associated_token: ctx.accounts.ata.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
                token_program: ctx.accounts.token_program.to_account_info(),
            },
        );
        associated_token::create(cpi_ctx)?;
        Ok(())
    }

    // Case 10: Missing signer on mint_authority => should trigger missing_signer_validation
    pub fn mint_to_missing_signer(ctx: Context<MintToMissingSigner>, amount: u64) -> Result<()> {
        let cpi_accounts = MintTo {  // [missing_signer_validation]
            mint: ctx.accounts.mint.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
            authority: ctx.accounts.mint_authority.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.key(), cpi_accounts);

        token::mint_to(cpi_ctx, amount)?;

        Ok(())
    }

    // Case 11: Mint authority is signer => no error
    pub fn mint_to_with_signer(ctx: Context<MintToWithSigner>, amount: u64) -> Result<()> {
        let cpi_accounts = MintTo {
            // [safe_signer_validation]
            mint: ctx.accounts.mint.to_account_info(),
            to: ctx.accounts.to.to_account_info(),
            authority: ctx.accounts.mint_authority.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.key(), cpi_accounts);

        token::mint_to(cpi_ctx, amount)?;

        Ok(())
    }

    // Case 12: Missing signer on authority => should trigger missing_signer_validation
    pub fn burn_missing_signer(ctx: Context<BurnMissingSigner>, amount: u64) -> Result<()> {
        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Burn { // [missing_signer_validation]
                    mint: ctx.accounts.mint.to_account_info(),
                    from: ctx.accounts.from.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(), 
                },
            ),
            amount,
        )?;

        Ok(())
    }

    // Case 13: Authority is signer => no error
    pub fn burn_with_signer(ctx: Context<BurnWithSigner>, amount: u64) -> Result<()> {
        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Burn {
                    // [safe_signer_validation]
                    mint: ctx.accounts.mint.to_account_info(),
                    from: ctx.accounts.from.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
            amount,
        )?;

        Ok(())
    }

    // Case 14: Missing signer on current_authority => should trigger missing_signer_validation
    pub fn set_authority_missing_signer(ctx: Context<SetAuthorityMissingSigner>) -> Result<()> {
        token::set_authority(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                SetAuthority { // [missing_signer_validation]
                    account_or_mint: ctx.accounts.account.to_account_info(),
                    current_authority: ctx.accounts.current_authority.to_account_info(),
                },
            ),
            anchor_spl::token::spl_token::instruction::AuthorityType::AccountOwner,
            Some(ctx.accounts.new_authority.key()),
        )?;

        Ok(())
    }

    // Case 15: Mint to with signer
    pub fn mint_to_with_pda_signer(ctx: Context<MintToWithPdaSigner>) -> Result<()> {
        // Mint 100 tokens to user account, signed by PDA
        let binding = ctx.accounts.mint.key();
        let seeds = &[b"mint_authority", binding.as_ref()];
        let signer = &[&seeds[..]];

        let cpi_accounts = token::MintTo {
            mint: ctx.accounts.mint.to_account_info(),
            to: ctx.accounts.user_token.to_account_info(),
            authority: ctx.accounts.mint_authority.to_account_info(), // [safe_signer_validation]
        };
        let cpi_ctx =
            CpiContext::new_with_signer(ctx.accounts.token_program.key(), cpi_accounts, signer);

        token::mint_to(cpi_ctx, 100)?;

        Ok(())
    }

    // Case 16: Missing signer on authority => should trigger missing_signer_validation
    pub fn close_account_missing_signer(ctx: Context<CloseAccountMissingSigner>) -> Result<()> {
        token::close_account(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                CloseAccount { // [missing_signer_validation]
                    account: ctx.accounts.account.to_account_info(),
                    destination: ctx.accounts.destination.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
        )?;

        Ok(())
    }

    // Case 17: Authority is signer => no error
    pub fn close_account_with_signer(ctx: Context<CloseAccountWithSigner>) -> Result<()> {
        token::close_account(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                CloseAccount {
                    // [safe_signer_validation]
                    account: ctx.accounts.account.to_account_info(),
                    destination: ctx.accounts.destination.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
        )?;

        Ok(())
    }

    // Case 18: Missing signer on authority => should trigger missing_signer_validation
    pub fn freeze_account_missing_signer(ctx: Context<FreezeAccountMissingSigner>) -> Result<()> {
        token::freeze_account(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                FreezeAccount { // [missing_signer_validation]
                    account: ctx.accounts.account.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
        )?;

        Ok(())
    }

    // Case 19: Authority is signer => no error
    pub fn freeze_account_with_signer(ctx: Context<FreezeAccountWithSigner>) -> Result<()> {
        token::freeze_account(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                FreezeAccount {
                    // [safe_signer_validation]
                    account: ctx.accounts.account.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
        )?;

        Ok(())
    }

    // Case 20: Missing signer on authority => should trigger missing_signer_validation
    pub fn thaw_account_missing_signer(ctx: Context<ThawAccountMissingSigner>) -> Result<()> {
        token::thaw_account(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                ThawAccount { // [missing_signer_validation]
                    account: ctx.accounts.account.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
        )?;

        Ok(())
    }

    // Case 21: Authority is signer => no error
    pub fn thaw_account_with_signer(ctx: Context<ThawAccountWithSigner>) -> Result<()> {
        token::thaw_account(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                ThawAccount {
                    // [safe_signer_validation]
                    account: ctx.accounts.account.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
        )?;

        Ok(())
    }

    // Case 22: Missing signer on authority => should trigger missing_signer_validation
    pub fn approve_missing_signer(ctx: Context<ApproveMissingSigner>, amount: u64) -> Result<()> {
        token::approve(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Approve { // [missing_signer_validation]
                    to: ctx.accounts.to.to_account_info(),
                    delegate: ctx.accounts.delegate.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
            amount,
        )?;

        Ok(())
    }

    // Case 23: Authority is signer => no error
    pub fn approve_with_signer(ctx: Context<ApproveWithSigner>, amount: u64) -> Result<()> {
        token::approve(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Approve {
                    // [safe_signer_validation]
                    to: ctx.accounts.to.to_account_info(),
                    delegate: ctx.accounts.delegate.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
            amount,
        )?;

        Ok(())
    }

    // Case 24: Missing signer on authority => should trigger missing_signer_validation
    pub fn revoke_missing_signer(ctx: Context<RevokeMissingSigner>) -> Result<()> {
        token::revoke(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Revoke { // [missing_signer_validation]
                    source: ctx.accounts.source.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
        )?;

        Ok(())
    }

    // Case 25: Authority is signer => no error
    pub fn revoke_with_signer(ctx: Context<RevokeWithSigner>) -> Result<()> {
        token::revoke(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Revoke { // [safe_signer_validation]
                    source: ctx.accounts.source.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
        )?;

        Ok(())
    }

    // Case 26: Missing signer on authority => should trigger missing_signer_validation
    pub fn sync_native_missing_signer(ctx: Context<SyncNativeMissingSigner>) -> Result<()> {
        token::sync_native(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                SyncNative { // [missing_signer_validation]
                    account: ctx.accounts.account.to_account_info(),
                },
            ),
        )?;

        Ok(())
    }

    // Case 27: Authority is signer => no error (SyncNative doesn't require authority in struct)
    pub fn sync_native_with_signer(ctx: Context<SyncNativeWithSigner>) -> Result<()> {
        token::sync_native(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                SyncNative {
                    // [safe_signer_validation]
                    account: ctx.accounts.account.to_account_info(),
                },
            ),
        )?;

        Ok(())
    }

    // Case 28: Token-2022 mint_to_checked with missing signer
    pub fn token2022_mint_to_checked_missing_signer(
        ctx: Context<Token2022MintToCheckedMissingSigner>,
        amount: u64,
    ) -> Result<()> {
        let ix = spl_token_2022::instruction::mint_to_checked(
            ctx.accounts.token_program.key,
            ctx.accounts.mint.key,
            ctx.accounts.to.key,
            ctx.accounts.mint_authority.key, // [missing_signer_validation]
            &[],
            amount,
            6,
        )?;

        anchor_lang::solana_program::program::invoke(
            &ix,
            &[
                ctx.accounts.mint.to_account_info(),
                ctx.accounts.to.to_account_info(),
                ctx.accounts.mint_authority.to_account_info(),
                ctx.accounts.token_program.to_account_info(),
            ],
        )?;

        Ok(())
    }

    // Case 29: Token-2022 mint_to_checked with signer
    pub fn token2022_mint_to_checked_with_signer(
        ctx: Context<Token2022MintToCheckedWithSigner>,
        amount: u64,
    ) -> Result<()> {
        let ix = spl_token_2022::instruction::mint_to_checked(
            ctx.accounts.token_program.key,
            ctx.accounts.mint.key,
            ctx.accounts.to.key,
            ctx.accounts.mint_authority.key, // [safe_signer_validation]
            &[],
            amount,
            6,
        )?;

        anchor_lang::solana_program::program::invoke(
            &ix,
            &[
                ctx.accounts.mint.to_account_info(),
                ctx.accounts.to.to_account_info(),
                ctx.accounts.mint_authority.to_account_info(),
                ctx.accounts.token_program.to_account_info(),
            ],
        )?;

        Ok(())
    }

    // Case 30: Token-2022 burn_checked with missing signer
    pub fn token2022_burn_checked_missing_signer(
        ctx: Context<Token2022BurnCheckedMissingSigner>,
        amount: u64,
    ) -> Result<()> {
        let ix = spl_token_2022::instruction::burn_checked(
            ctx.accounts.token_program.key,
            ctx.accounts.from.key,
            ctx.accounts.mint.key,
            ctx.accounts.authority.key, // [missing_signer_validation]
            &[],
            amount,
            6,
        )?;

        anchor_lang::solana_program::program::invoke(
            &ix,
            &[
                ctx.accounts.from.to_account_info(),
                ctx.accounts.mint.to_account_info(),
                ctx.accounts.authority.to_account_info(),
                ctx.accounts.token_program.to_account_info(),
            ],
        )?;

        Ok(())
    }

    // Case 31: Token-2022 burn_checked with signer
    pub fn token2022_burn_checked_with_signer(
        ctx: Context<Token2022BurnCheckedWithSigner>,
        amount: u64,
    ) -> Result<()> {
        let ix = spl_token_2022::instruction::burn_checked(
            ctx.accounts.token_program.key,
            ctx.accounts.from.key,
            ctx.accounts.mint.key,
            ctx.accounts.authority.key, // [safe_signer_validation]
            &[],
            amount,
            6,
        )?;

        anchor_lang::solana_program::program::invoke(
            &ix,
            &[
                ctx.accounts.from.to_account_info(),
                ctx.accounts.mint.to_account_info(),
                ctx.accounts.authority.to_account_info(),
                ctx.accounts.token_program.to_account_info(),
            ],
        )?;

        Ok(())
    }
}

impl<'info> TransferMissingSigner<'info> {
    // Case 1: system_program::transfer with account missing #[account(signer)]
    pub fn self_implemented_transfer_missing_signer(&mut self, amount: u64) -> Result<()> {
        let cpi_accounts = Transfer { // [missing_signer_validation]
            from: self.from.to_account_info(),
            to: self.to.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(self.system_program.key(), cpi_accounts);
        system_program::transfer(cpi_ctx, amount)?;
        Ok(())
    }
}

// Account structs for unsafe cases
#[derive(Accounts)]
pub struct TransferMissingSigner<'info> {
    #[account(mut)]
    pub from: Account<'info, UserState>, // Missing #[account(signer)]
    #[account(mut)]
    pub to: AccountInfo<'info>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct MultipleAccountsOneMissing<'info> {
    #[account(mut)]
    pub from: Account<'info, UserState>, // Missing #[account(signer)]
    #[account(signer)]
    pub authority: Signer<'info>, // Has signer
    #[account(mut)]
    pub to: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}

// Account structs for safe cases
#[derive(Accounts)]
pub struct TransferWithSigner<'info> {
    #[account(mut, signer)]
    pub from: Account<'info, UserState>, // Has #[account(signer)]
    #[account(mut)]
    pub to: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct TestAccounts<'info> {
    pub from: AccountInfo<'info>,
    pub mint: AccountInfo<'info>,
    pub to: AccountInfo<'info>,
    pub authority: AccountInfo<'info>,
    pub token_program: Program<'info, Token2022>,
}

#[derive(Accounts)]
pub struct TransferCheckedCpiWithSigner<'info> {
    pub from: AccountInfo<'info>,
    pub mint: AccountInfo<'info>,
    pub to: AccountInfo<'info>,
    #[account(signer)]
    pub authority: Signer<'info>,
    pub token_program: Program<'info, Token2022>,
}
#[derive(Accounts)]
pub struct CreateAtaMissingSigner<'info> {
    /// FAIL: missing #[account(signer)]
    pub payer: AccountInfo<'info>,

    #[account(mut)]
    pub ata: AccountInfo<'info>,

    pub authority: AccountInfo<'info>,
    pub mint: AccountInfo<'info>,

    pub token_program: Program<'info, anchor_spl::token::Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreateAtaWithSigner<'info> {
    /// OK: has signer
    #[account(signer)]
    pub payer: AccountInfo<'info>,

    #[account(mut)]
    pub ata: AccountInfo<'info>,
    #[account(signer)]
    pub authority: AccountInfo<'info>,
    pub mint: AccountInfo<'info>,

    pub token_program: Program<'info, anchor_spl::token::Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct MintToMissingSigner<'info> {
    #[account(mut)]
    pub mint: Account<'info, Mint>,

    #[account(mut)]
    pub to: Account<'info, TokenAccount>,

    pub mint_authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct MintToWithSigner<'info> {
    #[account(mut)]
    pub mint: Account<'info, Mint>,

    #[account(mut)]
    pub to: Account<'info, TokenAccount>,

    #[account(signer)]
    pub mint_authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct BurnMissingSigner<'info> {
    #[account(mut)]
    pub mint: Account<'info, Mint>,

    #[account(mut)]
    pub from: Account<'info, TokenAccount>,

    pub authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}
#[derive(Accounts)]
pub struct BurnWithSigner<'info> {
    #[account(mut)]
    pub mint: Account<'info, Mint>,

    #[account(mut)]
    pub from: Account<'info, TokenAccount>,

    pub authority: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct SetAuthorityMissingSigner<'info> {
    #[account(mut)]
    pub account: Account<'info, TokenAccount>,

    pub current_authority: AccountInfo<'info>,

    /// CHECK: just a pubkey
    pub new_authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct MintToWithPdaSigner<'info> {
    #[account(
        init,
        payer = payer,
        mint::decimals = 6,
        mint::authority = mint_authority
    )]
    pub mint: Account<'info, Mint>,

    /// CHECK: PDA used as mint authority
    #[account(seeds = [b"mint_authority", mint.key().as_ref()], bump)]
    pub mint_authority: AccountInfo<'info>,

    #[account(mut)]
    pub user_token: Account<'info, TokenAccount>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct CloseAccountMissingSigner<'info> {
    #[account(mut)]
    pub account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub destination: AccountInfo<'info>,

    pub authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct CloseAccountWithSigner<'info> {
    #[account(mut)]
    pub account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub destination: AccountInfo<'info>,

    #[account(signer)]
    pub authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct FreezeAccountMissingSigner<'info> {
    #[account(mut)]
    pub account: Account<'info, TokenAccount>,

    pub mint: Account<'info, Mint>,

    pub authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct FreezeAccountWithSigner<'info> {
    #[account(mut)]
    pub account: Account<'info, TokenAccount>,

    pub mint: Account<'info, Mint>,

    #[account(signer)]
    pub authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ThawAccountMissingSigner<'info> {
    #[account(mut)]
    pub account: Account<'info, TokenAccount>,

    pub mint: Account<'info, Mint>,

    pub authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ThawAccountWithSigner<'info> {
    #[account(mut)]
    pub account: Account<'info, TokenAccount>,

    pub mint: Account<'info, Mint>,

    #[account(signer)]
    pub authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ApproveMissingSigner<'info> {
    #[account(mut)]
    pub to: Account<'info, TokenAccount>,

    pub delegate: AccountInfo<'info>,

    pub authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ApproveWithSigner<'info> {
    #[account(mut)]
    pub to: Account<'info, TokenAccount>,

    pub delegate: AccountInfo<'info>,

    #[account(signer)]
    pub authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct RevokeMissingSigner<'info> {
    #[account(mut)]
    pub source: Account<'info, TokenAccount>,

    pub authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct RevokeWithSigner<'info> {
    #[account(mut)]
    pub source: Account<'info, TokenAccount>,

    #[account(signer)]
    pub authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct SyncNativeMissingSigner<'info> {
    #[account(mut)]
    pub account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct SyncNativeWithSigner<'info> {
    #[account(mut)]
    pub account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Token2022MintToCheckedMissingSigner<'info> {
    #[account(mut)]
    pub mint: AccountInfo<'info>,

    #[account(mut)]
    pub to: AccountInfo<'info>,

    pub mint_authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token2022>,
}

#[derive(Accounts)]
pub struct Token2022MintToCheckedWithSigner<'info> {
    #[account(mut)]
    pub mint: AccountInfo<'info>,

    #[account(mut)]
    pub to: AccountInfo<'info>,

    #[account(signer)]
    pub mint_authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token2022>,
}

#[derive(Accounts)]
pub struct Token2022BurnCheckedMissingSigner<'info> {
    #[account(mut)]
    pub from: AccountInfo<'info>,

    #[account(mut)]
    pub mint: AccountInfo<'info>,

    pub authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token2022>,
}

#[derive(Accounts)]
pub struct Token2022BurnCheckedWithSigner<'info> {
    #[account(mut)]
    pub from: AccountInfo<'info>,

    #[account(mut)]
    pub mint: AccountInfo<'info>,

    #[account(signer)]
    pub authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token2022>,
}

// State account
#[account]
pub struct UserState {
    pub balance: u64,
}

#[error_code]
pub enum CustomError {
    #[msg("Not signer")]
    NotSigner,
}
