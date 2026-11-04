use anchor_lang::prelude::*;

use crate::constants::*;
use crate::error::FloatError;
use crate::state::*;

#[derive(Accounts)]
#[instruction(group_name: String)]
pub struct EnablePool<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        mut,
        seeds = [GROUP_SEED, creator.key().as_ref(), group_name.as_bytes()],
        bump = group.bump,
        constraint = group.creator == creator.key() @ FloatError::Unauthorized,
        constraint = group.status == GROUP_STATUS_ACTIVE @ FloatError::GroupNotActive,
        constraint = !group.pool_enabled @ FloatError::PoolAlreadyEnabled
    )]
    pub group: Account<'info, Group>,

    #[account(
        init,
        payer = creator,
        space = 8 + Pool::INIT_SPACE,
        seeds = [POOL_SEED, group.key().as_ref()],
        bump
    )]
    pub pool: Account<'info, Pool>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<EnablePool>, _group_name: String) -> Result<()> {
    let group = &mut ctx.accounts.group;
    let pool = &mut ctx.accounts.pool;

    group.pool_enabled = true;

    pool.group = group.key();
    pool.total_shares = 0;
    pool.total_value = 0;
    pool.kamino_obligation = Pubkey::default();
    pool.last_harvest_ts = 0;
    pool.is_deployed = false;
    pool.bump = ctx.bumps.pool;

    Ok(())
}