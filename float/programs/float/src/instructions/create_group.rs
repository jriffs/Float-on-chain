use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

use crate::constants::*;
use crate::error::FloatError;
use crate::state::*;

#[derive(Accounts)]
#[instruction(name: String)]
pub struct CreateGroup<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        init,
        payer = creator,
        space = 8 + Group::INIT_SPACE,
        seeds = [GROUP_SEED, creator.key().as_ref(), name.as_bytes()],
        bump
    )]
    pub group: Account<'info, Group>,

    #[account(
        init,
        payer = creator,
        space = 8 + MemberBalance::INIT_SPACE,
        seeds = [MEMBER_BALANCE_SEED, group.key().as_ref(), creator.key().as_ref()],
        bump
    )]
    pub member_balance: Account<'info, MemberBalance>,

    pub settlement_mint: InterfaceAccount<'info, Mint>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<CreateGroup>,
    name: String,
    settlement_mint: Pubkey,
) -> Result<()> {
    require!(name.len() <= MAX_NAME_LEN, FloatError::NameTooLong);
    require!(
        ctx.accounts.settlement_mint.key() == settlement_mint,
        FloatError::InvalidSettlementMint
    );

    let group = &mut ctx.accounts.group;
    let member_balance = &mut ctx.accounts.member_balance;

    let mut name_bytes = [0u8; 32];
    name_bytes[..name.len()].copy_from_slice(name.as_bytes());

    group.creator = ctx.accounts.creator.key();
    group.name = name_bytes;
    group.settlement_mint = settlement_mint;
    group.member_count = 1;
    group.unsettled_member_count = 0;
    group.expense_count = 0;
    group.pool_enabled = false;
    group.status = GROUP_STATUS_ACTIVE;
    group.bump = ctx.bumps.group;

    member_balance.group = group.key();
    member_balance.member = ctx.accounts.creator.key();
    member_balance.balance = 0;
    member_balance.deposited_principal = 0;
    member_balance.pool_shares = 0;
    member_balance.bump = ctx.bumps.member_balance;

    Ok(())
}