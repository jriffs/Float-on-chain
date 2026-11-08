use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::AccountMeta;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};
use kvault_interface::instructions::deposit::{deposit, DepositAccounts};

use crate::constants::*;
use crate::error::FloatError;
use crate::kamino::{convert_kvault_instruction, invoke_kvault, to_sdk_pubkey};
use crate::state::*;

#[derive(Accounts)]
#[instruction(group_name: String)]
pub struct DeployToKamino<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    /// CHECK: Used for group PDA seeds.
    pub creator_authority: UncheckedAccount<'info>,

    #[account(
        seeds = [
            GROUP_SEED,
            creator_authority.key().as_ref(),
            group_name.as_bytes()
        ],
        bump = group.bump,
        constraint = group.creator == creator.key() @ FloatError::Unauthorized,
        constraint = group.creator == creator_authority.key() @ FloatError::Unauthorized,
        constraint = group.status == GROUP_STATUS_ACTIVE @ FloatError::GroupNotActive,
        constraint = group.pool_enabled @ FloatError::PoolNotEnabled,
    )]
    pub group: Account<'info, Group>,

    #[account(
        mut,
        seeds = [POOL_SEED, group.key().as_ref()],
        bump = pool.bump,
        constraint = pool.group == group.key() @ FloatError::InvalidPoolState,
        constraint = !pool.is_deployed @ FloatError::PoolIsDeployed,
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        address = group.settlement_mint @ FloatError::InvalidSettlementMint
    )]
    pub settlement_mint: InterfaceAccount<'info, Mint>,

    /// Pool's underlying token ATA (authority = Pool PDA).
    #[account(
        mut,
        associated_token::mint = settlement_mint,
        associated_token::authority = pool,
        associated_token::token_program = token_program,
    )]
    pub pool_token_account: InterfaceAccount<'info, TokenAccount>,

    /// Pool's Kamino shares ATA (authority = Pool PDA).
    #[account(
        mut,
        associated_token::mint = shares_mint,
        associated_token::authority = pool,
        associated_token::token_program = shares_token_program,
    )]
    pub pool_shares_account: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: Kamino vault state. Client resolver must supply the correct vault.
    #[account(mut)]
    pub vault_state: UncheckedAccount<'info>,

    /// CHECK: Vault token vault.
    #[account(mut)]
    pub token_vault: UncheckedAccount<'info>,

    /// CHECK: Base vault authority PDA.
    pub base_vault_authority: UncheckedAccount<'info>,

    /// CHECK: Kamino shares mint.
    #[account(mut)]
    pub shares_mint: UncheckedAccount<'info>,

    /// CHECK: Event authority (patched to Devnet by convert_kvault_instruction).
    pub event_authority: UncheckedAccount<'info>,

    /// CHECK: KVault program (Devnet).
    #[account(address = KVAULT_PROGRAM_ID @ FloatError::InvalidKaminoProgram)]
    pub kvault_program: UncheckedAccount<'info>,

    /// CHECK: Klend program.
    #[account(address = KLEND_PROGRAM_ID)]
    pub klend_program: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub shares_token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

pub fn handler<'info>(
    ctx: Context<'info, DeployToKamino<'info>>,
    _group_name: String,
) -> Result<()> {
    let amount = ctx.accounts.pool_token_account.amount;
    require!(amount > 0, FloatError::ZeroAmount);

    let shares_before = ctx.accounts.pool_shares_account.amount;

    // ---- Build remaining metas from the client-supplied refresh list ----
    let sdk_remaining: Vec<solana_instruction::AccountMeta> = ctx
        .remaining_accounts
        .iter()
        .map(|account| {
            let key = to_sdk_pubkey(*account.key);
            if account.is_writable {
                solana_instruction::AccountMeta::new(key, account.is_signer)
            } else {
                solana_instruction::AccountMeta::new_readonly(key, account.is_signer)
            }
        })
        .collect();

    // single conversion into the kvault DepositAccounts
    let sdk_accounts = DepositAccounts {
        user: to_sdk_pubkey(ctx.accounts.pool.key()),
        vault_state: to_sdk_pubkey(ctx.accounts.vault_state.key()),
        token_vault: to_sdk_pubkey(ctx.accounts.token_vault.key()),
        token_mint: to_sdk_pubkey(ctx.accounts.settlement_mint.key()),
        base_vault_authority: to_sdk_pubkey(ctx.accounts.base_vault_authority.key()),
        shares_mint: to_sdk_pubkey(ctx.accounts.shares_mint.key()),
        user_token_ata: to_sdk_pubkey(ctx.accounts.pool_token_account.key()),
        user_shares_ata: to_sdk_pubkey(ctx.accounts.pool_shares_account.key()),
        klend_program: to_sdk_pubkey(ctx.accounts.klend_program.key()),
        token_program: to_sdk_pubkey(ctx.accounts.token_program.key()),
        shares_token_program: to_sdk_pubkey(ctx.accounts.shares_token_program.key()),
    };

    let raw_ix = deposit(sdk_accounts, amount, sdk_remaining);
    let ix = convert_kvault_instruction(raw_ix)?;

    // ---- Pool PDA signs the deposit ----
    let group_key = ctx.accounts.group.key();
    let pool_seeds: &[&[u8]] = &[POOL_SEED, group_key.as_ref(), &[ctx.accounts.pool.bump]];
    let signer_seeds = &[pool_seeds];

    // Collect every AccountInfo the CPI may need.
    let mut available: Vec<AccountInfo> = vec![
        ctx.accounts.pool.to_account_info(),
        ctx.accounts.vault_state.to_account_info(),
        ctx.accounts.token_vault.to_account_info(),
        ctx.accounts.settlement_mint.to_account_info(),
        ctx.accounts.base_vault_authority.to_account_info(),
        ctx.accounts.shares_mint.to_account_info(),
        ctx.accounts.pool_token_account.to_account_info(),
        ctx.accounts.pool_shares_account.to_account_info(),
        ctx.accounts.klend_program.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.shares_token_program.to_account_info(),
        ctx.accounts.event_authority.to_account_info(),
        ctx.accounts.kvault_program.to_account_info(),
    ];
    available.extend(ctx.remaining_accounts.iter().cloned());

    invoke_kvault(&ix, &available, signer_seeds)?;

    // ---- Post-condition: shares must have been minted ----
    ctx.accounts.pool_shares_account.reload()?;
    require!(
        ctx.accounts.pool_shares_account.amount > shares_before,
        FloatError::ZeroAmount
    );

    // ---- Mark deployed ----
    let pool = &mut ctx.accounts.pool;
    pool.is_deployed = true;
    pool.kamino_vault = ctx.accounts.vault_state.key();
    // total_value stays at the pre-deploy principal until harvest after recall.
    // Tokens left the pool ATA; value is now represented by Kamino shares.

    Ok(())
}