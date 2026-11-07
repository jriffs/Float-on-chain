use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};

use crate::{
    constants::KVAULT_PROGRAM_ID,
    error::FloatError,
};

/// Converts a kvault-interface instruction (Solana 2.x types) into
/// Anchor's instruction type and patches Mainnet KVault references
/// to their Devnet equivalents.
pub fn convert_kvault_instruction(
    instruction: solana_instruction::Instruction,
) -> Result<Instruction> {
    let mainnet_program = Pubkey::new_from_array(
        kvault_interface::KVAULT_PROGRAM_ID.to_bytes(),
    );

    let (mainnet_event_authority, _) =
        kvault_interface::pda::event_authority(
            &kvault_interface::KVAULT_PROGRAM_ID,
        );

    let mainnet_event_authority =
        Pubkey::new_from_array(mainnet_event_authority.to_bytes());

    let (devnet_event_authority, _) = Pubkey::find_program_address(
        &[b"__event_authority"],
        &KVAULT_PROGRAM_ID,
    );

    let accounts = instruction
        .accounts
        .into_iter()
        .map(|meta| {
            let mut key = Pubkey::new_from_array(meta.pubkey.to_bytes());

            if key == mainnet_program {
                key = KVAULT_PROGRAM_ID;
            } else if key == mainnet_event_authority {
                key = devnet_event_authority;
            }

            if meta.is_signer && key != Pubkey::default() {
                // Signer privileges are allowed only where the caller
                // explicitly supplies the corresponding signer AccountInfo.
            }

            if meta.is_writable {
                AccountMeta::new(key, meta.is_signer)
            } else {
                AccountMeta::new_readonly(key, meta.is_signer)
            }
        })
        .collect();

    Ok(Instruction {
        program_id: KVAULT_PROGRAM_ID,
        accounts,
        data: instruction.data,
    })
}

/// Invoke a converted KVault instruction.
///
/// `available_accounts` must contain every account referenced by the
/// instruction, including the Devnet KVault executable account.
pub fn invoke_kvault<'info>(
    instruction: &Instruction,
    available_accounts: &[AccountInfo<'info>],
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    let mut ordered_accounts = Vec::with_capacity(
        instruction.accounts.len() + 1,
    );

    for meta in &instruction.accounts {
        let account_info = available_accounts
            .iter()
            .find(|account| account.key() == meta.pubkey)
            .ok_or(FloatError::KaminoAccountMissing)?;

        ordered_accounts.push(account_info.clone());
    }

    let program_info = available_accounts
        .iter()
        .find(|account| account.key() == KVAULT_PROGRAM_ID)
        .ok_or(FloatError::KaminoAccountMissing)?;

    require!(
        program_info.executable,
        FloatError::InvalidKaminoProgram
    );

    ordered_accounts.push(program_info.clone());

    invoke_signed(
        instruction,
        &ordered_accounts,
        signer_seeds,
    )?;

    Ok(())
}