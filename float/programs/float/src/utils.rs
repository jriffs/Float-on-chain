use anchor_lang::prelude::*;

use crate::error::FloatError;
use crate::state::Group;

/// Updates the number of members whose net expense balance is non-zero.
pub fn update_unsettled_member_count(
    group: &mut Group,
    old_balance: i64,
    new_balance: i64,
) -> Result<()> {
    match (old_balance == 0, new_balance == 0) {
        (true, false) => {
            group.unsettled_member_count = group
                .unsettled_member_count
                .checked_add(1)
                .ok_or(FloatError::Overflow)?;
        }
        (false, true) => {
            group.unsettled_member_count = group
                .unsettled_member_count
                .checked_sub(1)
                .ok_or(FloatError::Overflow)?;
        }
        _ => {}
    }

    Ok(())
}