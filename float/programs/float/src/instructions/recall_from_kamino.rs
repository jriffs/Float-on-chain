use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::AccountMeta;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};
use kvault_interface::instructions::withdraw::{withdraw, WithdrawAccounts};

use crate::constants::*;
use crate::error::FloatError;
use crate::kamino::{convert_kvault_instruction, invoke_kvault, to_sdk_pubkey};
use crate::state::*;

#[derive(Accounts)]
#[instruction(group_name: String)]
pub struct RecallFromKamino<'info> {
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
        constraint = group.pool_enabled @ FloatError::PoolNotEnabled,
    )]
    pub group: Account<'info, Group>,

    #[account(
        mut,
        seeds = [POOL_SEED, group.key().as_ref()],
        bump = pool.bump,
        constraint = pool.group == group.key() @ FloatError::InvalidPoolState,
        constraint = pool.is_deployed @ FloatError::PoolNotDeployed,
        constraint = pool.kamino_vault == vault_state.key() @ FloatError::InvalidKaminoVault,
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        address = group.settlement_mint @ FloatError::InvalidSettlementMint
    )]
    pub settlement_mint: InterfaceAccount<'info, Mint>,

    /// Pool's underlying token ATA (authority = Pool PDA). Receives redeemed funds.
    #[account(
        mut,
        associated_token::mint = settlement_mint,
        associated_token::authority = pool,
        associated_token::token_program = token_program,
    )]
    pub pool_token_account: InterfaceAccount<'info, TokenAccount>,

    /// Pool's Kamino shares ATA (authority = Pool PDA). Must be fully burned.
    #[account(
        mut,
        associated_token::mint = shares_mint,
        associated_token::authority = pool,
        associated_token::token_program = shares_token_program,
    )]
    pub pool_shares_account: InterfaceAccount<'info, TokenAccount>,

    // ---- Kamino WithdrawAccounts (fixed set) ----

    /// CHECK: Kamino vault state.
    #[account(mut)]
    pub vault_state: UncheckedAccount<'info>,

    /// CHECK: Global config.
    pub global_config: UncheckedAccount<'info>,

    /// CHECK: Vault token vault.
    #[account(mut)]
    pub token_vault: UncheckedAccount<'info>,

    /// CHECK: Base vault authority.
    pub base_vault_authority: UncheckedAccount<'info>,

    /// CHECK: Shares mint.
    #[account(mut)]
    pub shares_mint: UncheckedAccount<'info>,

    /// CHECK: Invested vault state (same as vault_state for most vaults).
    #[account(mut)]
    pub invested_vault_state: UncheckedAccount<'info>,

    /// CHECK: Reserve.
    #[account(mut)]
    pub reserve: UncheckedAccount<'info>,

    /// CHECK: CToken vault.
    #[account(mut)]
    pub ctoken_vault: UncheckedAccount<'info>,

    /// CHECK: Lending market.
    pub lending_market: UncheckedAccount<'info>,

    /// CHECK: Lending market authority.
    pub lending_market_authority: UncheckedAccount<'info>,

    /// CHECK: Reserve liquidity supply.
    #[account(mut)]
    pub reserve_liquidity_supply: UncheckedAccount<'info>,

    /// CHECK: Reserve collateral mint.
    #[account(mut)]
    pub reserve_collateral_mint: UncheckedAccount<'info>,

    /// CHECK: Reserve collateral token program.
    pub reserve_collateral_token_program: UncheckedAccount<'info>,

    /// CHECK: Instructions sysvar.
    #[account(address = pubkey!("Sysvar1nstructions1111111111111111111111111"))]
    pub instruction_sysvar_account: UncheckedAccount<'info>,

    /// CHECK: Event authority.
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
    ctx: Context<'info, RecallFromKamino<'info>>,
    _group_name: String,
) -> Result<()> {
    let shares_before = ctx.accounts.pool_shares_account.amount;
    require!(shares_before > 0, FloatError::ZeroAmount);

    let token_before = ctx.accounts.pool_token_account.amount;

    // ---- Remaining accounts = refresh list from client resolver ----
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

    // single conversion into the kvault WithdrawAccounts
    let sdk_accounts = WithdrawAccounts {
        user: to_sdk_pubkey(ctx.accounts.pool.key()),
        vault_state: to_sdk_pubkey(ctx.accounts.vault_state.key()),
        global_config: to_sdk_pubkey(ctx.accounts.global_config.key()),
        token_vault: to_sdk_pubkey(ctx.accounts.token_vault.key()),
        base_vault_authority: to_sdk_pubkey(ctx.accounts.base_vault_authority.key()),
        user_token_ata: to_sdk_pubkey(ctx.accounts.pool_token_account.key()),
        token_mint: to_sdk_pubkey(ctx.accounts.settlement_mint.key()),
        user_shares_ata: to_sdk_pubkey(ctx.accounts.pool_shares_account.key()),
        shares_mint: to_sdk_pubkey(ctx.accounts.shares_mint.key()),
        token_program: to_sdk_pubkey(ctx.accounts.token_program.key()),
        shares_token_program: to_sdk_pubkey(ctx.accounts.shares_token_program.key()),
        klend_program: to_sdk_pubkey(ctx.accounts.klend_program.key()),
        invested_vault_state: to_sdk_pubkey(ctx.accounts.invested_vault_state.key()),
        reserve: to_sdk_pubkey(ctx.accounts.reserve.key()),
        ctoken_vault: to_sdk_pubkey(ctx.accounts.ctoken_vault.key()),
        lending_market: to_sdk_pubkey(ctx.accounts.lending_market.key()),
        lending_market_authority: to_sdk_pubkey(ctx.accounts.lending_market_authority.key()),
        reserve_liquidity_supply: to_sdk_pubkey(ctx.accounts.reserve_liquidity_supply.key()),
        reserve_collateral_mint: to_sdk_pubkey(ctx.accounts.reserve_collateral_mint.key()),
        reserve_collateral_token_program: to_sdk_pubkey(
            ctx.accounts.reserve_collateral_token_program.key(),
        ),
        instruction_sysvar_account: to_sdk_pubkey(ctx.accounts.instruction_sysvar_account.key()),
    };

    // u64::MAX = full position. Program clamps to actual share balance.
    let raw_ix = withdraw(sdk_accounts, u64::MAX, sdk_remaining);
    let ix = convert_kvault_instruction(raw_ix)?;

    let group_key = ctx.accounts.group.key();
    let pool_seeds: &[&[u8]] = &[POOL_SEED, group_key.as_ref(), &[ctx.accounts.pool.bump]];
    let signer_seeds = &[pool_seeds];

    let mut available: Vec<AccountInfo> = vec![
        ctx.accounts.pool.to_account_info(),
        ctx.accounts.vault_state.to_account_info(),
        ctx.accounts.global_config.to_account_info(),
        ctx.accounts.token_vault.to_account_info(),
        ctx.accounts.base_vault_authority.to_account_info(),
        ctx.accounts.pool_token_account.to_account_info(),
        ctx.accounts.settlement_mint.to_account_info(),
        ctx.accounts.pool_shares_account.to_account_info(),
        ctx.accounts.shares_mint.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.shares_token_program.to_account_info(),
        ctx.accounts.klend_program.to_account_info(),
        ctx.accounts.event_authority.to_account_info(),
        ctx.accounts.kvault_program.to_account_info(),
        ctx.accounts.invested_vault_state.to_account_info(),
        ctx.accounts.reserve.to_account_info(),
        ctx.accounts.ctoken_vault.to_account_info(),
        ctx.accounts.lending_market.to_account_info(),
        ctx.accounts.lending_market_authority.to_account_info(),
        ctx.accounts.reserve_liquidity_supply.to_account_info(),
        ctx.accounts.reserve_collateral_mint.to_account_info(),
        ctx.accounts
            .reserve_collateral_token_program
            .to_account_info(),
        ctx.accounts.instruction_sysvar_account.to_account_info(),
    ];
    available.extend(ctx.remaining_accounts.iter().cloned());

    invoke_kvault(&ix, &available, signer_seeds)?;

    // ---- Guardrail: only clear is_deployed after FULL redemption ----
    ctx.accounts.pool_shares_account.reload()?;
    ctx.accounts.pool_token_account.reload()?;

    require!(
        ctx.accounts.pool_shares_account.amount == 0,
        FloatError::PoolIsDeployed // partial redemption is not allowed
    );

    require!(
        ctx.accounts.pool_token_account.amount > token_before,
        FloatError::ZeroAmount
    );

    let pool = &mut ctx.accounts.pool;
    pool.is_deployed = false;
    // Accounting refresh is the responsibility of harvest_yield.
    // We deliberately do NOT touch total_value here so the guardrail stays strict.

    Ok(())
}