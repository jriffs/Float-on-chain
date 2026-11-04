use anchor_lang::prelude::Pubkey;
use std::str::FromStr;

pub const KVAULT_PROGRAM_ID: Pubkey = solana_program::pubkey!("devkRngFnfp4gBc5a3LsadgbQKdPo8MSZ4prFiNSVmY");
pub const DEVNET_USDC_VAULT: Pubkey = solana_program::pubkey!("hXm2xSRF5PLKGMrTvAWqKhR76MuJX5dAabeSChkjqu2");

pub const GROUP_SEED: &[u8] = b"group";
pub const MEMBER_BALANCE_SEED: &[u8] = b"member_balance";
pub const POOL_SEED: &[u8] = b"pool";
pub const EXPENSE_SEED: &[u8] = b"expense";

pub const MAX_NAME_LEN: usize = 32;
pub const MAX_DESCRIPTION_LEN: usize = 64;