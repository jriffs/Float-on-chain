use anchor_lang::prelude::*;

#[error_code]
pub enum FloatError {
    #[msg("Group name too long")]
    NameTooLong,
    #[msg("Group is not active")]
    GroupNotActive,
    #[msg("Group is already closed")]
    GroupAlreadyClosed,
    #[msg("Pool is not enabled")]
    PoolNotEnabled,
    #[msg("Pool is already enabled")]
    PoolAlreadyEnabled,
    #[msg("Pool funds are currently deployed to Kamino")]
    PoolIsDeployed,
    #[msg("Pool funds are not deployed")]
    PoolNotDeployed,
    #[msg("Invalid shares: must sum exactly to amount")]
    InvalidShares,
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Unauthorized")]
    Unauthorized,
    #[msg("Member already exists")]
    MemberAlreadyExists,
    #[msg("Insufficient pool value")]
    InsufficientPoolValue,
    #[msg("Cannot withdraw while group is not closed")]
    GroupNotClosed,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Cannot close group while member balances remain unsettled")]
    OutstandingBalances,
    #[msg("Pool account is missing or inconsistent with group state")]
    InvalidPoolState,
    #[msg("Settlement mint does not match the group's configured mint")]
    InvalidSettlementMint,
}