pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("FkubJnmiEvHKY9iyf7jPtLUKZxj8W4F5dXkCQ84a6CaV");

#[program]
pub mod escrowq42026 {
    use super::*;

    #[instruction(discriminator = 0)]
    pub fn make(
        ctx: Context<Make>,
        seed: u64,
        deposit: u64,
        receive: u64,
        expiration: i64,
        taker: Option<Pubkey>,
    ) -> Result<()> {
        ctx.accounts
            .init_escrow(seed, receive, &ctx.bumps, expiration, taker)?;
        ctx.accounts.deposit(deposit)
    }

    #[instruction(discriminator = 1)]
    pub fn take(ctx: Context<Take>) -> Result<()> {
        ctx.accounts.check_conditions()?;
        ctx.accounts.deposit()?;
        ctx.accounts.withdraw_and_close_vault()
    }

    #[instruction(discriminator = 2)]
    pub fn refund(ctx: Context<Refund>) -> Result<()> {
        ctx.accounts.refund_and_close_vault()
    }

    #[instruction(discriminator = 3)]
    pub fn update(
        ctx: Context<Update>,
        receive: u64,
        expiration: i64,
        taker: Option<Pubkey>,
    ) -> Result<()> {
        ctx.accounts.update_escrow(receive, expiration, taker)
    }
}
