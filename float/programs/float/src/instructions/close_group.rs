use anchor_lang::prelude::*;

use crate::constants::*;
use crate::error::FloatError;
use crate::state::*;

#[derive(Accounts)]
#[instruction(group_name: String)]
pub struct CloseGroup<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        mut,
        seeds = [GROUP_SEED, creator.key().as_ref(), group_name.as_bytes()],
        bump = group.bump,
        constraint = group.creator == creator.key() @ FloatError::Unauthorized,
        constraint = group.status == GROUP_STATUS_ACTIVE @ FloatError::GroupNotActive
    )]
    pub group: Account<'info, Group>,

    #[account(
        seeds = [POOL_SEED, group.key().as_ref()],
        bump = pool.bump,
        constraint = !pool.is_deployed @ FloatError::PoolIsDeployed
    )]
    pub pool: Account<'info, Pool>,
}

pub fn handler(ctx: Context<CloseGroup>, _group_name: String) -> Result<()> {
    let group = &mut ctx.accounts.group;
    group.status = GROUP_STATUS_CLOSED;
    Ok(())
}