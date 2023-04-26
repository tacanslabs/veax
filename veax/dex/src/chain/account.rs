//! Account deposit is information per user about their balances in the exchange.
use super::Error;
use crate::dex::{self, AccountExtra, Result};
use crate::{ensure_here, error_here};
use near_contract_standards::storage_management::StorageBalance;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::{env, Balance, StorageUsage};

// this constant is derived from tests with a maximum AccoundId length of 64 characters
pub const INIT_ACCOUNT_STORAGE: StorageUsage = 366;
pub const TOKEN_REGISTER_STORAGE: StorageUsage = 284;

pub const CREATE_POOL_STORAGE: StorageUsage = 4457;
pub const OPEN_POSITION_STORAGE: StorageUsage = 1087;

/// Account deposits information and storage cost.
#[derive(Default, BorshSerialize, BorshDeserialize)]
pub struct Extra {
    /// Native NEAR amount sent to the exchange.
    /// Used for storage right now, but in future can be used for trading as well.
    pub near_amount: Balance,
}

impl dex::AccountLatest<super::Types> {
    /// Returns amount of $NEAR necessary to cover storage used by this data structure.
    pub(crate) fn storage_usage(&self) -> Balance {
        u128::from(
            INIT_ACCOUNT_STORAGE
                + self.token_balances.len() as u64 * TOKEN_REGISTER_STORAGE
                + self.positions.len() * OPEN_POSITION_STORAGE,
        ) * env::storage_byte_cost()
    }

    /// Returns how much NEAR is available for storage.
    pub(crate) fn storage_available(&self) -> Balance {
        self.extra.near_amount.saturating_sub(self.storage_usage())
    }

    /// Asserts there is sufficient amount of $NEAR to cover storage usage.
    pub(crate) fn ensure_storage_usage(&self) -> Result<()> {
        ensure_here!(
            self.storage_usage() <= self.extra.near_amount,
            Error::InsufficientStorage
        );
        Ok(())
    }

    /// Returns minimal account deposit storage usage possible.
    pub(crate) fn min_storage_usage() -> Balance {
        u128::from(INIT_ACCOUNT_STORAGE) * env::storage_byte_cost()
    }

    pub(crate) fn storage_balance_of(&self) -> StorageBalance {
        StorageBalance {
            total: U128(self.extra.near_amount),
            available: U128(self.storage_available()),
        }
    }
}

impl AccountExtra for Extra {
    fn on_pool_created(&mut self) -> Result<()> {
        self.near_amount = self
            .near_amount
            .checked_sub(u128::from(CREATE_POOL_STORAGE) * env::storage_byte_cost())
            .ok_or(error_here!(Error::InsufficientStorage))?;
        Ok(())
    }
}
