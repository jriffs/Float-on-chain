use anchor_lang::prelude::*;

use crate::constants::*;
use crate::error::FloatError;
use crate::state::*;
use crate::utils::update_unsettled_member_count;

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

    #[account(
        mut,
        seeds = [MEMBER_BALANCE_SEED, group.key().as_ref(), payer.key().as_ref()],
        bump = payer_balance.bump,
        constraint = payer_balance.member == payer.key() @ FloatError::Unauthorized,
        constraint = payer_balance.group == group.key() @ FloatError::Unauthorized
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
    let total_participants = 1 + other_participants.len();

    require!(
        total_participants <= group.member_count as usize,
        FloatError::InvalidShares
    );
    require!(
        total_participants <= u8::MAX as usize,
        FloatError::InvalidShares
    );

    let mut member_keys = vec![payer];

    for acc_info in other_participants.iter() {
        require!(acc_info.is_writable, FloatError::Unauthorized);

        let mb = Account::<MemberBalance>::try_from(acc_info)?;

        require!(mb.group == group.key(), FloatError::Unauthorized);
        require!(mb.member != payer, FloatError::InvalidShares);
        require!(
            !member_keys.contains(&mb.member),
            FloatError::InvalidShares
        );

        let (expected_pda, _) = Pubkey::find_program_address(
            &[
                MEMBER_BALANCE_SEED,
                group.key().as_ref(),
                mb.member.as_ref(),
            ],
            ctx.program_id,
        );

        require!(
            acc_info.key() == expected_pda,
            FloatError::Unauthorized
        );

        member_keys.push(mb.member);
    }

    let mut calculated_shares: Vec<u64> = Vec::with_capacity(total_participants);

    if shares.is_empty() {
        let participant_count = total_participants as u64;
        let base = amount / participant_count;
        let remainder = amount % participant_count;

        for member in member_keys.iter() {
            let mut share = base;

            if *member == payer {
                share = share
                    .checked_add(remainder)
                    .ok_or(FloatError::Overflow)?;
            }

            calculated_shares.push(share);
        }
    } else {
        require!(
            shares.len() == total_participants,
            FloatError::InvalidShares
        );

        let total = shares.iter().try_fold(0u64, |acc, share| {
            acc.checked_add(*share).ok_or(FloatError::Overflow)
        })?;

        require!(total == amount, FloatError::InvalidShares);
        calculated_shares = shares;
    }

    // Update payer balance and its zero/non-zero transition.
    let amount_i64 = i64::try_from(amount).map_err(|_| FloatError::Overflow)?;
    let payer_share_i64 =
        i64::try_from(calculated_shares[0]).map_err(|_| FloatError::Overflow)?;

    let old_payer_balance = payer_balance.balance;
    let payer_net = amount_i64
        .checked_sub(payer_share_i64)
        .ok_or(FloatError::Overflow)?;

    let new_payer_balance = old_payer_balance
        .checked_add(payer_net)
        .ok_or(FloatError::Overflow)?;

    update_unsettled_member_count(
        group,
        old_payer_balance,
        new_payer_balance,
    )?;

    payer_balance.balance = new_payer_balance;

    // Update other participants.
    for (i, acc_info) in other_participants.iter().enumerate() {
        let mut mb = Account::<MemberBalance>::try_from(acc_info)?;

        let share_i64 = i64::try_from(calculated_shares[i + 1])
            .map_err(|_| FloatError::Overflow)?;

        let old_balance = mb.balance;
        let new_balance = old_balance
            .checked_sub(share_i64)
            .ok_or(FloatError::Overflow)?;

        update_unsettled_member_count(group, old_balance, new_balance)?;

        mb.balance = new_balance;
        mb.exit(ctx.program_id)?;
    }

    let mut desc_bytes = [0u8; 64];
    let desc = description.as_bytes();
    desc_bytes[..desc.len()].copy_from_slice(desc);

    expense.group = group.key();
    expense.payer = payer;
    expense.amount = amount;
    expense.description = desc_bytes;
    expense.timestamp = Clock::get()?.unix_timestamp;
    expense.expense_index = group.expense_count;
    expense.participant_count =
        u8::try_from(total_participants).map_err(|_| FloatError::Overflow)?;
    expense.paid_from_pool = false;
    expense.bump = ctx.bumps.expense;

    group.expense_count = group
        .expense_count
        .checked_add(1)
        .ok_or(FloatError::Overflow)?;

    Ok(())
}