use anchor_lang::prelude::*;

use crate::{error::ErrorCode, Escrow, ESCROW_SEED};

#[derive(Accounts)]
pub struct Update<'info> {
    pub maker: Signer<'info>,
    #[account(
        mut,
        has_one = maker,
        seeds = [ESCROW_SEED, maker.key().as_ref(), escrow.seed.to_le_bytes().as_ref()],
        bump = escrow.bump
    )]
    pub escrow: Account<'info, Escrow>,
}

impl<'info> Update<'info> {
    //Maker can change the price, the deadline and who is allowed to take
    pub fn update_escrow(
        &mut self,
        receive: u64,
        expiration: i64,
        taker: Option<Pubkey>,
    ) -> Result<()> {
        require!(receive > 0, ErrorCode::InvalidAmount);
        require!(
            expiration > Clock::get()?.unix_timestamp,
            ErrorCode::InvalidExpiration
        );

        self.escrow.receive = receive;
        self.escrow.expiration = expiration;
        self.escrow.taker = taker;
        Ok(())
    }
}
