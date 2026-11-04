use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked},
};

use crate::constants::*;
use crate::error::FloatError;
use crate::state::*;
use crate::utils::update_unsettled_member_count;

#[derive(Accounts)]
#[instruction(group_name: String)]
pub struct Settle<'info> {
    #[account(mut)]
    pub settler: Signer<'info>,

    /// CHECK: Used for seeds
    pub creator: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [GROUP_SEED, creator.key().as_ref(), group_name.as_bytes()],
        bump = group.bump,
        constraint = group.creator == creator.key() @ FloatError::Unauthorized,
        constraint = group.status == GROUP_STATUS_ACTIVE @ FloatError::GroupNotActive
    )]
    pub group: Account<'info, Group>,

    #[account(
        mut,
        seeds = [MEMBER_BALANCE_SEED, group.key().as_ref(), settler.key().as_ref()],
        bump = from_balance.bump,
        constraint = from_balance.member == settler.key() @ FloatError::Unauthorized,
        constraint = from_balance.group == group.key() @ FloatError::Unauthorized
    )]
    pub from_balance: Account<'info, MemberBalance>,

    #[account(
        mut,
        seeds = [MEMBER_BALANCE_SEED, group.key().as_ref(), to_balance.member.as_ref()],
        bump = to_balance.bump,
        constraint = to_balance.group == group.key() @ FloatError::Unauthorized
    )]
    pub to_balance: Account<'info, MemberBalance>,

    #[account(
        constraint = settlement_mint.key() == group.settlement_mint
            @ FloatError::InvalidSettlementMint
    )]
    pub settlement_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = settlement_mint,
        associated_token::authority = settler,
        associated_token::token_program = token_program,
    )]
    pub from_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = settlement_mint,
        associated_token::authority = to_balance.member,
        associated_token::token_program = token_program,
    )]
    pub to_token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Settle>, _group_name: String, amount: u64) -> Result<()> {
    require!(amount > 0, FloatError::ZeroAmount);

    let amount_i64 = i64::try_from(amount).map_err(|_| FloatError::Overflow)?;

    let group = &mut ctx.accounts.group;
    let from = &mut ctx.accounts.from_balance;
    let to = &mut ctx.accounts.to_balance;

    require!(from.group == group.key(), FloatError::Unauthorized);
    require!(to.group == group.key(), FloatError::Unauthorized);
    require!(from.member != to.member, FloatError::Unauthorized);

    require!(
        from.balance <= -amount_i64,
        FloatError::InsufficientPoolValue
    );
    require!(
        to.balance >= amount_i64,
        FloatError::InsufficientPoolValue
    );

    let new_from_balance = from
        .balance
        .checked_add(amount_i64)
        .ok_or(FloatError::Overflow)?;

    let new_to_balance = to
        .balance
        .checked_sub(amount_i64)
        .ok_or(FloatError::Overflow)?;

    let decimals = ctx.accounts.settlement_mint.decimals;

    let cpi_accounts = TransferChecked {
        from: ctx.accounts.from_token_account.to_account_info(),
        mint: ctx.accounts.settlement_mint.to_account_info(),
        to: ctx.accounts.to_token_account.to_account_info(),
        authority: ctx.accounts.settler.to_account_info(),
    };

    let cpi_ctx = CpiContext::new(
        ctx.accounts.token_program.key(),
        cpi_accounts,
    );

    token_interface::transfer_checked(cpi_ctx, amount, decimals)?;

    update_unsettled_member_count(
        group,
        from.balance,
        new_from_balance,
    )?;

    update_unsettled_member_count(
        group,
        to.balance,
        new_to_balance,
    )?;

    from.balance = new_from_balance;
    to.balance = new_to_balance;

    Ok(())
}