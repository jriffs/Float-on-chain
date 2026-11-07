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
pub struct Withdraw<'info> {
    #[account(mut)]
    pub member: Signer<'info>,

    /// CHECK: Used for seeds
    pub creator: UncheckedAccount<'info>,

    #[account(
        seeds = [GROUP_SEED, creator.key().as_ref(), group_name.as_bytes()],
        bump = group.bump,
        constraint = group.creator == creator.key() @ FloatError::Unauthorized,
        constraint = group.status == GROUP_STATUS_CLOSED @ FloatError::GroupNotClosed,
        constraint = group.pool_enabled @ FloatError::PoolNotEnabled
    )]
    pub group: Account<'info, Group>,

    #[account(
        mut,
        seeds = [POOL_SEED, group.key().as_ref()],
        bump = pool.bump,
        constraint = pool.group == group.key() @ FloatError::InvalidPoolState
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        mut,
        seeds = [MEMBER_BALANCE_SEED, group.key().as_ref(), member.key().as_ref()],
        bump = member_balance.bump,
        constraint = member_balance.member == member.key() @ FloatError::Unauthorized,
        constraint = member_balance.group == group.key() @ FloatError::Unauthorized
    )]
    pub member_balance: Account<'info, MemberBalance>,

    #[account(
        constraint = settlement_mint.key() == group.settlement_mint
            @ FloatError::InvalidSettlementMint
    )]
    pub settlement_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = settlement_mint,
        associated_token::authority = pool,
        associated_token::token_program = token_program,
    )]
    pub pool_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = settlement_mint,
        associated_token::authority = member,
        associated_token::token_program = token_program,
    )]
    pub member_token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Withdraw>, _group_name: String) -> Result<()> {
    let group_key = ctx.accounts.group.key();
    let pool = &mut ctx.accounts.pool;
    let member_balance = &mut ctx.accounts.member_balance;

    let shares = member_balance.pool_shares;

    require!(shares > 0, FloatError::ZeroAmount);
    require!(
        pool.total_shares > 0 && shares <= pool.total_shares,
        FloatError::InvalidShares
    );

    let amount_out = if shares == pool.total_shares {
        pool.total_value
    } else {
        let amount = (shares as u128)
            .checked_mul(pool.total_value as u128)
            .ok_or(FloatError::Overflow)?
            .checked_div(pool.total_shares as u128)
            .ok_or(FloatError::Overflow)?;

        u64::try_from(amount).map_err(|_| FloatError::Overflow)?
    };

    // Only invoke the token program when there is a non-zero transfer.
    if amount_out > 0 {
        let seeds = &[
            POOL_SEED,
            group_key.as_ref(),
            &[pool.bump],
        ];
        let signer_seeds = &[&seeds[..]];

        let decimals = ctx.accounts.settlement_mint.decimals;

        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                TransferChecked {
                    from: ctx.accounts.pool_token_account.to_account_info(),
                    mint: ctx.accounts.settlement_mint.to_account_info(),
                    to: ctx.accounts.member_token_account.to_account_info(),
                    authority: pool.to_account_info(),
                },
                signer_seeds,
            ),
            amount_out,
            decimals,
        )?;
    }

    let principal_out = member_balance.deposited_principal;

    pool.total_principal = pool
        .total_principal
        .checked_sub(principal_out)
        .ok_or(FloatError::Overflow)?;

    pool.total_shares = pool
        .total_shares
        .checked_sub(shares)
        .ok_or(FloatError::Overflow)?;

    pool.total_value = pool
        .total_value
        .checked_sub(amount_out)
        .ok_or(FloatError::Overflow)?;

    member_balance.pool_shares = 0;
    member_balance.deposited_principal = 0;

    Ok(())
}