use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("The escrow has expired")]
    EscrowExpired,
    #[msg("Expiration must be in the future")]
    InvalidExpiration,
    #[msg("Amount must be greater than zero")]
    InvalidAmount,
    #[msg("This escrow is reserved for a different taker")]
    InvalidTaker,
}
