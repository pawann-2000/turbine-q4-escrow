use anchor_lang::{
    prelude::*,
    system_program::{transfer, Transfer},
};

use crate::{VaultState, STATE_SEED, VAULT_SEED};

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        init,
        payer = user,
        space = VaultState::DISCRIMINATOR.len() + VaultState::INIT_SPACE,
        seeds = [STATE_SEED, user.key().as_ref()],
        bump
    )]
    pub vault_state: Account<'info, VaultState>,
    #[account(
        mut,
        seeds = [VAULT_SEED, vault_state.key().as_ref()],
        bump
    )]
    pub vault: SystemAccount<'info>,
    pub system_program: Program<'info, System>,
}

impl<'info> Initialize<'info> {
    pub fn initialize(&mut self, bumps: &InitializeBumps) -> Result<()> {
        self.vault_state.set_inner(VaultState {
            vault_bump: bumps.vault,
            state_bump: bumps.vault_state,
        });

        //Fund the vault with the rent-exempt minimum so any deposit amount works
        let rent_exempt = Rent::get()?.minimum_balance(self.vault.data_len());

        let cpi_accounts = Transfer {
            from: self.user.to_account_info(),
            to: self.vault.to_account_info(),
        };

        let cpi_context = CpiContext::new(self.system_program.key(), cpi_accounts);

        transfer(cpi_context, rent_exempt)
    }
}
