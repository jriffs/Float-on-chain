use anchor_lang::prelude::*;

use crate::constants::*;
use crate::error::FloatError;
use crate::state::*;

#[derive(Accounts)]
#[instruction(name: String)]
pub struct JoinGroup<'info> {
    #[account(mut)]
    pub member: Signer<'info>,

    /// CHECK: Checked via constraint to ensure it is owned by the System Program
    #[account(owner = anchor_lang::solana_program::system_program::ID)]
    pub creator: UncheckedAccount<'info>,

    #[account(
        mut,
        has_one = creator,
        seeds = [GROUP_SEED, creator.key().as_ref(), name.as_bytes()],
        bump = group.bump,
        constraint = group.status == GROUP_STATUS_ACTIVE @ FloatError::GroupNotActive
    )]
    pub group: Account<'info, Group>,

    #[account(
        init,
        payer = member,
        space = 8 + MemberBalance::INIT_SPACE,
        seeds = [MEMBER_BALANCE_SEED, group.key().as_ref(), member.key().as_ref()],
        bump
    )]
    pub member_balance: Account<'info, MemberBalance>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<JoinGroup>, _name: String) -> Result<()> {
    let group = &mut ctx.accounts.group;
    let member_balance = &mut ctx.accounts.member_balance;

    // Increment member count (safe for MVP — u8 is enough)
    group.member_count = group
        .member_count
        .checked_add(1)
        .ok_or(FloatError::Overflow)?;

    member_balance.group = group.key();
    member_balance.member = ctx.accounts.member.key();
    member_balance.balance = 0;
    member_balance.deposited_principal = 0;
    member_balance.pool_shares = 0;
    member_balance.bump = ctx.bumps.member_balance;

    Ok(())
}