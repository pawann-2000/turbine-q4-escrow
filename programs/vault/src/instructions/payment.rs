use anchor_lang::{
    prelude::*,
    system_program::{transfer, Transfer},
};

use crate::{error::VaultError, VaultState, STATE_SEED, VAULT_SEED};

#[derive(Accounts)]
pub struct Payment<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
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

impl<'info> Payment<'info> {
    pub fn deposit(&mut self, amount: u64) -> Result<()> {
        require!(amount > 0, VaultError::InvalidAmount);

        let cpi_accounts = Transfer {
            from: self.user.to_account_info(),
            to: self.vault.to_account_info(),
        };

        let cpi_context = CpiContext::new(self.system_program.key(), cpi_accounts);

        transfer(cpi_context, amount)
    }

    pub fn withdraw(&mut self, amount: u64) -> Result<()> {
        require!(amount > 0, VaultError::InvalidAmount);

        //Rent-exempt reserve stays in the vault until close
        let rent_exempt = Rent::get()?.minimum_balance(self.vault.data_len());
        let available = self.vault.lamports().saturating_sub(rent_exempt);
        require!(amount <= available, VaultError::InsufficientFunds);

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

        transfer(cpi_context, amount)
    }
}
