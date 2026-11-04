use anchor_lang::prelude::*;

use crate::error::FloatError;

#[account]
#[derive(InitSpace)]
pub struct Group {
    pub creator: Pubkey,
    pub name: [u8; 32],
    pub settlement_mint: Pubkey,
    pub member_count: u8,
    pub unsettled_member_count: u8,
    pub expense_count: u32,
    pub pool_enabled: bool,
    pub status: u8,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct MemberBalance {
    pub group: Pubkey,
    pub member: Pubkey,
    pub balance: i64,
    pub deposited_principal: u64,
    pub pool_shares: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Pool {
    pub group: Pubkey,
    pub total_shares: u64,
    pub total_value: u64,
    pub kamino_obligation: Pubkey,
    pub last_harvest_ts: i64,
    pub is_deployed: bool,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Expense {
    pub group: Pubkey,
    pub payer: Pubkey,
    pub amount: u64,
    pub description: [u8; 64],
    pub timestamp: i64,
    pub expense_index: u32,
    pub participant_count: u8,
    pub paid_from_pool: bool,
    pub bump: u8,
}

pub const GROUP_STATUS_ACTIVE: u8 = 0;
pub const GROUP_STATUS_SETTLING: u8 = 1;
pub const GROUP_STATUS_CLOSED: u8 = 2;