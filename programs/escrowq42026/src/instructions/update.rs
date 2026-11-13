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
    //Maker can change how much token B they want
    pub fn update_escrow(&mut self, receive: u64) -> Result<()> {
        require!(receive > 0, ErrorCode::InvalidAmount);

        self.escrow.receive = receive;
        Ok(())
    }
}
