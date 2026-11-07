use anchor_lang::prelude::*;

pub const GROUP_SEED: &[u8] = b"group";
pub const MEMBER_BALANCE_SEED: &[u8] = b"member_balance";
pub const POOL_SEED: &[u8] = b"pool";
pub const EXPENSE_SEED: &[u8] = b"expense";

pub const MAX_NAME_LEN: usize = 32;
pub const MAX_DESCRIPTION_LEN: usize = 64;

// Kamino Devnet programs.
pub const KVAULT_PROGRAM_ID: Pubkey =
    pubkey!("devkRngFnfp4gBc5a3LsadgbQKdPo8MSZ4prFiNSVmY");

pub const KLEND_PROGRAM_ID: Pubkey =
    pubkey!("KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD");

// Candidate Devnet USDC vault. The client resolver must verify its
// current state, mint and reserve allocation before deployment.
pub const DEVNET_KAMINO_VAULT: Pubkey =
    pubkey!("7319GuA3DwpJV1SHKKbyLp9MZwiopfc9rUKqZqWJua7J");

pub const DEVNET_USDC_MINT: Pubkey =
    pubkey!("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU");