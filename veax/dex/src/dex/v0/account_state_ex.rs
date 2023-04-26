use super::super::errors::{ErrorKind, Result};
use super::super::{AccountV0, AccountWithdrawTracker, Map, MapRemoveKey, Types};
use crate::chain::{Amount, TokenId};
use crate::{ensure_here, error_here};
#[allow(unused)] // Some impls use it, some don't
use num_traits::Zero;

impl<T: Types> AccountV0<T> {
    #[track_caller]
    #[allow(unused)] // Need to use it in `Dex`, to properly check if account can be unregistered
    pub(in super::super) fn ensure_no_withdraw_in_progress(&self) -> Result<()> {
        ensure_here!(
            !self.withdraw_tracker.is_any_withdraw_in_progress(),
            ErrorKind::WithdrawInProgress
        );
        Ok(())
    }

    pub(crate) fn register_token(&mut self, token_id: &TokenId) {
        if !self.token_balances.contains_key(token_id) {
            self.token_balances.insert(token_id.clone(), Amount::zero());
        }
    }

    pub(crate) fn register_tokens<'a>(&mut self, tokens: impl IntoIterator<Item = &'a TokenId>) {
        for token in tokens {
            self.register_token(token);
        }
    }

    fn unregister_token(&mut self, token_id: &TokenId) -> Result<()> {
        if let Some(balance) = self.token_balances.inspect(token_id, |balance| *balance) {
            ensure_here!(balance == Amount::zero(), ErrorKind::NonZeroTokenBalance);
            self.token_balances.remove(token_id);
        }
        Ok(())
    }

    pub(crate) fn unregister_tokens<'a>(
        &mut self,
        tokens: impl IntoIterator<Item = &'a TokenId>,
    ) -> Result<()> {
        for token in tokens {
            ensure_here!(
                !self.withdraw_tracker.is_token_withdraw_in_progress(token),
                ErrorKind::WithdrawInProgress
            );
            self.unregister_token(token)?;
        }
        Ok(())
    }

    pub(crate) fn deposit(
        &mut self,
        token_id: &TokenId,
        amount: Amount,
    ) -> Result<Amount, ErrorKind> {
        self.token_balances
            .try_update(token_id, |balance| match balance.checked_add(amount) {
                Some(new_balance) => {
                    *balance = new_balance;
                    Ok(new_balance)
                }
                None => Err(error_here!(ErrorKind::DepositWouldOverflow)),
            })
            .map_err(|e| e.kind)
    }

    /// Withdraw tokens and return new amount
    pub(crate) fn withdraw(
        &mut self,
        token_id: &TokenId,
        amount: Amount,
    ) -> Result<Amount, ErrorKind> {
        self.token_balances
            .try_update(token_id, |balance| match balance.checked_sub(amount) {
                Some(new_balance) => {
                    *balance = new_balance;
                    Ok(new_balance)
                }
                None => Err(error_here!(ErrorKind::NotEnoughTokens)),
            })
            .map_err(|e| e.kind)
    }
}
