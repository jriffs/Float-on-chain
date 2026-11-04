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

    pub pool: Option<Account<'info, Pool>>,
}

pub fn handler(ctx: Context<CloseGroup>, _group_name: String) -> Result<()> {
    let group = &mut ctx.accounts.group;

    require!(
        group.unsettled_member_count == 0,
        FloatError::OutstandingBalances
    );

    if group.pool_enabled {
        let pool = ctx.accounts.pool.as_ref().ok_or(FloatError::InvalidPoolState)?;

        let group_key = group.key();
        let (expected_pool, _) = Pubkey::find_program_address(
            &[POOL_SEED, group_key.as_ref()],
            ctx.program_id,
        );

        require!(
            pool.key() == expected_pool,
            FloatError::InvalidPoolState
        );
        require!(
            pool.group == group_key,
            FloatError::InvalidPoolState
        );
        require!(!pool.is_deployed, FloatError::PoolIsDeployed);
    } else {
        require!(
            ctx.accounts.pool.is_none(),
            FloatError::InvalidPoolState
        );
    }

    group.status = GROUP_STATUS_CLOSED;

    Ok(())
}