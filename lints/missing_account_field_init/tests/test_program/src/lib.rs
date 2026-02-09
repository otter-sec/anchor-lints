use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount};

declare_id!("11111111111111111111111111111111");

#[account]
pub struct Collection {
    pub max_collectable_tokens: u64,
    pub authority: Pubkey,
    pub lifetime_tokens_collected: u64,
}

#[account]
pub struct UserProfile {
    pub owner: Pubkey,
    pub display_name: String,
    pub level: u64,
}

#[program]
pub mod missing_account_field_init_tests {
    use super::*;

    // BAD: forgets to initialize `max_collectable_tokens`
    pub fn init_collection_incomplete(
        ctx: Context<InitCollectionIncomplete>,
        max_collectable_tokens: u64,
    ) -> Result<()> {
        let collection_a = &mut ctx.accounts.collection;
        collection_a.max_collectable_tokens = 1;
        collection_a.lifetime_tokens_collected = 0;
        // max_collectable_tokens is never written
        let _ = max_collectable_tokens;
        Ok(())
    }

    // GOOD: initializes all fields
    pub fn init_collection_complete(
        ctx: Context<InitCollectionComplete>,
        max_collectable_tokens: u64,
    ) -> Result<()> {
        let collection = &mut ctx.accounts.collection;
        collection.authority = ctx.accounts.authority.key();
        collection.lifetime_tokens_collected = 0;
        collection.max_collectable_tokens = max_collectable_tokens;
        Ok(())
    }

    // BAD: initializes no fields
    pub fn init_collection_empty(
        ctx: Context<InitCollectionEmpty>,
        _max_collectable_tokens: u64,
    ) -> Result<()> {
        let _collection = &mut ctx.accounts.collection;
        // No fields are initialized
        Ok(())
    }

    // GOOD: direct field access without intermediate variable
    pub fn init_collection_direct(
        ctx: Context<InitCollectionDirect>,
        max_collectable_tokens: u64,
    ) -> Result<()> {
        ctx.accounts.collection.authority = ctx.accounts.authority.key();
        ctx.accounts.collection.lifetime_tokens_collected = 0;
        ctx.accounts.collection.max_collectable_tokens = max_collectable_tokens;
        Ok(())
    }

    // GOOD: mixed initialization (reference + direct access)
    pub fn init_collection_mixed(
        ctx: Context<InitCollectionMixed>,
        max_collectable_tokens: u64,
    ) -> Result<()> {
        let collection = &mut ctx.accounts.collection;
        collection.authority = ctx.accounts.authority.key();
        // Mix: use direct access for remaining fields
        ctx.accounts.collection.lifetime_tokens_collected = 0;
        ctx.accounts.collection.max_collectable_tokens = max_collectable_tokens;
        Ok(())
    }

    // BAD: two Collection accounts, only one is fully initialized
    pub fn init_two_collections_incomplete(
        ctx: Context<InitTwoCollectionsIncomplete>,
        max_collectable_tokens_a: u64,
        max_collectable_tokens_b: u64,
    ) -> Result<()> {
        // Fully initialize `collection_a`
        let collection_a = &mut ctx.accounts.collection_a;
        collection_a.authority = ctx.accounts.authority.key();

        // Partially initialize `collection_b` (misses `max_collectable_tokens`)
        let collection_b = &mut ctx.accounts.collection_b;
        collection_b.lifetime_tokens_collected = 0;
        let _ = max_collectable_tokens_b; // never written to account

        Ok(())
    }

    // GOOD: two Collection accounts, both fully initialized
    pub fn init_two_collections_complete(
        ctx: Context<InitTwoCollectionsComplete>,
        max_collectable_tokens_a: u64,
        max_collectable_tokens_b: u64,
    ) -> Result<()> {
        let collection_a = &mut ctx.accounts.collection_a;
        collection_a.authority = ctx.accounts.authority.key();
        collection_a.lifetime_tokens_collected = 0;
        collection_a.max_collectable_tokens = max_collectable_tokens_a;

        let collection_b = &mut ctx.accounts.collection_b;
        collection_b.authority = ctx.accounts.authority.key();
        collection_b.lifetime_tokens_collected = 0;
        collection_b.max_collectable_tokens = max_collectable_tokens_b;

        Ok(())
    }

    // BAD: two different account types, only one fully initialized
    pub fn init_collection_and_profile_incomplete(
        ctx: Context<InitCollectionAndProfileIncomplete>,
        max_collectable_tokens: u64,
        display_name: String,
    ) -> Result<()> {
        // Fully initialize Collection
        let collection = &mut ctx.accounts.collection;
        collection.max_collectable_tokens = max_collectable_tokens;
        collection.authority = ctx.accounts.authority.key();
        collection.lifetime_tokens_collected = 0;

        // Partially initialize UserProfile (forgets `display_name`)
        let profile = &mut ctx.accounts.profile;
        profile.owner = ctx.accounts.authority.key();
        Ok(())
    }

    // GOOD: both different account types fully initialized
    pub fn init_collection_and_profile_complete(
        ctx: Context<InitCollectionAndProfileComplete>,
        max_collectable_tokens: u64,
        display_name: String,
    ) -> Result<()> {
        let collection = &mut ctx.accounts.collection;
        collection.max_collectable_tokens = max_collectable_tokens;
        collection.authority = ctx.accounts.authority.key();
        collection.lifetime_tokens_collected = 0;

        let profile = &mut ctx.accounts.profile;
        profile.owner = ctx.accounts.authority.key();
        profile.display_name = display_name;
        profile.level = 1;
        Ok(())
    }

    // GOOD: initializes all fields via `set_inner`
    pub fn init_collection_via_set_inner(
        ctx: Context<InitCollectionViaSetInner>,
        authority: Pubkey,
        max_collectable_tokens: u64,
    ) -> Result<()> {
        let collection = Collection {
            max_collectable_tokens,
            authority,
            lifetime_tokens_collected: 0,
        };
        ctx.accounts.collection_via_set_inner.set_inner(collection);
        Ok(())
    }

    // BAD: uses self-method that fails to initialize all fields
    pub fn init_collection_via_method_incomplete(
        ctx: Context<InitCollectionViaMethodComplete>,
        max_collectable_tokens: u64,
    ) -> Result<()> {
        init_collection_helper(ctx, max_collectable_tokens)
    }

    pub fn init_collection_with_helper_set_inner(
        mut ctx: Context<InitCollectionWithHelperSetInner>,
        max_collectable_tokens: u64,
    ) -> Result<()> {
        let accounts = &mut ctx.accounts;
        accounts.init_via_set_inner(max_collectable_tokens);
        Ok(())
    }

    pub fn handle_test_create_vesting_escrow(
        ctx: Context<TestCreateVestingEscrowCtx>,
        params: TestCreateEscrowParams,
    ) -> Result<()> {
        let escrow = &mut ctx.accounts.escrow;
        // escrow.init(
        //     params.vesting_start_time,
        //     params.cliff_time,
        //     params.frequency,
        //     params.cliff_unlock_amount,
        //     params.amount_per_period,
        //     params.number_of_period,
        //     ctx.accounts.recipient.key(),
        //     ctx.accounts.sender_token.mint,
        //     ctx.accounts.sender.key(),
        //     ctx.accounts.base.key(),
        //     ctx.bumps.escrow,
        //     1,
        //     1,
        //     1,
        // );
        params.init_escrow(
            escrow,
            ctx.accounts.recipient.key(),
            ctx.accounts.sender_token.mint,
            ctx.accounts.sender.key(),
            ctx.accounts.base.key(),
            ctx.bumps.escrow,
            1,
        )?;
        Ok(())
    }

    // GOOD: primitive types and padding fields should be ignored
    pub fn init_account_with_primitives_and_padding(
        ctx: Context<InitAccountWithPrimitivesAndPadding>,
        authority: Pubkey,
    ) -> Result<()> {
        // Only initialize the non-primitive field (authority)
        // Primitive fields (total_claimed_amount: u64, cancelled_at: u64, buffer: [u8; 32])
        // and padding fields (padding_0, padding_1, _padding) should be ignored
        ctx.accounts.account.authority = authority;
        Ok(())
    }

    // BAD: non-primitive field (authority: Pubkey) is not initialized
    pub fn init_account_missing_non_primitive(
        ctx: Context<InitAccountMissingNonPrimitive>,
        _authority: Pubkey,
    ) -> Result<()> {
        // Missing authority initialization - should trigger warning
        Ok(())
    }
}

pub fn init_collection_helper(
    ctx: Context<InitCollectionViaMethodComplete>,
    max_collectable_tokens: u64,
) -> Result<()> {
    let collection = &mut ctx.accounts.collection;
    collection.authority = ctx.accounts.authority.key();
    collection.lifetime_tokens_collected = 0;
    collection.max_collectable_tokens = max_collectable_tokens;
    // collection.set_inner(Collection {
    //     max_collectable_tokens,
    //     authority: ctx.accounts.authority.key(),
    //     lifetime_tokens_collected: 0,
    // });
    Ok(())
}

#[derive(Accounts)]
pub struct InitCollectionWithHelperSetInner<'info> {
    #[account(
    init,
    payer = authority,
    space = 8 + 8 + 32 + 8,
    seeds = [b"collection_helper_set_inner", authority.key().as_ref()],
    bump
)]
    pub collection: Account<'info, Collection>, // [safe_account_field_init]
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

impl<'info> InitCollectionWithHelperSetInner<'info> {
    // GOOD: nested helper, uses `set_inner` to initialize all fields
    pub fn init_via_set_inner(&mut self, max_collectable_tokens: u64) {
        // self.collection.set_inner(Collection {
        //     max_collectable_tokens,
        //     authority: self.authority.key(),
        //     lifetime_tokens_collected: 0,
        // });
        self.collection.authority = self.authority.key();
        self.collection.lifetime_tokens_collected = 0;
        self.collection.max_collectable_tokens = max_collectable_tokens;
    }

    pub fn validate_collection(&mut self) -> Result<()> {
        if self.collection.max_collectable_tokens == 0 {
            self.collection.max_collectable_tokens = 100;
        }
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitCollectionIncomplete<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8 + 8,
        seeds = [b"collection", authority.key().as_ref()],
        bump
    )]
    pub collection: Account<'info, Collection>, // [missing_account_field_init]
}

#[derive(Accounts)]
pub struct InitCollectionComplete<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8 + 8,
        seeds = [b"collection", authority.key().as_ref()],
        bump
    )]
    pub collection: Account<'info, Collection>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitCollectionEmpty<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8 + 8,
        seeds = [b"collection_empty", authority.key().as_ref()],
        bump
    )]
    pub collection: Account<'info, Collection>, // [missing_account_field_init]
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitCollectionDirect<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8 + 8,
        seeds = [b"collection_direct", authority.key().as_ref()],
        bump
    )]
    pub collection: Account<'info, Collection>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitCollectionMixed<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8 + 8,
        seeds = [b"collection_mixed", authority.key().as_ref()],
        bump
    )]
    pub collection: Account<'info, Collection>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitTwoCollectionsIncomplete<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8 + 8,
        seeds = [b"collection_a", authority.key().as_ref()],
        bump
    )]
    pub collection_a: Account<'info, Collection>,
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8 + 8,
        seeds = [b"collection_b", authority.key().as_ref()],
        bump
    )]
    pub collection_b: Account<'info, Collection>, // [missing_account_field_init]
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitTwoCollectionsComplete<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8 + 8,
        seeds = [b"collection_a_complete", authority.key().as_ref()],
        bump
    )]
    pub collection_a: Account<'info, Collection>, // [safe_account_field_init]
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8 + 8,
        seeds = [b"collection_b_complete", authority.key().as_ref()],
        bump
    )]
    pub collection_b: Account<'info, Collection>, // [safe_account_field_init]
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitCollectionAndProfileIncomplete<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 8 + 32 + 8,
        seeds = [b"collection_profile_a", authority.key().as_ref()],
        bump
    )]
    pub collection: Account<'info, Collection>, // [safe_account_field_init]
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 4 + 32 + 8, // rough padding for String
        seeds = [b"profile_incomplete", authority.key().as_ref()],
        bump
    )]
    pub profile: Account<'info, UserProfile>, // [missing_account_field_init]
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitCollectionAndProfileComplete<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 8 + 32 + 8,
        seeds = [b"collection_profile_b", authority.key().as_ref()],
        bump
    )]
    pub collection: Account<'info, Collection>, // [safe_account_field_init]
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 4 + 32 + 8,
        seeds = [b"profile_complete", authority.key().as_ref()],
        bump
    )]
    pub profile: Account<'info, UserProfile>, // [safe_account_field_init]
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitCollectionViaMethodComplete<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 8 + 32 + 8,
        seeds = [b"collection_method_complete", authority.key().as_ref()],
        bump
    )]
    pub collection: Account<'info, Collection>, // [safe_account_field_init]
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitCollectionViaSetInner<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 8 + 32 + 8,
        seeds = [b"collection_set_inner", authority.key().as_ref()],
        bump
    )]
    pub collection_via_set_inner: Account<'info, Collection>, // [safe_account_field_init]
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct TestCreateVestingEscrowCtx<'info> {
    #[account(mut)]
    pub base: Signer<'info>,

    #[account(
        init,
        seeds = [b"test_escrow", base.key().as_ref()],
        bump,
        payer = sender,
        space = 8 +  /* size of TestVestingEscrow */
            8 + 8 + 8 + 8 + 8 + 8 + // u64 fields
            32 + 32 + 32 + 32 +     // Pubkeys
            1 + 1 + 1 + 1 +         // u8 fields
            8 + 8                   // extra u64s
    )]
    pub escrow: Account<'info, TestVestingEscrow>, // [safe_account_field_init]

    #[account(mut)]
    pub sender: Signer<'info>,

    #[account(mut)]
    pub sender_token: Account<'info, anchor_spl::token::TokenAccount>,

    /// CHECK: recipient
    pub recipient: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct TestCreateEscrowParams {
    pub vesting_start_time: u64,
    pub cliff_time: u64,
    pub frequency: u64,
    pub cliff_unlock_amount: u64,
    pub amount_per_period: u64,
    pub number_of_period: u64,
    pub update_recipient_mode: u8,
    pub cancel_mode: u8,
}

impl TestCreateEscrowParams {
    pub fn init_escrow(
        &self,
        vesting_escrow: &mut TestVestingEscrow,
        recipient: Pubkey,
        token_mint: Pubkey,
        creator: Pubkey,
        base: Pubkey,
        escrow_bump: u8,
        token_program_flag: u8,
    ) -> Result<()> {
        vesting_escrow.init(
            self.vesting_start_time,
            self.cliff_time,
            self.frequency,
            self.cliff_unlock_amount,
            self.amount_per_period,
            self.number_of_period,
            recipient,
            token_mint,
            creator,
            base,
            escrow_bump,
            self.update_recipient_mode,
            self.cancel_mode,
            token_program_flag,
        );
        Ok(())
    }
}

#[account]
pub struct TestVestingEscrow {
    pub vesting_start_time: u64,
    pub cliff_time: u64,
    pub frequency: u64,
    pub cliff_unlock_amount: u64,
    pub amount_per_period: u64,
    pub number_of_period: u64,
    pub recipient: Pubkey,
    pub token_mint: Pubkey,
    pub creator: Pubkey,
    pub base: Pubkey,
    pub escrow_bump: u8,
    pub update_recipient_mode: u8,
    pub cancel_mode: u8,
    pub token_program_flag: u8,
    pub total_claimed_amount: u64,
    pub cancelled_at: u64,
}

impl TestVestingEscrow {
    pub fn init(
        &mut self,
        vesting_start_time: u64,
        cliff_time: u64,
        frequency: u64,
        cliff_unlock_amount: u64,
        amount_per_period: u64,
        number_of_period: u64,
        recipient: Pubkey,
        token_mint: Pubkey,
        creator: Pubkey,
        base: Pubkey,
        escrow_bump: u8,
        update_recipient_mode: u8,
        cancel_mode: u8,
        token_program_flag: u8,
    ) {
        self.vesting_start_time = vesting_start_time;
        self.cliff_time = cliff_time;
        self.frequency = frequency;
        self.cliff_unlock_amount = cliff_unlock_amount;
        self.amount_per_period = amount_per_period;
        self.number_of_period = number_of_period;
        self.recipient = recipient;
        self.token_mint = token_mint;
        self.creator = creator;
        self.base = base;
        self.escrow_bump = escrow_bump;
        self.update_recipient_mode = update_recipient_mode;
        self.cancel_mode = cancel_mode;
        self.token_program_flag = token_program_flag;
        self.total_claimed_amount = 0;
        self.cancelled_at = 0;
    }
}

#[account]
pub struct AccountWithPrimitivesAndPadding {
    pub authority: Pubkey,         // Non-primitive - should be checked
    pub total_claimed_amount: u64, // Primitive - should be ignored
    pub cancelled_at: u64,         // Primitive - should be ignored
    pub buffer: [u8; 32],          // Array of primitives - should be ignored
    pub padding_0: u64,            // Padding field - should be ignored
    pub padding_1: u8,             // Padding field - should be ignored
    pub _padding: u32,             // Padding field (underscore prefix) - should be ignored
    pub reserved: u64,             // Reserved field - should be ignored
}

#[derive(Accounts)]
pub struct InitAccountWithPrimitivesAndPadding<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8 + 8 + 32 + 8 + 1 + 4 + 8, // rough estimate
        seeds = [b"account_primitives", authority.key().as_ref()],
        bump
    )]
    pub account: Account<'info, AccountWithPrimitivesAndPadding>, // [safe_account_field_init]
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitAccountMissingNonPrimitive<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8 + 8 + 32 + 8 + 1 + 4 + 8,
        seeds = [b"account_missing", authority.key().as_ref()],
        bump
    )]
    pub account: Account<'info, AccountWithPrimitivesAndPadding>, // [missing_account_field_init]
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}
