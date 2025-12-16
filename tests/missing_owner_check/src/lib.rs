use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use mpl_token_metadata::accounts::TokenRecord;

declare_id!("11111111111111111111111111111111");

// Simple metadata struct for testing deserialization
#[derive(Clone, Debug, PartialEq)]
pub struct Metadata {
    pub name: String,
    pub symbol: String,
}

impl Metadata {
    pub fn safe_deserialize(data: &mut [u8]) -> Result<Self> {
        // Simplified deserialization for testing
        if data.len() < 2 {
            return Err(anchor_lang::error::ErrorCode::ConstraintOwner.into());
        }
        Ok(Metadata {
            name: "Test".to_string(),
            symbol: "TST".to_string(),
        })
    }
}

#[program]
pub mod missing_owner_check_tests {
    use super::*;

    // Test Case 1: UncheckedAccount with data access (deserialization) - should trigger lint
    pub fn process_metadata(ctx: Context<ProcessMetadata>) -> Result<()> {
        // Data is deserialized without owner validation
        let metadata = Metadata::safe_deserialize(
            &mut ctx.accounts.metadata.to_account_info().data.borrow_mut()
        )?;
        msg!("Metadata: {:?}", metadata);
        Ok(())
    }

    // Test Case 2: UncheckedAccount with owner constraint - should NOT trigger
    pub fn process_metadata_with_owner(ctx: Context<ProcessMetadataWithOwner>) -> Result<()> {
        // Owner is validated via constraint
        let metadata = Metadata::safe_deserialize(
            &mut ctx.accounts.metadata.to_account_info().data.borrow_mut()
        )?;
        msg!("Metadata: {:?}", metadata);
        Ok(())
    }

    // Test Case 3: UncheckedAccount with seeds constraint (PDA) - should NOT trigger
    pub fn process_pda_metadata(ctx: Context<ProcessPdaMetadata>) -> Result<()> {
        // PDA validation via seeds
        let metadata = Metadata::safe_deserialize(
            &mut ctx.accounts.metadata.to_account_info().data.borrow_mut()
        )?;
        msg!("Metadata: {:?}", metadata);
        Ok(())
    }

    // Test Case 4: AccountInfo with data access - should trigger lint
    pub fn process_account_info(ctx: Context<ProcessAccountInfo>) -> Result<()> {
        // AccountInfo data is accessed
        let account_info = ctx.accounts.user_data.to_account_info();
        let data = account_info.data.borrow();
        msg!("Data length: {}", data.len());
        Ok(())
    }

    // Test Case 5: Account type - should NOT trigger (automatic validation)
    pub fn process_account_type(ctx: Context<ProcessAccountType>) -> Result<()> {
        // Account<'info, T> has automatic owner validation
        let account = &ctx.accounts.user_account;
        msg!("Account value: {}", account.value);
        Ok(())
    }

    // Test Case 6: UncheckedAccount only used for key - should NOT trigger
    pub fn process_key_only(ctx: Context<ProcessKeyOnly>) -> Result<()> {
        // Only using .key(), not accessing data
        let key = ctx.accounts.metadata.key();
        msg!("Key: {}", key);
        Ok(())
    }

    // Test Case 7: Program account used only as CPI target - should NOT trigger
    pub fn process_program_cpi_only(ctx: Context<ProcessProgramCpiOnly>) -> Result<()> {
        // Program account used only as CPI target, no data access
        anchor_spl::token::transfer(
            CpiContext::new(
                ctx.accounts.token_metadata_program.key(), // Only used as program ID
                anchor_spl::token::Transfer {
                    from: ctx.accounts.source.to_account_info(),
                    to: ctx.accounts.dest.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
            100,
        )?;
        Ok(())
    }

    // Test Case 7: Program account used only as CPI target - should NOT trigger
    pub fn process_program_cpi_only_with_builder(ctx: Context<ProcessProgramCpiOnlyWithBuilder>) -> Result<()> {
        // Program account used only as CPI target, no data access
        // Using CPI builder pattern like in real-world code
        use mpl_token_metadata::instructions::DelegateStakingV1CpiBuilder;
        
        DelegateStakingV1CpiBuilder::new(&ctx.accounts.token_metadata_program)
            .delegate(&ctx.accounts.authority.to_account_info())
            .metadata(&ctx.accounts.metadata.to_account_info())
            .mint(&ctx.accounts.mint.to_account_info())
            .token(&ctx.accounts.token.to_account_info())
            .authority(&ctx.accounts.authority)
            .payer(&ctx.accounts.authority)
            .system_program(&ctx.accounts.system_program)
            .invoke()?;
        
        Ok(())
    }

    // Test Case 8: AccountInfo with address constraint - should NOT trigger
    pub fn process_address_constraint(ctx: Context<ProcessAddressConstraint>) -> Result<()> {
        // Address constraint validates it, so no owner check needed
        let account_info = ctx.accounts.sysvar.to_account_info();
        let data = account_info.data.borrow();
        msg!("Data length: {}", data.len());
        Ok(())
    }

    // Test Case 9: UncheckedAccount passed to CPI with data access - should trigger
    pub fn process_cpi_with_data_access(ctx: Context<ProcessCpiWithDataAccess>) -> Result<()> {
        // Data accessed before CPI
        let account_info = ctx.accounts.token_account.to_account_info();
        let data = account_info.data.borrow();
        msg!("Token account data length: {}", data.len());
        
        // Then passed to CPI (but data was already accessed)
        anchor_spl::token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                anchor_spl::token::Transfer {
                    from: ctx.accounts.token_account.to_account_info(),
                    to: ctx.accounts.destination.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
            100,
        )?;
        Ok(())
    }

    // Test Case 10: AccountInfo only used for key - should NOT trigger
    pub fn process_account_info_key_only(ctx: Context<ProcessAccountInfoKeyOnly>) -> Result<()> {
        // Only using .key(), not accessing data
        let key = ctx.accounts.metadata.key(); 
        msg!("Key: {}", key);
        Ok(())
    }

    // Test Case 11: AccountInfo with has_one constraint
    pub fn process_has_one(ctx: Context<ProcessHasOne>) -> Result<()> {
        let binding = ctx.accounts.state.to_account_info();
        let data = binding.data.borrow();
        msg!("len {}", data.len());
        Ok(())
    }

    // Test Case 12: AccountInfo with seeds constraint (PDA) and authority
    pub fn process_pda_authority(ctx: Context<ProcessPdaAuthority>) -> Result<()> {
        let binding = ctx.accounts.pda.to_account_info();
        let data = binding.data.borrow();
        msg!("len {}", data.len());
        Ok(())
    }

    // Test Case 13: Post-CPI read access - should trigger lint
    pub fn process_post_cpi_read(ctx: Context<ProcessPostCpiRead>) -> Result<()> {
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.source.to_account_info(),
                    to: ctx.accounts.dest.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
            1,
        )?;
    
        // CPI does NOT validate ownership for reads
        let binding = ctx.accounts.source.to_account_info();
        let data = binding.data.borrow();
        msg!("len {}", data.len());
        Ok(())
    }
    pub fn read(ctx: Context<ReadMeta>) -> Result<()> {
        // reading data without owner validation
        let meta = Metadata::safe_deserialize(
            &mut ctx.accounts.metadata.to_account_info().data.borrow_mut()
        )?;
        msg!("meta: {:?}", meta);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct ReadMeta<'info> {
    // no owner check
    pub metadata: UncheckedAccount<'info>, // [missing_owner_check]
}

// Test Case 1: Missing owner check
#[derive(Accounts)]
pub struct ProcessMetadata<'info> {
    /// CHECK: Metadata will be deserialized
    pub metadata: UncheckedAccount<'info>, // [missing_owner_check]
}

// Test Case 2: Has owner constraint
#[derive(Accounts)]
pub struct ProcessMetadataWithOwner<'info> {
    #[account(owner = anchor_spl::token::ID)]
    pub metadata: UncheckedAccount<'info>, // [safe_owner_check]
}

// Test Case 3: Has seeds constraint (PDA)
#[derive(Accounts)]
pub struct ProcessPdaMetadata<'info> {
    #[account(seeds = [b"metadata"], bump)]
    pub metadata: UncheckedAccount<'info>, // [safe_owner_check]
}

// Test Case 4: AccountInfo with data access
#[derive(Accounts)]
pub struct ProcessAccountInfo<'info> {
    /// CHECK: User data will be accessed
    pub user_data: AccountInfo<'info>, // [missing_owner_check]
}

// Test Case 5: Account type (automatic validation)
#[derive(Accounts)]
pub struct ProcessAccountType<'info> {
    pub user_account: Account<'info, UserAccount>, // [safe_owner_check]
}

// Test Case 6: Only using key
#[derive(Accounts)]
pub struct ProcessKeyOnly<'info> {
    /// CHECK: Only using key, not data
    pub metadata: UncheckedAccount<'info>, // [safe_owner_check]
}

// Test Case 7: Program account used only as CPI target
#[derive(Accounts)]
pub struct ProcessProgramCpiOnly<'info> {
    /// CHECK: Program account used only as CPI target, no data access
    pub token_metadata_program: UncheckedAccount<'info>, // [safe_owner_check]
    pub source: Account<'info, TokenAccount>,
    pub dest: Account<'info, TokenAccount>,
    #[account(signer)]
    pub authority: Signer<'info>,
}


// Test Case 7: Program account used only as CPI target
#[derive(Accounts)]
pub struct ProcessProgramCpiOnlyWithBuilder<'info> {
    /// CHECK: Program account used only as CPI target, no data access
    pub token_metadata_program: UncheckedAccount<'info>, // [safe_owner_check]
    pub authority: Signer<'info>,
    pub metadata: AccountInfo<'info>,
    pub mint: AccountInfo<'info>,
    pub token: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}


// Test Case 8: AccountInfo with address constraint
#[derive(Accounts)]
pub struct ProcessAddressConstraint<'info> {
    #[account(address = anchor_lang::solana_program::sysvar::rent::ID)]
    pub sysvar: AccountInfo<'info>, // [safe_owner_check]
}

// Test Case 9: UncheckedAccount passed to CPI with data access
#[derive(Accounts)]
pub struct ProcessCpiWithDataAccess<'info> {
    /// CHECK: Token account data will be accessed
    pub token_account: UncheckedAccount<'info>, // [missing_owner_check]
    pub destination: Account<'info, TokenAccount>,
    #[account(signer)]
    pub authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

// Test Case 10: AccountInfo only used for key
#[derive(Accounts)]
pub struct ProcessAccountInfoKeyOnly<'info> {
    /// CHECK: Only using key, not data
    pub metadata: AccountInfo<'info>, // [safe_owner_check]
}

// Test Case 11: AccountInfo with has_one constraint
#[derive(Accounts)]
pub struct ProcessHasOne<'info> {
    #[account(has_one = authority)]
    pub state: Account<'info, State>, // [safe_owner_check]
    pub authority: Signer<'info>,
}

// Test Case 12: AccountInfo with seeds constraint (PDA) and authority
#[derive(Accounts)]
pub struct ProcessPdaAuthority<'info> {
    #[account(
        seeds = [b"authority", user.key().as_ref()],
        bump
    )]
    pub pda: UncheckedAccount<'info>, // [safe_owner_check]
    pub user: Signer<'info>,
}

// Test Case 13: Post-CPI read access - should trigger lint
#[derive(Accounts)]
pub struct ProcessPostCpiRead<'info> {
    pub source: UncheckedAccount<'info>, // [missing_owner_check]
    pub dest: Account<'info, TokenAccount>,
    pub authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[account]
pub struct State {
    pub data: u64,
    pub authority: Pubkey,
}

#[account]
pub struct UserAccount {
    pub value: u64,
}

