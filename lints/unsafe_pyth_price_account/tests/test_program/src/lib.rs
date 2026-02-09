use anchor_lang::prelude::*;
use pyth_solana_receiver_sdk::price_update::{FeedId, PriceUpdateV2, get_feed_id_from_hex};

declare_id!("Test111111111111111111111111111111111111111");

// In real code, this would be a constant canonical feed address
const CANONICAL_FEED_ADDRESS: Pubkey = anchor_lang::pubkey!("11111111111111111111111111111111");

pub const MAXIMUM_AGE: u64 = 60; // One minute
pub const FEED_ID: &str = "0xef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d"; // SOL/USD price feed id from https://pyth.network/developers/price-feed-ids

pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
    let price_update = &mut ctx.accounts.price_update;
    let feed_id = get_feed_id_from_hex(FEED_ID).unwrap();
    let _price = price_update.get_price_no_older_than( // [unsafe_account_accessed]
        &Clock::get()?,
        MAXIMUM_AGE,
        &feed_id,
    )?;
    /// Do something with the price
    Ok(())
}

pub fn bad_price_usage(ctx: Context<BadPriceUsage>) -> Result<()> {
    // unsafe_account_accessed
    let feed_id = get_feed_id_from_hex(FEED_ID).unwrap();
    let price = ctx.accounts.price_account.get_price_no_older_than(  // [unsafe_account_accessed]
        &ctx.accounts.clock,
        60,
        &feed_id,
    )?;
    msg!("Price: {}", price.price);
    Ok(())
}

pub fn good_price_usage_with_key_check(ctx: Context<GoodPriceUsageWithKeyCheck>) -> Result<()> {
    // Check against canonical feed address
    require_keys_eq!(ctx.accounts.price_account.key(), CANONICAL_FEED_ADDRESS);
    // safe_account_accessed
    let feed_id = get_feed_id_from_hex(FEED_ID).unwrap();
    let price = ctx.accounts.price_account.get_price_no_older_than(  // [safe_account_accessed]
        &ctx.accounts.clock,
        60,
        &feed_id,
    )?;
    msg!("Price: {}", price.price);
    Ok(())
}

pub fn good_price_usage_with_monotonic_time(
    ctx: Context<GoodPriceUsageWithMonotonicTime>,
) -> Result<()> {
    // safe_account_accessed
    let feed_id = get_feed_id_from_hex(FEED_ID).unwrap();
    let price =
        ctx.accounts
            .price_account
            .get_price_no_older_than(&ctx.accounts.clock, 60, &feed_id); // [safe_account_accessed]

    // Enforce monotonic publish time
    require!(
        ctx.accounts.price_account.price_message.publish_time > ctx.accounts.state.last_publish_time,
        ErrorCode::StalePrice
    );

    ctx.accounts.state.last_publish_time = ctx.accounts.price_account.price_message.publish_time;

    Ok(())
}

pub fn good_pda_price_account(ctx: Context<GoodPdaPriceAccount>) -> Result<()> {
    // safe_account_accessed (PDA, so should be skipped by lint)
    let feed_id = get_feed_id_from_hex(FEED_ID).unwrap();
    let price =
        ctx.accounts
            .price_account
            .get_price_no_older_than(&ctx.accounts.clock, 60, &feed_id); // [safe_account_accessed]

    Ok(())
}

pub fn good_unchecked_account_with_key_check(
    ctx: Context<GoodUncheckedAccountWithKeyCheck>,
) -> Result<()> {
    // Check against canonical feed address
    require_keys_eq!(ctx.accounts.price_account.key(), CANONICAL_FEED_ADDRESS);
    // In real code, would deserialize and use PriceUpdateV2
    // safe_account_accessed
    Ok(())
}

pub fn bad_replayable_price_usage(ctx: Context<BadReplayablePriceUsage>) -> Result<()> {
    let feed_id = get_feed_id_from_hex(FEED_ID).unwrap();

    // Attacker can submit t2 first, then replay t1 (still within max_age)
    let price = ctx.accounts.price_account.get_price_no_older_than( // [unsafe_account_accessed]
        &ctx.accounts.clock,
        MAXIMUM_AGE,
        &feed_id,
    )?;

    msg!(
        "Replayable price accepted at publish_time={}",
        ctx.accounts.price_account.price_message.publish_time
    );

    Ok(())
}

// Test case with two PriceUpdateV2 accounts: one safe, one unsafe
pub fn mixed_price_accounts_usage(ctx: Context<MixedPriceAccounts>) -> Result<()> {
    let feed_id = get_feed_id_from_hex(FEED_ID).unwrap();

    // Safe: canonical_price_account has key validation
    require_keys_eq!(
        ctx.accounts.canonical_price_account.key(),
        CANONICAL_FEED_ADDRESS
    );
    let safe_price = ctx
        .accounts
        .canonical_price_account
        .get_price_no_older_than( // [safe_account_accessed]
            &ctx.accounts.clock,
            MAXIMUM_AGE,
            &feed_id,
        )?;

    // Unsafe: user_price_account has no validation
    let unsafe_price = ctx.accounts.user_price_account.get_price_no_older_than( // [unsafe_account_accessed]
        &ctx.accounts.clock,
        MAXIMUM_AGE,
        &feed_id,
    )?;

    msg!(
        "Safe price: {}, Unsafe price: {}",
        safe_price.price,
        unsafe_price.price
    );
    Ok(())
}

// case: Only feed_id validation, no pubkey check or monotonic time
pub fn vulnerable_feed_id_only_validation(ctx: Context<VulnerableFeedIdOnly>) -> Result<()> {
    let feed_id = get_feed_id_from_hex(FEED_ID).unwrap();
    let price = ctx.accounts.price_account.get_price_no_older_than( // [unsafe_account_accessed]
        &ctx.accounts.clock,
        MAXIMUM_AGE,
        &feed_id,
    )?;
    
    msg!("Price: {}", price.price);
    Ok(())
}

// case: Incomplete monotonicity: stores publish_time but doesn't check it
pub fn incomplete_monotonicity_store_only(
    ctx: Context<IncompleteMonotonicityStoreOnly>,
) -> Result<()> {
    let feed_id = get_feed_id_from_hex(FEED_ID).unwrap();
    let price = ctx.accounts.price_account.get_price_no_older_than( // [unsafe_account_accessed]
        &ctx.accounts.clock,
        MAXIMUM_AGE,
        &feed_id,
    )?;
    
    ctx.accounts.state.last_publish_time = ctx.accounts.price_account.price_message.publish_time;
    
    msg!("Price: {}", price.price);
    Ok(())
}

// Attack scenario: Replay attack - accepts older price after newer one
pub fn replay_attack_scenario(ctx: Context<ReplayAttackScenario>) -> Result<()> {
    let feed_id = get_feed_id_from_hex(FEED_ID).unwrap();
    

    // 1. Submit transaction with price at time t₂ (newer)
    // 2. Submit transaction with price at time t₁ (older, but still within max_age)
    // Both pass max_age check, but t₁ < t₂, so stale price is accepted
    let price = ctx.accounts.price_account.get_price_no_older_than( // [unsafe_account_accessed]
        &ctx.accounts.clock,
        MAXIMUM_AGE,
        &feed_id,
    )?;
    
    msg!(
        "Accepted price with publish_time={}",
        ctx.accounts.price_account.price_message.publish_time
    );
    Ok(())
}


#[derive(Accounts)]
#[instruction(amount_in_usd : u64)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    pub price_update: Account<'info, PriceUpdateV2>,
    // Add more accounts here
}

// Bad: Using PriceUpdateV2 without canonical source validation or monotonic publish time
#[derive(Accounts)]
pub struct BadPriceUsage<'info> {
    pub price_account: Account<'info, PriceUpdateV2>,
    pub clock: Sysvar<'info, Clock>,
}

// Good: Using PriceUpdateV2 with canonical source validation
#[derive(Accounts)]
pub struct GoodPriceUsageWithKeyCheck<'info> {
    pub price_account: Account<'info, PriceUpdateV2>,
    pub clock: Sysvar<'info, Clock>,
}

// Good: Using PriceUpdateV2 with monotonic publish time enforcement
#[derive(Accounts)]
pub struct GoodPriceUsageWithMonotonicTime<'info> {
    pub price_account: Account<'info, PriceUpdateV2>, // [safe_account_accessed]
    pub state: Account<'info, PriceState>,
    pub clock: Sysvar<'info, Clock>,
}

#[account]
pub struct PriceState {
    pub last_publish_time: i64,
}

// Good: PDA-derived price account (should be skipped)
#[derive(Accounts)]
pub struct GoodPdaPriceAccount<'info> {
    #[account(
        seeds = [b"price", b"feed"],
        bump
    )]
    pub price_account: Account<'info, PriceUpdateV2>,
    pub clock: Sysvar<'info, Clock>,
}

// Good: UncheckedAccount with key check
#[derive(Accounts)]
pub struct GoodUncheckedAccountWithKeyCheck<'info> {
    pub price_account: UncheckedAccount<'info>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct BadReplayablePriceUsage<'info> {
    pub price_account: Account<'info, PriceUpdateV2>,
    pub clock: Sysvar<'info, Clock>,
}

// Test case with two PriceUpdateV2 accounts: one safe (with key check), one unsafe (no validation)
#[derive(Accounts)]
pub struct MixedPriceAccounts<'info> {
    pub canonical_price_account: Account<'info, PriceUpdateV2>,
    pub user_price_account: Account<'info, PriceUpdateV2>,
    pub clock: Sysvar<'info, Clock>,
}

impl MixedPriceAccounts<'_> {
    pub fn unsafe_price_account(&self) -> Result<()> {
        let feed_id = get_feed_id_from_hex(FEED_ID).unwrap();

        // Safe: canonical_price_account has key validation
        require_keys_eq!(self.canonical_price_account.key(), CANONICAL_FEED_ADDRESS);
        let safe_price = self.canonical_price_account.get_price_no_older_than(
            // [safe_account_accessed]
            &self.clock,
            MAXIMUM_AGE,
            &feed_id,
        )?;

        // Unsafe: user_price_account has no validation
        let unsafe_price = self.user_price_account.get_price_no_older_than( // [unsafe_account_accessed]
            &self.clock,
            MAXIMUM_AGE,
            &feed_id,
        )?;

        msg!(
            "Safe price: {}, Unsafe price: {}",
            safe_price.price,
            unsafe_price.price
        );
        Ok(())
    }
    pub fn safe_price_account(&self) -> Result<()> {
        let feed_id = get_feed_id_from_hex(FEED_ID).unwrap();

        // Safe: canonical_price_account has key validation
        require_keys_eq!(self.canonical_price_account.key(), CANONICAL_FEED_ADDRESS);
        let safe_price = self.canonical_price_account.get_price_no_older_than(
            // [safe_account_accessed]
            &self.clock,
            MAXIMUM_AGE,
            &feed_id,
        )?;

        require_keys_eq!(self.user_price_account.key(), CANONICAL_FEED_ADDRESS);
        let unsafe_price = self.user_price_account.get_price_no_older_than(
            // [safe_account_accessed]
            &self.clock,
            MAXIMUM_AGE,
            &feed_id,
        )?;

        msg!(
            "Safe price: {}, Unsafe price: {}",
            safe_price.price,
            unsafe_price.price
        );
        Ok(())
    }
}

// Vulnerable: Only feed_id validation, no pubkey check
#[derive(Accounts)]
pub struct VulnerableFeedIdOnly<'info> {
    pub price_account: Account<'info, PriceUpdateV2>,
    pub clock: Sysvar<'info, Clock>,
}

// Incomplete: Stores publish_time but doesn't check it
#[derive(Accounts)]
pub struct IncompleteMonotonicityStoreOnly<'info> {
    pub price_account: Account<'info, PriceUpdateV2>,
    pub state: Account<'info, PriceState>,
    pub clock: Sysvar<'info, Clock>,
}

// Attack scenario: Replay attack vulnerability
#[derive(Accounts)]
pub struct ReplayAttackScenario<'info> {
    pub price_account: Account<'info, PriceUpdateV2>,
    pub clock: Sysvar<'info, Clock>,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Stale price")]
    StalePrice,
}
