//! NEAR blockchain implementation
use crate::dex::collection_helpers::{PairKeyIter, StorageRef, StorageRefIter, StorageRefPairIter};
use crate::dex::{self, KeyAt, Map, PoolId, Result};
use events::{log_storage_balance_event, Logger};
use near_contract_standards::fungible_token::core::ext_ft_core;
use near_contract_standards::storage_management::{StorageBalance, StorageBalanceBounds};
use near_iterable_maps::{DoublyLinkedListMap, LinkedListMap};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{TreeMap, UnorderedMap, UnorderedSet};
use near_sdk::Promise;
use near_sdk::{env, ext_contract, json_types::U128, near_bindgen, Gas, PanicOnDefault};
use std::ops::{Deref, DerefMut};
use thiserror::Error;

use crate::dex::tick::Tick;
use crate::dex::TickState;
use crate::fp::U128X128;
pub use account::{
    CREATE_POOL_STORAGE, INIT_ACCOUNT_STORAGE, OPEN_POSITION_STORAGE, TOKEN_REGISTER_STORAGE,
};
pub use pairs::Pair;
pub use types::*;

pub type AccountId = near_sdk::AccountId;
pub type TokenId = near_sdk::AccountId;
pub type UInt = u128;
pub type UIntBig = crate::fp::U256;
pub type FixedPoint = crate::fp::U128X128;
pub type FixedPointBig = crate::fp::U192X192;
pub type FixedPointSigned = crate::fp::I192X64;
pub type Amount = UInt;
pub type AmountUFP = crate::fp::U256X256;
pub type AmountSFP = crate::fp::I256X256;
pub type Liquidity = crate::fp::U192X64; // TODO: rename to LiquidityUFP
pub type LiquiditySFP = crate::fp::I192X64;
pub type NetLiquidityUFP = crate::fp::U192X64;
pub type NetLiquiditySFP = crate::fp::I192X64;
pub type GrossLiquidityUFP = crate::fp::U192X192; // TODO: Replace with U192X128
pub type FeeLiquidityUFP = crate::fp::U192X192; // TODO: Replace with U192X128
pub type LPFeePerFeeLiquidity = crate::fp::I128X128;
pub type SqrtpriceUFP = crate::fp::U128X128; // TODO: Replace with U64X128
pub type SqrtpriceSFP = crate::fp::I128X128; // TODO: Replace with I64X128
pub type AccSqrtpriceSFP = crate::fp::I128X128;

mod account;
mod events;
mod pairs;
mod types;
mod utils;

pub mod log;
pub mod wasm;

use crate::dex::latest::NUM_FEE_LEVELS;
pub use dex::describe_error_code;

use self::wasm::NearUnwrap;
/// FIXME: deduce actual amount required for just returning value
const GAS_FOR_RETURN_AMOUNT: Gas = Gas(1_000_000_000);
/// `hotfix_insuffient_gas_for_mft_resolve_transfer`.
const GAS_FOR_RESOLVE_TRANSFER: Gas = Gas(20_000_000_000_000);
/// Amount of gas for fungible token transfers, increased to 20T to support AS token contracts.
const GAS_FOR_FT_TRANSFER: Gas = Gas(20_000_000_000_000);
/// Maximum value for price tick
pub const MAX_TICK: i32 = 887_273;
/// Minimum value for price tick
pub const MIN_TICK: i32 = -887_273;

pub const MIN_EFF_TICK: i32 = MIN_TICK - 2i32.pow(NUM_FEE_LEVELS as u32 - 1);
pub const MAX_EFF_TICK: i32 = MAX_TICK + 2i32.pow(NUM_FEE_LEVELS as u32 - 1);

/// Number of precalculated ticks
pub const NUM_PRECALCULATED_TICKS: usize = 20;

crate::wrap_float! {
    #[derive(BorshDeserialize, BorshSerialize)]
    pub f64 {
        MANTISSA_BITS: f64::MANTISSA_DIGITS - 1,
        MAX: f64::MAX,
        zero: 0.0,
        one: 1.0,
        cmp: |l, r| l.partial_cmp(r),
        classify: |v| v.classify(),
        add: |l, r| l + r,
        sub: |l, r| l - r,
        mul: |l, r| l * r,
        div: |l, r| l / r,
        rem: |l, r| l % r,
        sqrt: |v| v.sqrt(),
        round: |v| v.round(),
        floor: |v| v.floor(),
        ceil: |v| v.ceil(),
        from u64: |v| {
            #[allow(clippy::cast_precision_loss)]
            fn u64_as_f64(value: u64) -> f64 {
                value as f64
            }
            u64_as_f64(v)
        },
        from BasisPoints: |bp| f64::from(bp),
        from UInt: |value| {
            #[allow(clippy::cast_precision_loss)]
            fn u128_as_f64(value: u128) -> f64 {
                value as f64
            }
            u128_as_f64(value)
        },
        try_into UInt: |value| {
            use crate::fp;
            #[allow(clippy::cast_precision_loss)]
            fn u128_as_f64(value: u128) -> f64 {
                value as f64
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            fn f64_as_u128(value: f64) -> u128 {
                value as u128
            }
            if value.is_nan() {
                Err(fp::Error::NaN)
            } else if value > u128_as_f64(u128::MAX) {
                Err(fp::Error::Overflow)
            } else if value.is_sign_negative() {
                Err(fp::Error::NegativeToUnsigned)
            } else {
                Ok(f64_as_u128(value))
            }
        },
        integer_decode: |value| num_traits::Float::integer_decode(value),
        try_into_lossy FixedPoint: |v|
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::float_cmp
        )]
        {
            use crate::fp;
            const TWO_POW_64: f64 = (1_u128 << 64) as f64;
            const TWO_POW_128: f64 = TWO_POW_64 * TWO_POW_64;

            if v.is_nan() {
                Err(fp::Error::NaN)
            } else if v.is_sign_negative() {
                Err(fp::Error::NegativeToUnsigned)
            } else if v >= TWO_POW_128 {
                // Over 128 integer bits
                Err(fp::Error::Overflow)
            } else {
                // Fits into fixed-point, possibly with precision loss
                let int_part = v.trunc() as u128;
                // Get fraction and move it into integral
                let fract_part = v.fract() * TWO_POW_128;
                let (fract_part, _loss_part) = (fract_part.trunc() as u128, fract_part.fract());
                Ok(U128X128::from([
                        fract_part as u64,
                        (fract_part >> 64) as u64,
                        int_part as u64,
                        (int_part >> 64) as u64,
                    ]))
            }
        },
    }
}

#[allow(clippy::cast_precision_loss)]
pub const FLOAT_TWO_POW_64: Float = Float((1u128 << 64) as f64);

#[allow(clippy::cast_precision_loss)]
pub const FLOAT_TWO_POW_128: Float = Float(((1u128 << 64) as f64) * ((1u128 << 64) as f64));

crate::custom_error! {
    pub enum Error {
        #[error("Insufficient $NEAR storage deposit")]
        InsufficientStorage,
        #[error("No storage, cannot withdraw")]
        NoStorageCanWithdraw,
        #[error("Storage withdraw too much")]
        StorageWithdrawTooMuch,
        #[error("Deposit less than min storage")]
        DepositLessThanMinStorage,
        #[error("Insufficient storage deposit: needed {needed} but attached {attached}")]
        NotEnoughStorageDeposit { needed: Amount, attached: Amount },
        #[error("Requires attached deposit of at least 1 yoctoNEAR")]
        AtLeastOneYocto,
        #[error("Invalid argument")]
        InvalidArgument,
        #[error("Wrong message format. Message passed to `ft_on_transfer` must be either empty string or JSON-encoded list of actions. Parsing error: {0}")]
        WrongMsgFormat(near_sdk::serde_json::Error),
        #[error("`RegisterAccount` action isn't allowed during token transfer - no way to provide storage deposit")]
        RegisterAccountNotAllowedOnDeposit,
        #[error("Received incorrect number of result values from previous async operation")]
        PromiseWrongResultsCount,
        #[error("Previous async operation is not ready")]
        PromiseNotReady,
        #[error("Previous async operation failed")]
        PromiseFailed,
        #[error("Could not parse result of previous async operation as {0}: parse error {1}")]
        PromiseResultParseFailed(&'static str, near_sdk::serde_json::Error),
    }
}

pub struct Types;

#[cfg(test)]
pub type TestTypes = Types;

pub type Contract = dex::Contract<Types>;
pub type Account = dex::Account<Types>;
pub type Pool = dex::Pool<Types>;
pub type Position = dex::Position<Types>;

impl dex::Types for Types {
    type Bound = ();
    type ContractExtra = ();
    type AccountsMap = AccountsMap;
    type TickStatesMap = TreeMap<Tick, TickState<Types>>;
    type AccountTokenBalancesMap = DoublyLinkedListMap<AccountId, Amount>;
    type AccountWithdrawTracker = dex::withdraw_trackers::FullTracker;
    type AccountExtra = account::Extra;
    type PoolsMap = LinkedListMap<PoolId, Pool>;
    type PoolPositionsMap = DoublyLinkedListMap<dex::PositionId, Position>;
    type AccountPositionsSet = UnorderedSet<dex::PositionId>;
    type VerifiedTokensSet = UnorderedSet<TokenId>;
    type PositionToPoolMap = DoublyLinkedListMap<dex::PositionId, PoolId>;
    type AccountIdSet = UnorderedSet<AccountId>;
    #[cfg(feature = "smart-routing")]
    type TokenConnectionsMap = DoublyLinkedListMap<TokenId, Self::TokensSet>;
    #[cfg(feature = "smart-routing")]
    type TokensSet = UnorderedSet<TokenId>;
    #[cfg(feature = "smart-routing")]
    type TokensArraySet = UnorderedSet<TokenId>;
    #[cfg(feature = "smart-routing")]
    type TopPoolsMap = DoublyLinkedListMap<TokenId, Self::TokensArraySet>;
}

#[near_bindgen]
#[derive(BorshSerialize, BorshDeserialize, PanicOnDefault)]
pub struct State(Contract);

impl dex::State<Types> for State {
    fn contract(&self) -> &Contract {
        &self.0
    }
}

static mut LOGGER: Logger = Logger;

impl State {
    fn delay_return_option_amount(amount: Option<wasm::WasmAmount>) -> near_sdk::Promise {
        ext_self::ext(env::current_account_id())
            .with_attached_deposit(0u128)
            .with_static_gas(GAS_FOR_RETURN_AMOUNT)
            .return_option_amount(amount)
    }

    fn fold_promises(
        promises: impl IntoIterator<Item = near_sdk::Promise>,
    ) -> Option<near_sdk::Promise> {
        promises.into_iter().reduce(near_sdk::Promise::and)
    }
}

impl dex::StateMut<Types> for State {
    type SendTokensResult = near_sdk::Promise;
    type SendTokensExtraParam = ();

    fn members_mut(&mut self) -> dex::StateMembersMut<'_, Types> {
        dex::StateMembersMut {
            contract: &mut self.0,
            // Actually safe - ItemFactory is zero-sized
            item_factory: unsafe { &mut ITEM_FACTORY },
            // Actually safe - Logger is zero-sized
            logger: unsafe { &mut LOGGER },
        }
    }

    fn send_tokens(
        &mut self,
        account_id: &AccountId,
        token_id: &TokenId,
        amount: Amount,
        unregister: bool,
        _extra: Self::SendTokensExtraParam,
    ) -> Self::SendTokensResult {
        // Event logging and deregistration should be handled by callback,
        // here we only start tracking
        let dex::Contract::V0(ref mut contract) = self.contract_mut();

        contract
            .accounts
            .try_update(account_id, |dex::Account::V0(acc)| {
                acc.withdraw_tracker.track(token_id.clone(), amount);
                Ok(())
            })
            .near_unwrap();
        // Token transfer
        let builder_ft_core_ext = ext_ft_core::ext(token_id.clone())
            .with_attached_deposit(1u128)
            .with_static_gas(GAS_FOR_FT_TRANSFER);
        // Result callback
        let builder_ext_self = ext_self::ext(env::current_account_id())
            .with_attached_deposit(0u128)
            .with_static_gas(GAS_FOR_RESOLVE_TRANSFER);
        // Finally, promise chain
        builder_ft_core_ext
            .ft_transfer(account_id.clone(), U128::from(amount), None)
            .then(builder_ext_self.exchange_callback_post_withdraw(
                token_id.clone(),
                account_id.clone(),
                U128::from(amount),
                unregister,
            ))
    }

    fn get_initiator_id(&self) -> AccountId {
        env::signer_account_id()
    }

    fn get_caller_id(&self) -> AccountId {
        env::predecessor_account_id()
    }
}
/// Internal methods implementation.
impl State {
    pub fn as_dex(&self) -> dex::Dex<Types, Self, &Self> {
        dex::Dex::new(self)
    }

    pub fn as_dex_mut(&mut self) -> dex::Dex<Types, Self, &mut Self> {
        dex::Dex::new(self)
    }
}

/// Serves as newtype wrapper, to have different implementation of `dex::Map` trait
#[derive(BorshSerialize, BorshDeserialize)]
pub struct AccountsMap(DoublyLinkedListMap<AccountId, Account>);

impl Deref for AccountsMap {
    type Target = DoublyLinkedListMap<AccountId, Account>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for AccountsMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl dex::Map for AccountsMap {
    type Key = AccountId;
    type Value = Account;
    type KeyRef<'a> = StorageRef<'a, AccountId> where Self: 'a;
    type ValueRef<'a> = StorageRef<'a, Account> where Self: 'a;
    type Iter<'a> = DoublyLinkedListMapIter<'a, AccountId, Account> where Self: 'a;

    fn iter(&self) -> Self::Iter<'_> {
        dex::Map::iter(&self.0)
    }

    fn clear(&mut self) {
        self.0.clear();
    }

    #[allow(clippy::cast_possible_truncation)]
    fn len(&self) -> usize {
        self.0.len() as usize
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn contains_key(&self, key: &AccountId) -> bool {
        self.0.contains_key(key)
    }

    fn inspect<R, F: FnOnce(&Account) -> R>(&self, key: &AccountId, inspect_fn: F) -> Option<R> {
        self.0.get(key).map(|acc| inspect_fn(&acc))
    }

    fn update<R, F: FnOnce(&mut Account) -> Result<R>>(
        &mut self,
        key: &AccountId,
        update_fn: F,
    ) -> Option<Result<R>> {
        self.get(key).map(|mut value| {
            update_fn(&mut value)
                .and_then(|result| save_account(&mut self.0, key, value).map(|()| result))
        })
    }

    fn update_or_insert<R, F, U>(
        &mut self,
        key: &AccountId,
        factory_fn: F,
        update_fn: U,
    ) -> Result<R>
    where
        F: FnOnce() -> Result<Account>,
        U: FnOnce(&mut Account, /* exists */ bool) -> Result<R>,
    {
        let (value, exists) = self
            .get(key)
            .map_or_else(|| (factory_fn(), false), |value| (Ok(value), true));
        let mut value = value?;
        let result = update_fn(&mut value, exists)?;
        save_account(&mut self.0, key, value)?;
        Ok(result)
    }

    fn insert(&mut self, key: AccountId, value: Account) {
        self.0.insert(&key, value);
    }
}

impl dex::MapRemoveKey for AccountsMap {
    fn remove(&mut self, key: &AccountId) {
        self.0.remove(key);
    }
}

fn save_account(
    map: &mut DoublyLinkedListMap<AccountId, Account>,
    account_id: &AccountId,
    account: Account,
) -> Result<()> {
    let storage_deposit_total;
    let storage_available;
    {
        let dex::Account::V0(ref account_v0) = account;
        account_v0.ensure_storage_usage()?;
        storage_deposit_total = account_v0.extra.near_amount;
        storage_available = account_v0.storage_available();
    }
    map.insert(account_id, account);
    log_storage_balance_event(account_id, &storage_available, &storage_deposit_total);
    Ok(())
}

#[ext_contract(ext_self)]
trait Exchange {
    fn exchange_callback_post_withdraw(
        &mut self,
        token_id: AccountId,
        sender_id: AccountId,
        amount: U128,
        unregister: bool,
    );

    fn return_option_amount(&self, amount: Option<wasm::WasmAmount>) -> Option<wasm::WasmAmount>;

    fn do_wnear_register(&mut self) -> Promise;

    fn finish_wnear_register(&mut self);
}

#[ext_contract(ext_wrap_near)]
trait WrapNear {
    #[payable]
    fn near_deposit(&mut self);

    #[payable]
    fn near_withdraw(&mut self, amount: U128) -> Promise;
    /// Method from StorageManagement, needed on init
    fn storage_balance_bounds(&self) -> StorageBalanceBounds;
    /// Method from StorageManagement, needed on init
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance;
}

pub struct ItemFactory;
static mut ITEM_FACTORY: ItemFactory = ItemFactory;

impl ItemFactory {
    /// Generates next fixed-size key prefix for collection being created
    fn next_prefix() -> Vec<u8> {
        // Key for entry where prefix counter is stored
        const NEXT_PREFIX_KEY: [u8; 8] = 0u64.to_le_bytes();
        let next_prefix: u64 = if let Some(bytes) = env::storage_read(&NEXT_PREFIX_KEY) {
            let Ok(next) = u64::deserialize(&mut bytes.as_ref()) else {
                // If we get here, contract state is irreversibly broken anyway
                unreachable!()
            };
            next
        } else {
            1
        };
        env::storage_write(&NEXT_PREFIX_KEY, &(next_prefix + 1).to_le_bytes());
        next_prefix.to_le_bytes().to_vec()
    }

    fn new_set<I>() -> UnorderedSet<I> {
        UnorderedSet::new(Self::next_prefix())
    }

    #[allow(unused)]
    fn new_map<K, V>() -> UnorderedMap<K, V> {
        UnorderedMap::new(Self::next_prefix())
    }

    fn new_ordered_map<
        K: Ord + Clone + BorshSerialize + BorshDeserialize,
        V: BorshSerialize + BorshDeserialize,
    >() -> TreeMap<K, V> {
        TreeMap::new(Self::next_prefix())
    }

    fn new_linked_list_map<
        K: Ord + Clone + BorshSerialize + BorshDeserialize,
        V: BorshSerialize + BorshDeserialize,
    >() -> LinkedListMap<K, V> {
        LinkedListMap::new(Self::next_prefix())
    }

    fn new_doubly_linked_list_map<
        K: Ord + Clone + BorshSerialize + BorshDeserialize,
        V: BorshSerialize + BorshDeserialize,
    >() -> DoublyLinkedListMap<K, V> {
        DoublyLinkedListMap::new(Self::next_prefix())
    }
}

impl dex::ItemFactory<Types> for ItemFactory {
    fn new_accounts_map(&mut self) -> <Types as dex::Types>::AccountsMap {
        AccountsMap(Self::new_doubly_linked_list_map())
    }

    fn new_tick_states_map(&mut self) -> <Types as dex::Types>::TickStatesMap {
        Self::new_ordered_map()
    }

    fn new_account_token_balances_map(&mut self) -> <Types as dex::Types>::AccountTokenBalancesMap {
        Self::new_doubly_linked_list_map()
    }

    fn new_account_withdraw_tracker(&mut self) -> <Types as dex::Types>::AccountWithdrawTracker {
        dex::withdraw_trackers::FullTracker::default()
    }

    fn new_pools_map(&mut self) -> <Types as dex::Types>::PoolsMap {
        Self::new_linked_list_map()
    }

    fn new_pool_positions_map(&mut self) -> <Types as dex::Types>::PoolPositionsMap {
        Self::new_doubly_linked_list_map()
    }

    fn new_account_positions_set(&mut self) -> UnorderedSet<dex::PositionId> {
        Self::new_set()
    }

    fn new_verified_tokens_set(&mut self) -> <Types as dex::Types>::VerifiedTokensSet {
        Self::new_set()
    }

    fn new_position_to_pool_map(&mut self) -> <Types as dex::Types>::PositionToPoolMap {
        Self::new_doubly_linked_list_map()
    }

    fn new_guards(&mut self) -> <Types as dex::Types>::AccountIdSet {
        Self::new_set()
    }

    #[cfg(feature = "smart-routing")]
    fn new_token_connections_map(&mut self) -> <Types as dex::Types>::TokenConnectionsMap {
        Self::new_doubly_linked_list_map()
    }

    #[cfg(feature = "smart-routing")]
    fn new_top_pools_map(&mut self) -> <Types as dex::Types>::TopPoolsMap {
        Self::new_doubly_linked_list_map()
    }

    #[cfg(feature = "smart-routing")]
    fn new_tokens_array_set(&mut self) -> <Types as dex::Types>::TokensArraySet {
        Self::new_set()
    }

    #[cfg(feature = "smart-routing")]
    fn new_tokens_set(&mut self) -> <Types as dex::Types>::TokensSet {
        Self::new_set()
    }
}

pub type TreeSetIter<'a, I> =
    StorageRefIter<'a, I, PairKeyIter<'a, I, (), <&'a TreeMap<I, ()> as IntoIterator>::IntoIter>>;

impl<I: Ord + Clone + BorshSerialize + BorshDeserialize> dex::Set for TreeMap<I, ()> {
    type Item = I;
    type Ref<'a> = StorageRef<'a, I> where Self: 'a;
    type Iter<'a> = TreeSetIter<'a, I> where Self: 'a;

    fn clear(&mut self) {
        self.clear();
    }

    #[allow(clippy::cast_possible_truncation)]
    fn len(&self) -> usize {
        self.len() as usize
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn iter(&self) -> Self::Iter<'_> {
        TreeSetIter::new(PairKeyIter::new(self.into_iter()))
    }

    fn contains_item(&self, item: &I) -> bool {
        self.contains_key(item)
    }

    fn add_item(&mut self, item: I) {
        self.insert(&item, &());
    }

    fn remove_item(&mut self, item: &I) {
        self.remove(item);
    }
}

pub type UnorderedMapIter<'a, K, V> =
    StorageRefPairIter<'a, K, V, near_sdk::collections::unordered_map::Iter<'a, K, V>>;

impl<K, V> dex::Map for UnorderedMap<K, V>
where
    K: BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    type Key = K;
    type Value = V;
    type KeyRef<'a> = StorageRef<'a, K> where Self: 'a;
    type ValueRef<'a> = StorageRef<'a, V> where Self: 'a;
    type Iter<'a> = UnorderedMapIter<'a, K, V> where Self: 'a;

    fn iter(&self) -> Self::Iter<'_> {
        StorageRefPairIter::new(self.iter())
    }

    fn clear(&mut self) {
        self.clear();
    }

    #[allow(clippy::cast_possible_truncation)]
    fn len(&self) -> usize {
        self.len() as usize
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn contains_key(&self, key: &K) -> bool {
        UnorderedMap::get(self, key).is_some()
    }

    fn inspect<R, F: FnOnce(&V) -> R>(&self, key: &K, inspect_fn: F) -> Option<R> {
        self.get(key).map(|value| inspect_fn(&value))
    }

    fn update<R, F: FnOnce(&mut V) -> Result<R>>(
        &mut self,
        key: &K,
        update_fn: F,
    ) -> Option<Result<R>> {
        self.get(key).map(|mut value| {
            update_fn(&mut value).map(|result| {
                self.insert(key, &value);
                result
            })
        })
    }

    fn update_or_insert<R, F, U>(&mut self, key: &K, factory_fn: F, update_fn: U) -> Result<R>
    where
        F: FnOnce() -> Result<V>,
        U: FnOnce(&mut V, /* exists */ bool) -> Result<R>,
    {
        let (value, exists) = self
            .get(key)
            .map_or_else(|| (factory_fn(), false), |value| (Ok(value), true));
        let mut value = value?;
        let result = update_fn(&mut value, exists)?;
        self.insert(key, &value);
        Ok(result)
    }

    fn insert(&mut self, key: K, value: V) {
        UnorderedMap::insert(self, &key, &value);
    }
}

impl<K, V> dex::MapRemoveKey for UnorderedMap<K, V>
where
    K: BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    fn remove(&mut self, key: &K) {
        UnorderedMap::remove(self, key);
    }
}

pub type UnorderedSetIter<'a, I> =
    StorageRefIter<'a, I, near_sdk::collections::vector::Iter<'a, I>>;

impl<I: BorshSerialize + BorshDeserialize> dex::Set for UnorderedSet<I> {
    type Item = I;
    type Ref<'a> = StorageRef<'a, I> where Self: 'a;
    type Iter<'a> = UnorderedSetIter<'a, I> where Self: 'a;

    fn iter(&self) -> Self::Iter<'_> {
        UnorderedSetIter::new(self.as_vector().iter())
    }

    fn clear(&mut self) {
        self.clear();
    }

    #[allow(clippy::cast_possible_truncation)]
    fn len(&self) -> usize {
        self.len() as usize
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn contains_item(&self, item: &I) -> bool {
        self.contains(item)
    }

    fn add_item(&mut self, item: I) {
        self.insert(&item);
    }

    fn remove_item(&mut self, item: &I) {
        self.remove(item);
    }
}

pub type TreeMapIter<'a, K, V> =
    StorageRefPairIter<'a, K, V, <&'a TreeMap<K, V> as IntoIterator>::IntoIter>;

impl<K, V> dex::Map for TreeMap<K, V>
where
    K: Ord + Clone + BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    type Key = K;
    type Value = V;
    type KeyRef<'a> = StorageRef<'a, K> where Self: 'a;
    type ValueRef<'a> = StorageRef<'a, V> where Self: 'a;
    type Iter<'a> = TreeMapIter<'a, K, V> where Self: 'a;

    fn iter(&self) -> Self::Iter<'_> {
        StorageRefPairIter::new(self.into_iter())
    }

    fn clear(&mut self) {
        self.clear();
    }

    #[allow(clippy::cast_possible_truncation)]
    fn len(&self) -> usize {
        self.len() as usize
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn contains_key(&self, key: &K) -> bool {
        self.contains_key(key)
    }

    fn inspect<R, F: FnOnce(&V) -> R>(&self, key: &K, inspect_fn: F) -> Option<R> {
        self.get(key).map(|value| inspect_fn(&value))
    }

    fn update<R, F: FnOnce(&mut V) -> Result<R>>(
        &mut self,
        key: &K,
        update_fn: F,
    ) -> Option<Result<R>> {
        self.get(key).map(|mut value| {
            update_fn(&mut value).map(|result| {
                self.insert(key, &value);
                result
            })
        })
    }

    fn update_or_insert<R, F, U>(&mut self, key: &K, factory_fn: F, update_fn: U) -> Result<R>
    where
        F: FnOnce() -> Result<V>,
        U: FnOnce(&mut V, /* exists */ bool) -> Result<R>,
    {
        let (value, exists) = self
            .get(key)
            .map_or_else(|| (factory_fn(), false), |value| (Ok(value), true));
        let mut value = value?;
        let result = update_fn(&mut value, exists)?;
        self.insert(key, &value);
        Ok(result)
    }

    fn insert(&mut self, key: K, value: V) {
        self.insert(&key, &value);
    }
}

impl<K, V> dex::MapRemoveKey for TreeMap<K, V>
where
    K: Ord + Clone + BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    fn remove(&mut self, key: &K) {
        self.remove(key);
    }
}

fn treemap_find_key_at<K, V>(map: &TreeMap<K, V>, at: KeyAt<&K>) -> Option<K>
where
    K: Ord + Clone + BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    match at {
        KeyAt::Min => map.min(),
        KeyAt::Max => map.max(),
        KeyAt::Above(key) => map.higher(key),
        KeyAt::Below(key) => map.lower(key),
    }
}

impl<K, V> dex::OrderedMap for TreeMap<K, V>
where
    K: Ord + Clone + BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    fn inspect_at<R, F: FnOnce(&K, &V) -> R>(
        &self,
        at: dex::KeyAt<&K>,
        inspect_fn: F,
    ) -> Option<R> {
        treemap_find_key_at(self, at)
            .and_then(|key| self.inspect(&key, |value| inspect_fn(&key, value)))
    }

    fn update_at<R, F: FnOnce(&K, &mut V) -> Result<R>>(
        &mut self,
        at: KeyAt<&K>,
        update_fn: F,
    ) -> Option<Result<R>> {
        treemap_find_key_at(self, at)
            .and_then(|key| self.update(&key, |value| update_fn(&key, value)))
    }
}

pub type DoublyLinkedListMapIter<'a, K, V> =
    StorageRefPairIter<'a, K, V, <&'a DoublyLinkedListMap<K, V> as IntoIterator>::IntoIter>;

impl<K, V> dex::Map for DoublyLinkedListMap<K, V>
where
    K: Ord + Clone + BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    type Key = K;
    type Value = V;
    type KeyRef<'a> = StorageRef<'a, K> where Self: 'a;
    type ValueRef<'a> = StorageRef<'a, V> where Self: 'a;
    type Iter<'a> = DoublyLinkedListMapIter<'a, K, V> where Self: 'a;

    fn iter(&self) -> Self::Iter<'_> {
        StorageRefPairIter::new(self.into_iter())
    }

    fn clear(&mut self) {
        self.clear();
    }

    #[allow(clippy::cast_possible_truncation)]
    fn len(&self) -> usize {
        self.len()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn contains_key(&self, key: &K) -> bool {
        self.contains_key(key)
    }

    fn inspect<R, F: FnOnce(&V) -> R>(&self, key: &K, inspect_fn: F) -> Option<R> {
        self.get(key).map(|value| inspect_fn(&value))
    }

    fn update<R, F: FnOnce(&mut V) -> Result<R>>(
        &mut self,
        key: &K,
        update_fn: F,
    ) -> Option<Result<R>> {
        self.get(key).map(|mut value| {
            update_fn(&mut value).map(|result| {
                self.insert(key, value);
                result
            })
        })
    }

    fn update_or_insert<R, F, U>(&mut self, key: &K, factory_fn: F, update_fn: U) -> Result<R>
    where
        F: FnOnce() -> Result<V>,
        U: FnOnce(&mut V, /* exists */ bool) -> Result<R>,
    {
        let (value, exists) = self
            .get(key)
            .map_or_else(|| (factory_fn(), false), |value| (Ok(value), true));
        let mut value = value?;
        let result = update_fn(&mut value, exists)?;
        self.insert(key, value);
        Ok(result)
    }

    fn insert(&mut self, key: K, value: V) {
        self.insert(&key, value);
    }
}

impl<K, V> dex::MapRemoveKey for DoublyLinkedListMap<K, V>
where
    K: Ord + Clone + BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    fn remove(&mut self, key: &K) {
        self.remove(key);
    }
}

pub type LinkedListMapIter<'a, K, V> =
    StorageRefPairIter<'a, K, V, <&'a LinkedListMap<K, V> as IntoIterator>::IntoIter>;

impl<K, V> dex::Map for LinkedListMap<K, V>
where
    K: Ord + Clone + BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    type Key = K;
    type Value = V;
    type KeyRef<'a> = StorageRef<'a, K> where Self: 'a;
    type ValueRef<'a> = StorageRef<'a, V> where Self: 'a;
    type Iter<'a> = LinkedListMapIter<'a, K, V> where Self: 'a;

    fn iter(&self) -> Self::Iter<'_> {
        StorageRefPairIter::new(self.into_iter())
    }

    fn clear(&mut self) {
        self.clear();
    }

    #[allow(clippy::cast_possible_truncation)]
    fn len(&self) -> usize {
        self.len()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn contains_key(&self, key: &K) -> bool {
        self.contains_key(key)
    }

    fn inspect<R, F: FnOnce(&V) -> R>(&self, key: &K, inspect_fn: F) -> Option<R> {
        self.get(key).map(|value| inspect_fn(&value))
    }

    fn update<R, F: FnOnce(&mut V) -> Result<R>>(
        &mut self,
        key: &K,
        update_fn: F,
    ) -> Option<Result<R>> {
        self.get(key).map(|mut value| {
            update_fn(&mut value).map(|result| {
                self.insert(key, &value);
                result
            })
        })
    }

    fn update_or_insert<R, F, U>(&mut self, key: &K, factory_fn: F, update_fn: U) -> Result<R>
    where
        F: FnOnce() -> Result<V>,
        U: FnOnce(&mut V, /* exists */ bool) -> Result<R>,
    {
        let (value, exists) = self
            .get(key)
            .map_or_else(|| (factory_fn(), false), |value| (Ok(value), true));
        let mut value = value?;
        let result = update_fn(&mut value, exists)?;
        self.insert(key, &value);
        Ok(result)
    }

    fn insert(&mut self, key: K, value: V) {
        self.insert(&key, &value);
    }
}
