use super::AccountWithdrawTracker;
use crate::chain::{Amount, TokenId};

#[cfg(feature = "near")]
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
/// Withdraw tracker which is a stub and performs no actual tracking.
/// Usable only in synchronous contexts.
#[cfg_attr(feature = "near", derive(BorshDeserialize, BorshSerialize))]
#[derive(Copy, Clone, Debug, Default)]
pub struct NoopTracker;

impl AccountWithdrawTracker for NoopTracker {
    fn is_any_withdraw_in_progress(&self) -> bool {
        false
    }

    fn is_token_withdraw_in_progress(&self, _token_id: &TokenId) -> bool {
        false
    }
}
/// Simple withdraw tracker which only counts number of pending withdraw operations
#[cfg_attr(feature = "near", derive(BorshDeserialize, BorshSerialize))]
#[derive(Default)]
pub struct CountingTracker(u32);

impl CountingTracker {
    pub fn track(&mut self) {
        self.0 += 1;
    }

    pub fn untrack(&mut self) -> bool {
        if self.0 > 0 {
            self.0 -= 1;
            true
        } else {
            false
        }
    }
}

impl AccountWithdrawTracker for CountingTracker {
    fn is_any_withdraw_in_progress(&self) -> bool {
        self.0 > 0
    }

    fn is_token_withdraw_in_progress(&self, _token_id: &TokenId) -> bool {
        self.0 > 0
    }
}
/// Full withdraw tracker which stores every withdraw as a pair of token id and amount
///
/// Withdraws are stored as a sorted vector, not as on-store key-value container
#[cfg_attr(feature = "near", derive(BorshDeserialize, BorshSerialize))]
#[derive(Default)]
pub struct FullTracker(Vec<(TokenId, Amount)>);

impl FullTracker {
    pub fn track(&mut self, token_id: TokenId, amount: Amount) {
        // We maintain natural sort order, to simplify lookups in future
        let at = match self
            .0
            .binary_search_by_key(&(&token_id, &amount), |(t, a)| (t, a))
        {
            Ok(at) | Err(at) => at,
        };
        self.0.insert(at, (token_id, amount));
    }

    pub fn is_tracked(&self, token_id: &TokenId, amount: &Amount) -> bool {
        self.0
            .binary_search_by_key(&(token_id, amount), |(t, a)| (t, a))
            .is_ok()
    }

    pub fn untrack(&mut self, token_id: &TokenId, amount: &Amount) {
        if let Ok(at) = self
            .0
            .binary_search_by_key(&(token_id, amount), |(t, a)| (t, a))
        {
            self.0.remove(at);
        }
    }
}

impl AccountWithdrawTracker for FullTracker {
    fn is_any_withdraw_in_progress(&self) -> bool {
        !self.0.is_empty()
    }

    fn is_token_withdraw_in_progress(&self, token_id: &TokenId) -> bool {
        // Token id is the first part of key, so all entries are naturally ordered by it
        self.0
            .binary_search_by_key(&token_id, |(tok, _)| tok)
            .is_ok()
    }
}
