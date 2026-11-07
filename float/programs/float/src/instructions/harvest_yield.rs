use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount};

use crate::constants::*;
use crate::error::FloatError;
use crate::state::*;

#[derive(Accounts)]
#[instruction(group_name: String)]
pub struct HarvestYield<'info> {
    pub creator: Signer<'info>,

    /// CHECK: Used to derive the group PDA.
    pub creator_authority: UncheckedAccount<'info>,

    #[account(
        seeds = [
            GROUP_SEED,
            creator_authority.key().as_ref(),
            group_name.as_bytes()
        ],
        bump = group.bump,
        constraint = group.creator == creator.key()
            @ FloatError::Unauthorized,
        constraint = group.pool_enabled
            @ FloatError::PoolNotEnabled,
    )]
    pub group: Account<'info, Group>,

    #[account(
        mut,
        seeds = [POOL_SEED, group.key().as_ref()],
        bump = pool.bump,
        constraint = pool.group == group.key()
            @ FloatError::InvalidPoolState,
        constraint = !pool.is_deployed
            @ FloatError::PoolIsDeployed,
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        address = group.settlement_mint
            @ FloatError::InvalidSettlementMint,
    )]
    pub settlement_mint: InterfaceAccount<'info, Mint>,

    #[account(
        associated_token::mint = settlement_mint,
        associated_token::authority = pool,
        associated_token::token_program = token_program,
    )]
    pub pool_token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, anchor_spl::token_interface::TokenInterface>,
}

pub fn handler(
    ctx: Context<HarvestYield>,
    _group_name: String,
) -> Result<()> {
    let pool = &mut ctx.accounts.pool;

    // This is an accounting refresh, not a Kamino harvest CPI.
    // Funds must already have been recalled into the Pool's token account.
    require!(!pool.is_deployed, FloatError::PoolIsDeployed);

    pool.total_value = ctx.accounts.pool_token_account.amount;
    pool.last_harvest_ts = Clock::get()?.unix_timestamp;

    Ok(())
}