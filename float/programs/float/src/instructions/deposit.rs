use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked},
};

use crate::constants::*;
use crate::error::FloatError;
use crate::state::*;

#[derive(Accounts)]
#[instruction(group_name: String)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub depositor: Signer<'info>,

    /// CHECK: Used for seeds
    pub creator: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [GROUP_SEED, creator.key().as_ref(), group_name.as_bytes()],
        bump = group.bump,
        constraint = group.creator == creator.key() @ FloatError::Unauthorized,
        constraint = group.status == GROUP_STATUS_ACTIVE @ FloatError::GroupNotActive,
        constraint = group.pool_enabled @ FloatError::PoolNotEnabled
    )]
    pub group: Account<'info, Group>,

    #[account(
        mut,
        seeds = [POOL_SEED, group.key().as_ref()],
        bump = pool.bump,
        constraint = !pool.is_deployed @ FloatError::PoolIsDeployed
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        mut,
        seeds = [MEMBER_BALANCE_SEED, group.key().as_ref(), depositor.key().as_ref()],
        bump = member_balance.bump,
        constraint = member_balance.member == depositor.key() @ FloatError::Unauthorized
    )]
    pub member_balance: Account<'info, MemberBalance>,

    pub settlement_mint: InterfaceAccount<'info, Mint>,

    // Depositor's token account
    #[account(
        mut,
        associated_token::mint = settlement_mint,
        associated_token::authority = depositor,
        associated_token::token_program = token_program,
    )]
    pub depositor_token_account: InterfaceAccount<'info, TokenAccount>,

    // Pool's token account (owned by the Pool PDA)
    #[account(
        mut,
        associated_token::mint = settlement_mint,
        associated_token::authority = pool,
        associated_token::token_program = token_program,
    )]
    pub pool_token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Deposit>, _group_name: String, amount: u64) -> Result<()> {
    require!(amount > 0, FloatError::ZeroAmount);

    let pool = &mut ctx.accounts.pool;
    let member_balance = &mut ctx.accounts.member_balance;

    // ---- Calculate shares ----
    let shares = if pool.total_shares == 0 {
        // First deposit → 1:1
        amount
    } else {
        // shares = amount * total_shares / total_value
        (amount as u128)
            .checked_mul(pool.total_shares as u128)
            .ok_or(FloatError::Overflow)?
            .checked_div(pool.total_value as u128)
            .ok_or(FloatError::Overflow)? as u64
    };

    require!(shares > 0, FloatError::ZeroAmount);

    // ---- Transfer tokens from depositor → pool ----
    let decimals = ctx.accounts.settlement_mint.decimals;

    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.depositor_token_account.to_account_info(),
                mint: ctx.accounts.settlement_mint.to_account_info(),
                to: ctx.accounts.pool_token_account.to_account_info(),
                authority: ctx.accounts.depositor.to_account_info(),
            },
        ),
        amount,
        decimals,
    )?;

    // ---- Update state ----
    member_balance.deposited_principal = member_balance
        .deposited_principal
        .checked_add(amount)
        .ok_or(FloatError::Overflow)?;

    member_balance.pool_shares = member_balance
        .pool_shares
        .checked_add(shares)
        .ok_or(FloatError::Overflow)?;

    pool.total_shares = pool
        .total_shares
        .checked_add(shares)
        .ok_or(FloatError::Overflow)?;

    pool.total_value = pool
        .total_value
        .checked_add(amount)
        .ok_or(FloatError::Overflow)?;

    Ok(())
}