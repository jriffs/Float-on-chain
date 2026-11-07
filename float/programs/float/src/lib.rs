pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;
pub mod utils;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;
pub use utils::*;

declare_id!("GUFWB2jfAK5e8QSra1d2XrqrwBCBr7k3f2FwDWr8CE8m");

#[program]
pub mod float {
    use super::*;

    pub fn create_group(
        ctx: Context<CreateGroup>,
        name: String,
        settlement_mint: Pubkey,
    ) -> Result<()> {
        instructions::create_group::handler(ctx, name, settlement_mint)
    }

    pub fn join_group(ctx: Context<JoinGroup>, name: String) -> Result<()> {
        instructions::join_group::handler(ctx, name)
    }

    pub fn log_expense(
        ctx: Context<LogExpense>,
        group_name: String,
        amount: u64,
        description: String,
        shares: Vec<u64>,
    ) -> Result<()> {
        instructions::log_expense::handler(ctx, group_name, amount, description, shares)
    }

    pub fn settle(ctx: Context<Settle>, group_name: String, amount: u64) -> Result<()> {
        instructions::settle::handler(ctx, group_name, amount)
    }

    pub fn enable_pool(ctx: Context<EnablePool>, group_name: String) -> Result<()> {
        instructions::enable_pool::handler(ctx, group_name)
    }

    pub fn deposit(ctx: Context<Deposit>, group_name: String, amount: u64) -> Result<()> {
        instructions::deposit::handler(ctx, group_name, amount)
    }

    pub fn close_group(ctx: Context<CloseGroup>, group_name: String) -> Result<()> {
        instructions::close_group::handler(ctx, group_name)
    }

    pub fn withdraw(ctx: Context<Withdraw>, group_name: String) -> Result<()> {
        instructions::withdraw::handler(ctx, group_name)
    }
}
