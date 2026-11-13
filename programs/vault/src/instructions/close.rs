use anchor_lang::{
    prelude::*,
    system_program::{transfer, Transfer},
};

use crate::{VaultState, STATE_SEED, VAULT_SEED};

#[derive(Accounts)]
pub struct Close<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut,
        close = user,
        seeds = [STATE_SEED, user.key().as_ref()],
        bump = vault_state.state_bump
    )]
    pub vault_state: Account<'info, VaultState>,
    #[account(
        mut,
        seeds = [VAULT_SEED, vault_state.key().as_ref()],
        bump = vault_state.vault_bump
    )]
    pub vault: SystemAccount<'info>,
    pub system_program: Program<'info, System>,
}

impl<'info> Close<'info> {
    //Empty the vault back to the user, vault_state is closed by the close constraint
    pub fn close(&mut self) -> Result<()> {
        let vault_state_key = self.vault_state.key();
        let signer_seeds: [&[&[u8]]; 1] = [&[
            VAULT_SEED,
            vault_state_key.as_ref(),
            &[self.vault_state.vault_bump],
        ]];

        let cpi_accounts = Transfer {
            from: self.vault.to_account_info(),
            to: self.user.to_account_info(),
        };

        let cpi_context =
            CpiContext::new_with_signer(self.system_program.key(), cpi_accounts, &signer_seeds);

        transfer(cpi_context, self.vault.lamports())
    }
}
