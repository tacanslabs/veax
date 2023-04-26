//! Contract's WASM API
//! Helper structures are re-exported through other means,
//! to ensure they're not visible in case of WASM build
use super::log::log_str;
use super::{
    Account, AccountId, AmountInOut, Contract, ContractMetadata, Error, Pair, PoolInfo,
    PositionInfo, RefStorageState, State, StateExt, TokenId,
};
use crate::dex::latest::one_over_sqrt_one_minus_fee_rate;
use crate::dex::{
    self, Action, BasisPoints, FeeLevel, ItemFactory, PositionInit, StateMembersMut, StateMut as _,
    VersionInfo,
};
#[cfg(feature = "smart-routing")]
use crate::dex::{v0::NUM_TOP_POOLS, Path};
use crate::{ensure_here, error_here, Liquidity};
use itertools::Itertools as _;
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};

/// Defines amount type used in WASM entrypoint APIs
pub use near_sdk::json_types::U128 as WasmAmount;
use near_sdk::json_types::{U128, U64};
use near_sdk::{assert_one_yocto, env, near_bindgen, Promise, PromiseOrValue};
use std::collections::HashMap;

/// Extracts promise result and transforms it into normal `Result`
///
/// # Parameter variants
/// * `()` - just checks promise result and returns either `()` or respective error;
///     does not deserialize promise result out of success buffer
/// * `($result_ty)` - checks promise result and either tries to deserialize it into `$result_ty`
///     or returns error
macro_rules! promise_result {
    (@get_result) => {
        if near_sdk::env::promise_results_count() != 1 {
            Err($crate::error_here!($crate::chain::Error::PromiseWrongResultsCount))
        }
        else {
            Ok(())
        }
        .and_then(|()| {
            match near_sdk::env::promise_result(0) {
                near_sdk::PromiseResult::NotReady => Err(
                    error_here!($crate::chain::Error::PromiseNotReady)
                ),
                near_sdk::PromiseResult::Failed => Err(
                    error_here!($crate::chain::Error::PromiseFailed)
                ),
                near_sdk::PromiseResult::Successful(bytes) => Ok(bytes)
            }
        })
    };
    () => {
        promise_result!(@get_result).map(|_| ())
    };
    ($result_ty:ty) => {
        promise_result!(@get_result).and_then(|bytes| {
            near_sdk::serde_json::from_slice::<$result_ty>(bytes.as_ref())
                .map_err(|e| error_here!(
                    $crate::chain::Error::PromiseResultParseFailed(stringify!($result_ty), e)
                ))
        })
    };
}

/// Extension trait which performs `Result` unwrapping through `near_sdk::env::panic_str`
pub(super) trait NearUnwrap<T> {
    fn near_unwrap(self) -> T;
}

impl<T, E: std::fmt::Display> NearUnwrap<T> for std::result::Result<T, E> {
    #[track_caller]
    fn near_unwrap(self) -> T {
        match self {
            Ok(value) => value,
            Err(err) => near_sdk::env::panic_str(&format!("{err}")),
        }
    }
}

impl State {
    /// Produces account registration callback which stores attached NEAR in account's storage
    /// and refunds rest back to caller, if needed.
    ///
    /// Refund happens even if lambda wasn't called
    fn on_register_account(
        &self,
        registration_only: bool,
    ) -> impl FnOnce(
        &AccountId,
        &mut dex::Account<super::Types>,
        /* exists: */ bool,
    ) -> dex::Result<StorageBalance> {
        let min_balance = self.storage_balance_bounds().min.0;

        move |_, Account::V0(ref mut account), already_registered| {
            let deposit = env::attached_deposit();

            ensure_here!(
                deposit >= min_balance || already_registered,
                Error::DepositLessThanMinStorage
            );
            let refund = match (registration_only, already_registered) {
                // Just add amount to account's NEAR balance
                (false, _) => {
                    account.extra.near_amount += deposit;
                    0
                }
                // Supply min balance to account and refund rest
                (true, false) => {
                    let required_deposit = min_balance.saturating_sub(account.extra.near_amount);
                    account.extra.near_amount += required_deposit;
                    deposit - required_deposit
                }
                // Registration only setups the account but doesn't leave space for tokens.
                (true, true) => {
                    log_str("ERR_ACC_REGISTERED");
                    0
                }
            };

            if refund != 0 {
                Promise::new(env::predecessor_account_id()).transfer(refund);
            }

            Ok(account.storage_balance_of())
        }
    }

    fn on_register_account_action(
        &self,
        registration_only: bool,
    ) -> impl FnOnce(
        &AccountId,
        &mut dex::Account<super::Types>,
        /* exists: */ bool,
    ) -> dex::Result<()> {
        let cb = self.on_register_account(registration_only);

        move |id, acc, exists| {
            cb(id, acc, exists)?;
            Ok(())
        }
    }
}

/// Construction of new State instance
#[near_bindgen]
impl State {
    /// # Parameters
    /// - `owner_id` - This account will be allowed to call 'admin' functions of the DEX,
    ///     such as withdrawal of protocol fee or changing protocol fee fraction.
    ///     Normally this should be the governance SC. Defaults to account which deploys SC.
    /// - `protocol_fee_fraction` - Fraction of the total fee, that will go to the DEX.
    ///     The rest of the fee will be distributed among the liquidity providers.
    ///     Specified in units of 1/FEE_DIVISOR. For example, if FEE_DIVISOR
    ///     is 10000, and one wants 13% of the total fee to go to the DEX, one must set
    ///     protocol_fee_fraction = 0.13*10000 = 1300. In such case, if a swap is performed
    ///     on a level with e.g. 0.2% total fee rate, and the total amount paid by the
    ///     trader is e.g. 100000 tokens, then the total charged fee will be 2000 tokens,
    ///     out of which 260 tokens will go to the DEX, and the rest 1740 tokens
    ///     will be distributed among the LPs. Defaults to 1300.
    /// - `fee_rates` - Total fee rates on each of the level (including protocol and LP fees).
    ///     Specified in units of 1/FEE_DIVISOR. For example, if FEE_DIVISOR is 10000,
    ///     and one wants to set fee rates to 0.01%, 0.02%, 0.04%, 0.08%, 0.16%, 0.32%,
    ///     0.64%, 1.28%, on levels 0-7 correspondingly, then one must set
    ///     fee_rates = [1, 2, 4, 8, 16, 32, 64, 128]. Defaults to `[1,2,4,8,16,32,64,128]`.
    #[init]
    pub fn new(
        owner_id: Option<AccountId>,
        protocol_fee_fraction: Option<BasisPoints>,
        fee_rates: Option<dex::latest::RawFeeLevelsArray<BasisPoints>>,
    ) -> Self {
        Self(
            unsafe { &mut super::ITEM_FACTORY }
                .new_contract(
                    owner_id.unwrap_or_else(env::predecessor_account_id),
                    protocol_fee_fraction.unwrap_or(1300),
                    fee_rates.unwrap_or([1, 2, 4, 8, 16, 32, 64, 128]),
                )
                .near_unwrap(),
        )
    }
}

/// Various view methods into contract state
#[near_bindgen]
impl State {
    pub fn metadata(&self) -> ContractMetadata {
        let fee_rates = self.as_dex().fee_rates_ticks();
        let Contract::V0(ref contract) = &self.0;
        ContractMetadata {
            owner: contract.owner_id.clone(),
            pool_count: contract.pool_count,
            protocol_fee_fraction: contract.protocol_fee_fraction,
            fee_rates,
            fee_divisor: dex::BASIS_POINT_DIVISOR,
        }
    }

    /// Returns balances of the deposits for given user outside of any pools.
    /// Returns empty list if no tokens deposited.
    pub fn get_deposits(&self, account_id: &AccountId) -> HashMap<AccountId, U128> {
        let Contract::V0(ref contract) = &self.0;
        contract
            .accounts
            .get(account_id)
            .map(|Account::V0(ref account)| {
                account
                    .token_balances
                    .into_iter()
                    .map(|(token_id, amount)| (token_id, amount.into()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Returns balance of the deposit for given user outside of any pools.
    pub fn get_deposit(&self, account_id: &AccountId, token_id: &AccountId) -> U128 {
        let Contract::V0(ref contract) = &self.0;
        contract
            .accounts
            .get(account_id)
            .and_then(|Account::V0(ref account)| account.token_balances.get(token_id))
            .unwrap_or(0)
            .into()
    }

    /// Get ordered allowed tokens list.
    pub fn get_verified_tokens(&self) -> Vec<AccountId> {
        let Contract::V0(ref contract) = &self.0;
        contract.verified_tokens.iter().collect()
    }

    /// Get specific user tokens.
    pub fn get_user_tokens(&self, account_id: &AccountId) -> Vec<AccountId> {
        let Contract::V0(ref contract) = &self.0;
        contract
            .accounts
            .get(account_id)
            .map(|Account::V0(ref account)| {
                account
                    .token_balances
                    .into_iter()
                    .map(|(token_id, _)| token_id)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_pool_info(&self, tokens: Pair<TokenId>) -> Option<PoolInfo> {
        self.as_dex()
            .get_pool_info(tokens.into())
            .near_unwrap()
            .map(TryInto::try_into)
            .transpose()
            .near_unwrap()
    }

    /// Get user's storage deposit and needed in the account of current version
    pub fn get_user_storage_state(&self, account_id: &AccountId) -> Option<RefStorageState> {
        let Contract::V0(ref contract) = &self.0;
        contract
            .accounts
            .get(account_id)
            .map(|Account::V0(ref account)| RefStorageState {
                deposit: account.extra.near_amount.into(),
                usage: account.storage_usage().into(),
            })
    }

    pub fn get_version(&self) -> VersionInfo {
        self.as_dex().get_version()
    }
}
/// Receive tokens from other contracts
#[near_bindgen]
impl FungibleTokenReceiver for State {
    /// Callback on receiving tokens by this contract
    ///
    /// # Parameters
    /// * `sender_id` - original owner of tokens
    /// * `amount` - how many tokens to deposit
    /// * `msg` - additional transfer payload; if empty, performs simple deposit;
    ///     otherwise should contain vector of actions serialized as JSON;
    ///     see `execute_actions` on required format
    #[allow(unreachable_code)]
    #[payable]
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        // Token id is the caller here
        let token_in = env::predecessor_account_id();
        let remainder = U128(0);
        // Diverge based on message contents
        if msg.is_empty() {
            self.as_dex_mut()
                .deposit(&sender_id, &token_in, amount.into())
                .near_unwrap();

            PromiseOrValue::Value(remainder)
        } else {
            let actions: Vec<Action<()>> = serde_json::from_str(&msg)
                .map_err(|e| error_here!(Error::WrongMsgFormat(e)))
                .near_unwrap();
            let outcomes = self
                .as_dex_mut()
                .deposit_execute_actions(
                    &sender_id,
                    token_in,
                    amount.into(),
                    |_, _, _| Err(error_here!(Error::RegisterAccountNotAllowedOnDeposit)),
                    actions,
                )
                .near_unwrap();

            match Self::fold_promises(outcomes) {
                Some(p) => PromiseOrValue::Promise(
                    p.then(Self::delay_return_option_amount(Some(remainder))),
                ),
                None => PromiseOrValue::Value(remainder),
            }
        }
    }
}
/// Operations over accounts
#[near_bindgen]
impl State {
    /// Registers given token in the user's account deposit.
    /// Fails if not enough balance on this account to cover storage.
    #[payable]
    pub fn register_tokens(&mut self, token_ids: &Vec<AccountId>) {
        assert_one_yocto();
        let account_id = env::predecessor_account_id();
        self.as_dex_mut()
            .register_tokens(&account_id, token_ids)
            .near_unwrap();
    }
    /// Unregister given token from user's account deposit.
    /// Panics if the balance of any given token is non 0.
    #[payable]
    pub fn unregister_tokens(&mut self, token_ids: &Vec<AccountId>) {
        assert_one_yocto();
        self.as_dex_mut()
            .unregister_tokens(&env::predecessor_account_id(), token_ids)
            .near_unwrap();
    }

    pub fn token_register_of(&self, account_id: &AccountId, token_id: &AccountId) -> bool {
        let Contract::V0(ref contract) = &self.0;
        contract
            .accounts
            .get(account_id)
            .map_or(false, |Account::V0(ref account)| {
                account.token_balances.get(token_id).is_some()
            })
    }
    /// Withdraws given token from the deposits of given user.
    /// a zero amount means to withdraw all in user's inner account.
    ///
    /// # Parameters
    /// * `token_id` - token which should be withdrawn
    /// * `amount` - amount to withdraw
    /// * `unregister` - if `Ok(true)`, DEX will attempt to unregister token from account.
    ///     Unregister will succeed only if named token has zero balance, but failure to do so will not
    ///     interrupt operation as a whole
    ///
    /// # Returns
    /// Promise which produces final operation result
    #[payable]
    #[allow(clippy::needless_pass_by_value)] // token_id is passed by value, dictated by existing API
    pub fn withdraw(
        &mut self,
        token_id: AccountId,
        amount: U128,
        unregister: Option<bool>,
    ) -> Option<Promise> {
        assert_one_yocto();
        self.as_dex_mut()
            .withdraw(
                &env::predecessor_account_id(),
                &token_id,
                amount.into(),
                unregister.unwrap_or(false),
                (),
            )
            .near_unwrap()
    }

    #[private]
    pub fn exchange_callback_post_withdraw(
        &mut self,
        token_id: &AccountId,
        sender_id: &AccountId,
        amount: U128,
        unregister: bool,
    ) {
        let succeeded = promise_result!().is_ok();

        let mut dex = self.as_dex_mut();
        let StateMembersMut {
            contract: Contract::V0(ref mut contract),
            logger,
            ..
        } = dex.members_mut();

        contract
            .accounts
            .try_update(sender_id, |dex::Account::V0(acc)| {
                let amount = amount.into();
                // Untrack in any case
                acc.withdraw_tracker.untrack(token_id, &amount);

                if succeeded {
                    // And try unregister token - but don't fail if we can't do it
                    if unregister {
                        let _ignore = acc.unregister_tokens([token_id]);
                    }
                } else {
                    // If we fail, return tokens back to balance
                    let balance = acc.deposit(token_id, amount).map_err(|e| error_here!(e))?;
                    logger.log_deposit_event(sender_id, token_id, &amount, &balance);
                }

                Ok(())
            })
            .near_unwrap();
    }
    // Just return value passed in. Used to complete async withdrawals with value
    #[private]
    pub fn return_option_amount(&self, amount: Option<WasmAmount>) -> Option<WasmAmount> {
        amount
    }
}
/// Owner-only APIs
#[near_bindgen]
impl State {
    /// Get the owner of this account.
    pub fn get_owner(&self) -> AccountId {
        let Contract::V0(ref contract) = &self.0;
        contract.owner_id.clone()
    }

    /// Extend verified tokens with new tokens. Only can be called by owner.
    #[payable]
    pub fn extend_verified_tokens(&mut self, tokens: Vec<TokenId>) {
        self.as_dex_mut().add_verified_tokens(tokens).near_unwrap();
    }

    /// Remove verified token. Only can be called by owner.
    #[payable]
    pub fn remove_verified_tokens(&mut self, tokens: Vec<TokenId>) {
        self.as_dex_mut()
            .remove_verified_tokens(tokens)
            .near_unwrap();
    }

    /// Extend guard accounts with new accounts. Only can be called by owner.
    #[payable]
    pub fn extend_guard_accounts(&mut self, accounts: Vec<AccountId>) {
        self.as_dex_mut().add_guard_accounts(accounts).near_unwrap();
    }

    /// Remove guard accounts. Only can be called by owner.
    #[payable]
    pub fn remove_guard_accounts(&mut self, accounts: Vec<AccountId>) {
        self.as_dex_mut()
            .remove_guard_accounts(accounts)
            .near_unwrap();
    }

    /// Suspend payable API calls. It can be done by owner or by guards.
    #[payable]
    pub fn suspend_payable_api(&mut self) {
        self.as_dex_mut().suspend_payable_api().near_unwrap();
    }

    /// Resume payable API calls. It can be done by owner or by guards.
    #[payable]
    pub fn resume_payable_api(&mut self) {
        self.as_dex_mut().resume_payable_api().near_unwrap();
    }

    /// Fraction of the fee which goes to the DEX out of the total fee charged in swaps.
    /// In units of 1/FEE_DIVISOR
    #[payable]
    pub fn set_protocol_fee_fraction(&mut self, protocol_fee_fraction: BasisPoints) {
        assert_one_yocto();
        self.as_dex_mut()
            .set_protocol_fee_fraction(protocol_fee_fraction)
            .near_unwrap();
    }

    /// Withdraw owner inner account token to owner wallet.
    /// Owner inner account should be prepared in advance.
    #[payable]
    #[allow(clippy::needless_pass_by_value)]
    pub fn withdraw_owner_token(&mut self, token_id: AccountId, amount: U128) -> Promise {
        assert_one_yocto();
        self.as_dex_mut()
            .owner_withdraw(&token_id, amount.into(), ())
            .near_unwrap()
    }

    /// Withdraw protocol fee onto the dex-owner account on the dex.
    #[payable]
    pub fn withdraw_protocol_fee(&mut self, pool_id: (TokenId, TokenId)) -> (U128, U128) {
        assert_one_yocto();
        let fee_amounts = self
            .as_dex_mut()
            .withdraw_protocol_fee(pool_id)
            .near_unwrap();
        (fee_amounts.0.into(), fee_amounts.1.into())
    }
}
/// Storage APIs
#[near_bindgen]
impl StorageManagement for State {
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        let account_id = account_id.unwrap_or_else(env::predecessor_account_id);
        let register_cb = self.on_register_account(registration_only.unwrap_or(false));

        self.as_dex_mut()
            .register_account_and_then(account_id, register_cb)
            .near_unwrap()
    }

    #[payable]
    fn storage_withdraw(&mut self, amount: Option<U128>) -> StorageBalance {
        assert_one_yocto();
        let account_id = env::predecessor_account_id();
        let amount = amount.unwrap_or(U128(0)).0;
        let mut dex = self.as_dex_mut();
        dex.ensure_payable_api_resumed().near_unwrap();
        let Contract::V0(ref mut contract) = dex.contract_mut();
        let (withdraw_amount, storage_balance) = contract
            .accounts
            .try_update(&account_id, |Account::V0(ref mut account)| {
                let available = account.storage_available();
                ensure_here!(available > 0, Error::NoStorageCanWithdraw);
                let withdraw_amount = if amount == 0 { available } else { amount };
                ensure_here!(withdraw_amount <= available, Error::StorageWithdrawTooMuch);
                account.extra.near_amount -= withdraw_amount;
                Ok((withdraw_amount, account.storage_balance_of()))
            })
            .near_unwrap();
        Promise::new(account_id).transfer(withdraw_amount);
        storage_balance
    }

    #[allow(unused_variables)]
    #[payable]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        assert_one_yocto();

        self.as_dex_mut()
            .unregister_account_with_cb(None, |_, dex::Account::V0(acc)| Ok(acc.extra.near_amount))
            .near_unwrap()
            .map(|balance| {
                Promise::new(env::predecessor_account_id()).transfer(balance);
            })
            .is_some()
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        StorageBalanceBounds {
            min: dex::AccountLatest::min_storage_usage().into(),
            max: None,
        }
    }

    fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        let Contract::V0(ref contract) = &self.0;
        contract
            .accounts
            .get(&account_id)
            .map(|Account::V0(ref account)| account.storage_balance_of())
    }
}
/// Pools manipulation
#[near_bindgen]
impl State {
    /// Executes generic set of actions.
    #[payable]
    #[allow(clippy::ptr_arg)]
    pub fn execute_actions(
        &mut self,
        actions: Vec<Action<()>>,
    ) -> PromiseOrValue<Option<WasmAmount>> {
        let register_cb = self.on_register_account_action(false);
        let (outcomes, amount) = self
            .as_dex_mut()
            .execute_actions(register_cb, actions)
            .near_unwrap();

        let amount = amount.map(Into::into);

        match Self::fold_promises(outcomes) {
            Some(p) => PromiseOrValue::Promise(p.then(Self::delay_return_option_amount(amount))),
            None => PromiseOrValue::Value(amount),
        }
    }
    /// Execute set of swap actions between pools.
    #[payable]
    #[allow(clippy::ptr_arg)]
    pub fn swap_exact_in(
        &mut self,
        tokens: &Vec<AccountId>,
        amount_in: U128,
        min_amount_out: U128,
    ) -> AmountInOut {
        let (amount_in, amount_out) = self
            .as_dex_mut()
            .swap_exact_in(tokens, amount_in.into(), min_amount_out.into())
            .near_unwrap();
        AmountInOut {
            amount_in: amount_in.into(),
            amount_out: amount_out.into(),
        }
    }

    #[payable]
    #[allow(clippy::ptr_arg)]
    pub fn swap_exact_out(
        &mut self,
        tokens: &Vec<AccountId>,
        amount_out: U128,
        max_amount_in: U128,
    ) -> AmountInOut {
        let (amount_in, amount_out) = self
            .as_dex_mut()
            .swap_exact_out(tokens, amount_out.into(), max_amount_in.into())
            .near_unwrap();
        AmountInOut {
            amount_in: amount_in.into(),
            amount_out: amount_out.into(),
        }
    }

    /// Execute set of swap actions between pools with multiple paths.
    #[cfg(feature = "smart-routing")]
    #[payable]
    #[allow(clippy::ptr_arg)]
    pub fn multiple_path_swap_exact_in(
        &mut self,
        paths: &Vec<Path>,
        min_amount_out: U128,
    ) -> Vec<AmountInOut> {
        self.as_dex_mut()
            .multiple_path_swap_exact_in(paths, min_amount_out.into())
            .near_unwrap()
            .into_iter()
            .map(|(amount_in, amount_out)| AmountInOut {
                amount_in: amount_in.into(),
                amount_out: amount_out.into(),
            })
            .collect()
    }

    #[cfg(feature = "smart-routing")]
    #[payable]
    #[allow(clippy::ptr_arg)]
    pub fn multiple_path_swap_exact_out(
        &mut self,
        paths: &Vec<Path>,
        max_amount_in: U128,
    ) -> Vec<AmountInOut> {
        self.as_dex_mut()
            .multiple_path_swap_exact_out(paths, max_amount_in.into())
            .near_unwrap()
            .into_iter()
            .map(|(amount_in, amount_out)| AmountInOut {
                amount_in: amount_in.into(),
                amount_out: amount_out.into(),
            })
            .collect()
    }

    /// Open a new position, i.e. deposit tokens in the pool as a liquidity provider.
    /// Tokens must be deposited in the ratio of the current spot price on the current fee level.
    /// As the actual spot price may slip, the caller has to specify the ranges of token_a
    /// and token_b amounts to be deposited.
    ///  - token_a: first token identifying the pool; tokens may be in arbitrary order.
    ///  - token_b: second token identifying the pool; tokens may be in arbitrary order.
    ///  - fee: fee rate in units of 1/FEE_DIVISOR
    ///  - position: currently only FullRangePosition is suported. That is the ranges
    ///     of token amounts to be depsoted. TODO: provide example JSON.
    ///
    /// Attached NEAR should be enough to cover the added storage.
    /// If pool doesn't exist, it is implicitly created. In such case more attached NEAR is required.
    /// As pool can never be deleted, the NEAR deposited for pool storage can not be withdrawn.
    ///
    /// Returns:
    ///  - position_id
    ///  - amount of token a
    ///  - amount of token b
    ///  - accounted liquidity
    ///
    /// Notice that actual accounted position liqudity may be smaller sqrt(amount_a, amount_b)
    ///
    #[payable]
    pub fn open_position(
        &mut self,
        token_a: &AccountId,
        token_b: &AccountId,
        fee_rate: dex::BasisPoints,
        position: PositionInit,
    ) -> (U64, U128, U128, f64) {
        // TODO: returned liquidity is rounded. consider returning in full precision

        assert_one_yocto();

        let (position_id, amount_a, amount_b, net_liquidity) = self
            .as_dex_mut()
            .open_position(token_a, token_b, fee_rate, position)
            .near_unwrap();

        let fee_level: FeeLevel = self
            .as_dex()
            .fee_rates_ticks()
            .iter()
            .find_position(|rate| **rate == fee_rate)
            .ok_or(Error::InvalidArgument)
            .near_unwrap()
            .0
            .try_into()
            .near_unwrap();

        let liquidity = net_liquidity
            * Liquidity::try_from(one_over_sqrt_one_minus_fee_rate(fee_level)).near_unwrap();

        // TODO: Review output type:
        let liquidity: f64 = crate::chain::Float::from(liquidity).into();

        (
            position_id.into(),
            amount_a.into(),
            amount_b.into(),
            liquidity,
        )
    }

    #[payable]
    pub fn close_position(&mut self, position_id: U64) {
        assert_one_yocto();

        self.as_dex_mut()
            .close_position(position_id.into())
            .near_unwrap();
    }

    #[payable]
    pub fn withdraw_fee(&mut self, position_id: U64) -> Pair<U128> {
        assert_one_yocto();
        self.as_dex_mut()
            .withdraw_fee(position_id.into())
            .near_unwrap()
            .into()
    }

    pub fn get_position_info(&self, position_id: U64) -> PositionInfo {
        self.as_dex()
            .get_position_info(position_id.into())
            .near_unwrap()
            .into()
    }

    #[cfg(feature = "smart-routing")]
    pub fn update_top_pools(&mut self) -> HashMap<TokenId, [TokenId; NUM_TOP_POOLS]> {
        self.as_dex_mut().update_top_pools().unwrap()
    }

    #[cfg(feature = "smart-routing")]
    pub fn get_token_top_pools(&self, token: &TokenId) -> [TokenId; NUM_TOP_POOLS] {
        self.as_dex().get_token_top_pools(token).unwrap()
    }

    #[cfg(feature = "smart-routing")]
    pub fn calculate_path_liquidity(&self, token_id_vec: &Vec<TokenId>) -> Liquidity {
        self.as_dex()
            .calculate_path_liquidity(token_id_vec.as_slice())
            .unwrap()
    }
}
