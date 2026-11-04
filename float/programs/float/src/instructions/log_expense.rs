use anchor_lang::prelude::*;

use crate::constants::*;
use crate::error::FloatError;
use crate::state::*;

#[derive(Accounts)]
#[instruction(group_name: String, amount: u64, description: String)]
pub struct LogExpense<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: only used for seeds
    pub creator: UncheckedAccount<'info>,

    #[account(
        mut,
        has_one = creator,
        seeds = [GROUP_SEED, creator.key().as_ref(), group_name.as_bytes()],
        bump = group.bump,
        constraint = group.status == GROUP_STATUS_ACTIVE @ FloatError::GroupNotActive
    )]
    pub group: Account<'info, Group>,

    // Payer must already be a member
    #[account(
        mut,
        seeds = [MEMBER_BALANCE_SEED, group.key().as_ref(), payer.key().as_ref()],
        bump = payer_balance.bump,
        constraint = payer_balance.member == payer.key() @ FloatError::Unauthorized
    )]
    pub payer_balance: Account<'info, MemberBalance>,

    #[account(
        init,
        payer = payer,
        space = 8 + Expense::INIT_SPACE,
        seeds = [
            EXPENSE_SEED,
            group.key().as_ref(),
            &group.expense_count.to_le_bytes()
        ],
        bump
    )]
    pub expense: Account<'info, Expense>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<LogExpense>,
    _group_name: String,
    amount: u64,
    description: String,
    shares: Vec<u64>,
) -> Result<()> {
    require!(amount > 0, FloatError::ZeroAmount);
    require!(
        description.len() <= MAX_DESCRIPTION_LEN,
        FloatError::NameTooLong
    );

    let group = &mut ctx.accounts.group;
    let expense = &mut ctx.accounts.expense;
    let payer = ctx.accounts.payer.key();
    let payer_balance = &mut ctx.accounts.payer_balance;

    let other_participants = ctx.remaining_accounts;
    let total_participants = 1 + other_participants.len(); // payer + others

    require!(total_participants >= 1, FloatError::InvalidShares);

    // Collect all member keys (payer first, then others)
    let mut member_keys = vec![payer];
    for acc_info in other_participants.iter() {
        let mb = Account::<MemberBalance>::try_from(acc_info)?;
        require!(mb.group == group.key(), FloatError::Unauthorized);
        require!(mb.member != payer, FloatError::InvalidShares); // payer should not be duplicated
        member_keys.push(mb.member);
    }

    // ---- Calculate shares ----
    let mut calculated_shares: Vec<u64> = Vec::with_capacity(total_participants);

    if shares.is_empty() {
        let base = amount / total_participants as u64;
        let remainder = amount % total_participants as u64;

        for (_i, &member) in member_keys.iter().enumerate() {
            let mut share = base;
            if member == payer {
                share = share.checked_add(remainder).ok_or(FloatError::Overflow)?;
            }
            calculated_shares.push(share);
        }
    } else {
        require!(shares.len() == total_participants, FloatError::InvalidShares);
        let total: u64 = shares.iter().try_fold(0u64, |acc, &x| {
            acc.checked_add(x).ok_or(FloatError::Overflow)
        })?;
        require!(total == amount, FloatError::InvalidShares);
        calculated_shares = shares;
    }

    // ---- Update payer (named account) ----
    let payer_share = calculated_shares[0];
    let net = (amount as i64)
        .checked_sub(payer_share as i64)
        .ok_or(FloatError::Overflow)?;
    payer_balance.balance = payer_balance
        .balance
        .checked_add(net)
        .ok_or(FloatError::Overflow)?;

    // ---- Update everyone else (remaining accounts) ----
    for (i, acc_info) in other_participants.iter().enumerate() {
        let mut mb = Account::<MemberBalance>::try_from(acc_info)?;
        let share = calculated_shares[i + 1]; // +1 because index 0 is payer

        mb.balance = mb
            .balance
            .checked_sub(share as i64)
            .ok_or(FloatError::Overflow)?;

        mb.exit(&crate::ID)?;
    }

    // ---- Write Expense ----
    let mut desc_bytes = [0u8; 64];
    let desc = description.as_bytes();
    desc_bytes[..desc.len()].copy_from_slice(desc);

    expense.group = group.key();
    expense.payer = payer;
    expense.amount = amount;
    expense.description = desc_bytes;
    expense.timestamp = Clock::get()?.unix_timestamp;
    expense.expense_index = group.expense_count;
    expense.participant_count = total_participants as u8;
    expense.paid_from_pool = false;
    expense.bump = ctx.bumps.expense;

    group.expense_count = group
        .expense_count
        .checked_add(1)
        .ok_or(FloatError::Overflow)?;

    Ok(())
}