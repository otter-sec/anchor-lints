use anchor_lang::prelude::*;

declare_id!("11111111111111111111111111111111");

#[program]
pub mod missing_mut_constraint {
    use super::*;

    // Bad: Account (vault) is mutated but not declared with #[account(mut)]
    pub fn update_bad(ctx: Context<UpdateBad>) -> Result<()> {
        ctx.accounts.vault.amount += 1;
        Ok(())
    }

    // Good: Account (vault) has #[account(mut)]
    pub fn update_good(ctx: Context<UpdateGood>) -> Result<()> {
        ctx.accounts.vault.amount += 1;
        Ok(())
    }

    // Good: Account (vault) is only read, no mut needed
    pub fn read_only(ctx: Context<ReadOnly>) -> Result<()> {
        let _ = ctx.accounts.vault.amount;
        Ok(())
    }

    // Bad: Account (treasury) is mutated but not declared with #[account(mut)] (Account (vault) has mut)
    pub fn transfer_bad(ctx: Context<TransferBad>) -> Result<()> {
        ctx.accounts.vault.amount -= 10;
        ctx.accounts.treasury.amount += 10;
        Ok(())
    }

    // Good: lamport mutation with #[account(mut)] on payer
    pub fn pay_lamports(ctx: Context<PayLamports>, amount: u64) -> Result<()> {
        **ctx.accounts.payer.lamports.borrow_mut() -= amount;
        **ctx.accounts.recipient.lamports.borrow_mut() += amount;
        Ok(())
    }
}

#[account]
pub struct Vault {
    pub amount: u64,
}

#[derive(Accounts)]
pub struct UpdateBad<'info> {
    pub vault: Account<'info, Vault>, // [missing_mut_constraint]
}

#[derive(Accounts)]
pub struct UpdateGood<'info> {
    #[account(mut)]
    pub vault: Account<'info, Vault>,
}

#[derive(Accounts)]
pub struct ReadOnly<'info> {
    pub vault: Account<'info, Vault>,
}

#[derive(Accounts)]
pub struct TransferBad<'info> {
    #[account(mut)]
    pub vault: Account<'info, Vault>,
    pub treasury: Account<'info, Vault>, // [missing_mut_constraint]
}

#[derive(Accounts)]
pub struct PayLamports<'info> {
    #[account(mut)]
    pub payer: AccountInfo<'info>,
    #[account(mut)]
    pub recipient: AccountInfo<'info>,
}
