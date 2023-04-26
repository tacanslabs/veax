//! Traits and types which must be implemented and/or used
//! to use Dex shared implementstion
use std::fmt::Arguments;
use std::marker::PhantomData;
use std::ops::Deref;

use super::errors::Result;
use super::{
    latest, Account, AccountLatest, BasisPoints, Contract, ContractLatest, FeeLevel, Float, Pool,
    PoolId, PoolLatest, PoolUpdateReason, Position, PositionId, PositionLatest, Side, TickState,
    TickStateV0,
};
use crate::chain::{AccountId, Amount, LPFeePerFeeLiquidity, Liquidity, LiquiditySFP, TokenId};
use crate::dex::tick::{EffTick, Tick};
use crate::dex::ErrorKind;
use crate::{ensure_here, AmountUFP};
use latest::RawFeeLevelsArray;

#[allow(unused)]
use num_traits::Zero;

#[cfg(feature = "near")]
mod near {
    use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
    use near_sdk::serde::{Deserialize, Serialize};

    pub trait PersistentBound {}

    impl<T> PersistentBound for T {}

    pub trait PersistentCollection<T: PersistentBound>: BorshSerialize + BorshDeserialize {}

    impl<T: BorshSerialize + BorshDeserialize, U: PersistentBound> PersistentCollection<U> for T {}
    /// Bounding trait for serializable values which fit into single data block
    pub trait Persistent: BorshDeserialize + BorshSerialize {}

    impl<T: BorshSerialize + BorshDeserialize> Persistent for T {}
    /// Bounding trait for serializable values which are used in public WASM APIs
    pub trait WasmApi: Serialize + for<'a> Deserialize<'a> {}

    impl<T: Serialize + for<'a> Deserialize<'a>> WasmApi for T {}
}
#[cfg(feature = "near")]
pub use near::{Persistent, PersistentBound, PersistentCollection, WasmApi};

/// Set of types used to parametrize contract state
///
/// Decoupled from State/StateMut trait definition to avoid circular types
/// and complex type constraints in certain cases
pub trait Types: Sized {
    /// Type which parametrizes `PersistentCollection` trait bound, passed through associated type
    /// to reduce overall clutter
    type Bound: PersistentBound;

    /// Blockchain-specific extra data for each contract
    type ContractExtra: PersistentCollection<Self::Bound> + Default;

    /// Map from account identifiers to account records
    type AccountsMap: PersistentCollection<Self::Bound>
        + MapRemoveKey<Key = AccountId, Value = super::Account<Self>>;

    /// Ticks
    type TickStatesMap: PersistentCollection<Self::Bound>
        + OrderedMap<Key = Tick, Value = TickState<Self>>;

    /// Per-account Map of token balances indexed by token ids
    type AccountTokenBalancesMap: PersistentCollection<Self::Bound>
        + MapRemoveKey<Key = TokenId, Value = Amount>;

    /// Handles tracking of withdrawals, on per-account basis
    type AccountWithdrawTracker: PersistentCollection<Self::Bound> + AccountWithdrawTracker;

    /// Extra data stored in each account entry, blockchain-specific
    type AccountExtra: PersistentCollection<Self::Bound> + Default + AccountExtra;

    /// Map of liquidity pools indexed by pool identifier
    type PoolsMap: PersistentCollection<Self::Bound> + Map<Key = PoolId, Value = super::Pool<Self>>;

    /// Per-pool map of position records indexed by position ids
    type PoolPositionsMap: PersistentCollection<Self::Bound>
        + MapRemoveKey<Key = PositionId, Value = super::Position<Self>>;

    /// Set of position ids opened by some account
    type AccountPositionsSet: PersistentCollection<Self::Bound> + Set<Item = PositionId>;

    /// Set of verified tokens
    type VerifiedTokensSet: PersistentCollection<Self::Bound> + Set<Item = TokenId>;

    /// Mapping from position id to pool id it belongs to
    type PositionToPoolMap: PersistentCollection<Self::Bound>
        + MapRemoveKey<Key = PositionId, Value = PoolId>;

    /// Set of accounts
    type AccountIdSet: PersistentCollection<Self::Bound> + Set<Item = AccountId>;

    /// Map of existing connections between tokens
    /// Connection means being in one pool
    #[cfg(feature = "smart-routing")]
    type TokenConnectionsMap: PersistentCollection<Self::Bound>
        + MapRemoveKey<Key = TokenId, Value = Self::TokensSet>;

    /// Set of tokens
    #[cfg(feature = "smart-routing")]
    type TokensSet: PersistentCollection<Self::Bound> + Set<Item = TokenId>;

    /// Set of tokens, that's going to be used as an array
    #[cfg(feature = "smart-routing")]
    type TokensArraySet: PersistentCollection<Self::Bound> + Set<Item = TokenId>;

    // Map of top pools for each token
    #[cfg(feature = "smart-routing")]
    type TopPoolsMap: PersistentCollection<Self::Bound>
        + MapRemoveKey<Key = TokenId, Value = Self::TokensArraySet>;
}

/// Tracks withdrawals for each account, which may imply different confirmation strategies,
/// depending on concrete blockchain specifics
pub trait AccountWithdrawTracker {
    /// Checks if whole account can't be unregistered due to unfinished withdraws
    fn is_any_withdraw_in_progress(&self) -> bool;
    /// Check if specific token can't be unregistered due to unfinished withdraws
    fn is_token_withdraw_in_progress(&self, token_id: &TokenId) -> bool;
}
/// Additional actions may need to be performed with `AccountExtra` data
pub trait AccountExtra {
    /// Actions during pool creation
    fn on_pool_created(&mut self) -> Result<()> {
        Ok(())
    }
}

pub trait State<T: Types + ?Sized> {
    /// Get immutable reference to contract state
    ///
    /// # Returns
    /// Immutable reference to contract state
    fn contract(&self) -> &super::Contract<T>;
    /// Make temporary immutable `Dex` instance out of `&self`
    fn as_dex(&self) -> super::Dex<T, Self, &Self>
    where
        Self: Sized,
    {
        super::Dex::new(self)
    }
}
/// Keeps mutable references to components of contract
pub struct StateMembersMut<'a, T: Types + ?Sized> {
    /// Actual contract state
    pub contract: &'a mut super::Contract<T>,
    /// Factory object, used to construct various state records
    pub item_factory: &'a mut dyn ItemFactory<T>,
    /// Logger, writes events to blockchain log
    pub logger: &'a mut dyn Logger,
}

pub trait StateMut<T: Types + ?Sized>: State<T> {
    type SendTokensResult: 'static + Sized;
    type SendTokensExtraParam: 'static + Sized + WasmApi;

    fn members_mut(&mut self) -> StateMembersMut<'_, T>;

    fn contract_mut<'a>(&'a mut self) -> &'a mut super::Contract<T>
    where
        T: 'a,
    {
        self.members_mut().contract
    }

    fn item_factory_mut(&mut self) -> &mut dyn ItemFactory<T> {
        self.members_mut().item_factory
    }

    fn logger_mut<'a>(&'a mut self) -> &'a mut dyn Logger
    where
        // Need this constraint because `logger` lifetime
        // formally depends on `self` and `T`, but `T`
        // isn't present in `Logger`
        T: 'a,
    {
        self.members_mut().logger
    }
    /// Sends specified tokens to its actual owner
    ///
    /// This method must invoke `Dex::finish_send_tokens` with actual send results,
    /// one or another way, to properly finalize or roll back send operation.
    /// Doing so in generic way isn't possible because some BCs use coroutine-like
    /// pseudo-sync calls, while others may use async calls with result handling through callback
    ///
    /// # Parameters
    /// * `account_id` - account to which tokens should be sent
    /// * `token_id` - token which should be sent
    /// * `amount` - amount of tokens to send
    /// * `unregister_token` - whether to attempt to unregister token; this parameter
    ///     should be passed to `Dex::finish_send_tokens`
    /// * `extra` - extra parameter, blockchain-specific
    ///
    /// # Returns
    /// Blockchain-specific send result, propagated out of `Dex` to caller
    fn send_tokens(
        &mut self,
        account_id: &AccountId,
        token_id: &TokenId,
        amount: Amount,
        unregister_token: bool,
        extra: Self::SendTokensExtraParam,
    ) -> Self::SendTokensResult;
    /// Retrieve account identifier which initiated whole chain of calls
    /// which constitutes transactions.
    fn get_initiator_id(&self) -> AccountId;
    /// Retrieve identifier of entity which called smart contract's method
    fn get_caller_id(&self) -> AccountId;
    /// Make temporary mutable `Dex` instance out of `&mut self`
    fn as_dex_mut(&mut self) -> super::Dex<T, Self, &mut Self>
    where
        Self: Sized,
    {
        super::Dex::new(self)
    }
}

pub trait ItemFactory<T: Types + ?Sized> {
    fn new_accounts_map(&mut self) -> T::AccountsMap;
    fn new_tick_states_map(&mut self) -> T::TickStatesMap;
    fn new_account_token_balances_map(&mut self) -> T::AccountTokenBalancesMap;
    fn new_account_withdraw_tracker(&mut self) -> T::AccountWithdrawTracker;
    fn new_pools_map(&mut self) -> T::PoolsMap;
    fn new_pool_positions_map(&mut self) -> T::PoolPositionsMap;
    fn new_account_positions_set(&mut self) -> T::AccountPositionsSet;
    fn new_verified_tokens_set(&mut self) -> T::VerifiedTokensSet;
    fn new_position_to_pool_map(&mut self) -> T::PositionToPoolMap;
    fn new_guards(&mut self) -> T::AccountIdSet;
    #[cfg(feature = "smart-routing")]
    fn new_token_connections_map(&mut self) -> T::TokenConnectionsMap;
    #[cfg(feature = "smart-routing")]
    fn new_tokens_set(&mut self) -> T::TokensSet;
    #[cfg(feature = "smart-routing")]
    fn new_top_pools_map(&mut self) -> T::TopPoolsMap;
    #[cfg(feature = "smart-routing")]
    fn new_tokens_array_set(&mut self) -> T::TokensArraySet;

    fn new_contract(
        &mut self,
        owner_id: AccountId,
        protocol_fee_fraction: BasisPoints,
        fee_rates: latest::RawFeeLevelsArray<BasisPoints>,
    ) -> Result<Contract<T>> {
        ensure_here!(
            fee_rates == [1, 2, 4, 8, 16, 32, 64, 128],
            ErrorKind::InvalidParams
        );
        Ok(Contract::V0(ContractLatest {
            owner_id,
            guards: self.new_guards(),
            suspended: false,
            pools: self.new_pools_map().into(),
            accounts: self.new_accounts_map().into(),
            verified_tokens: self.new_verified_tokens_set(),
            pool_count: 0,
            next_free_position_id: 0,
            position_to_pool_id: self.new_position_to_pool_map().into(),
            protocol_fee_fraction,
            #[cfg(feature = "smart-routing")]
            token_connections: self.new_token_connections_map().into(),
            #[cfg(feature = "smart-routing")]
            top_pools: self.new_top_pools_map().into(),
            extra: T::ContractExtra::default(),
        }))
    }

    fn new_account(&mut self) -> Result<Account<T>> {
        Ok(Account::V0(AccountLatest {
            token_balances: self.new_account_token_balances_map().into(),
            positions: self.new_account_positions_set(),
            withdraw_tracker: self.new_account_withdraw_tracker(),
            extra: Default::default(),
        }))
    }

    fn new_pool(&mut self) -> Result<Pool<T>> {
        Ok(Pool::V0(PoolLatest {
            total_reserves: (Amount::zero(), Amount::zero()),
            positions: self.new_pool_positions_map().into(),
            eff_sqrtprices: latest::FeeLevelsArray::default(),
            acc_lp_fees_per_fee_liquidity: latest::FeeLevelsArray::from_value((
                LPFeePerFeeLiquidity::zero(),
                LPFeePerFeeLiquidity::zero(),
            )),
            acc_lp_fee: (AmountUFP::zero(), AmountUFP::zero()),
            tick_states: latest::FeeLevelsArray::from_fn(|_| self.new_tick_states_map().into()),
            net_liquidities: latest::FeeLevelsArray::default(),
            position_reserves: latest::FeeLevelsArray::from_value((
                AmountUFP::zero(),
                AmountUFP::zero(),
            )),
            next_active_ticks_left: latest::FeeLevelsArray::default(),
            next_active_ticks_right: latest::FeeLevelsArray::default(),
            top_active_level: 0,
            active_side: Side::Left,
            pivot: EffTick::default(),
        }))
    }

    fn new_position(
        &mut self,
        fee_level: FeeLevel,
        net_liquidity: Liquidity,
        init_acc_lp_fees_per_fee_liquidity: (LPFeePerFeeLiquidity, LPFeePerFeeLiquidity),
        ticks_range: (Tick, Tick),
        init_sqrtprice: Float,
    ) -> Result<Position<T>> {
        Ok(Position::V0(PositionLatest {
            fee_level,
            net_liquidity,
            init_sqrtprice,
            init_acc_lp_fees_per_fee_liquidity,
            tick_bounds: ticks_range,
            unwithdrawn_acc_lp_fees_per_fee_liquidity: init_acc_lp_fees_per_fee_liquidity,
            phantom_t: PhantomData,
        }))
    }

    fn new_tick(
        &mut self,
        net_liquidity_change: LiquiditySFP,
        reference_counter: u32,
        acc_lp_fees_per_fee_liquidity_outside: (LPFeePerFeeLiquidity, LPFeePerFeeLiquidity),
    ) -> Result<TickState<T>> {
        Ok(TickState::V0(TickStateV0 {
            net_liquidity_change,
            reference_counter,
            acc_lp_fees_per_fee_liquidity_outside,
            phantom_t: PhantomData,
        }))
    }

    fn new_default_tick(&mut self) -> Result<TickState<T>> {
        Ok(TickState::V0(TickStateV0::default()))
    }
}

pub trait Set {
    /// Actual type of set item
    type Item;
    /// Any type which works as reference to set item
    type Ref<'a>: Deref<Target = Self::Item>
    where
        Self: 'a;
    /// Iterator over all set items
    type Iter<'a>: Iterator<Item = Self::Ref<'a>>
    where
        Self: 'a;
    /// Iterate over set items; iteration order is defined by implementation
    fn iter(&self) -> Self::Iter<'_>;
    /// Remove all items from set
    fn clear(&mut self);
    /// Check if set is empty
    fn is_empty(&self) -> bool;
    /// Get number of elements in set
    fn len(&self) -> usize;
    /// Check if set contains specified item
    fn contains_item(&self, item: &Self::Item) -> bool;
    /// Add item to set
    fn add_item(&mut self, item: Self::Item);
    /// Remove item from set
    fn remove_item(&mut self, item: &Self::Item);
}
/// Common trait for all map-like constructs
pub trait Map {
    /// Type of map key
    type Key;
    /// Type of map value
    type Value;
    /// Any type which works as immutable reference to key
    ///
    /// Note: having just `Ref<'a, T>` is possible
    /// but leads to bogus "type K (or V) may not live long enough" everywhere
    /// and needs kilometer-long annotations
    type KeyRef<'a>: Deref<Target = Self::Key>
    where
        Self: 'a;
    /// Any type which works as immutable reference to key
    type ValueRef<'a>: Deref<Target = Self::Value>
    where
        Self: 'a;
    /// Iterator over all map entries
    type Iter<'a>: Iterator<Item = (Self::KeyRef<'a>, Self::ValueRef<'a>)>
    where
        Self: 'a;
    /// Iterate over map's key-value pairs; iteration order is defined by implementation
    fn iter(&self) -> Self::Iter<'_>;
    /// Remove all items from set
    fn clear(&mut self);
    /// Check if set is empty
    fn is_empty(&self) -> bool;
    /// Get number of elements in set
    fn len(&self) -> usize;
    /// Checks if map contains specified key
    fn contains_key(&self, key: &Self::Key) -> bool;
    /// Find value in map and supply its immutable reference to provided callback, if found
    ///
    /// # Arguments
    /// * `key` - key to look for
    /// * `inspect_fn` - callback which will receive immutable reference to value, if one is found
    ///
    /// # Returns
    /// * Some(_) - if value was found
    /// * None - if value was not found; contains map's custom "not found" error if needed
    fn inspect<R, F: FnOnce(&Self::Value) -> R>(&self, key: &Self::Key, inspect_fn: F)
        -> Option<R>;
    /// Find value in map and supply its immutable reference to provided callback, if found
    ///
    /// Saves value to backing storage if `update_fn` succeeds.
    ///
    /// # Arguments
    /// * `key` - key to look for
    /// * `update_fn` - callback which receives mutable reference to value, if one is found;
    ///   can break update procedure by returning `Err(_)`
    ///
    /// # Returns
    /// * Some(Ok(_)_ - if value was found
    /// * None - if value was not found
    /// * Some(Err(_)) - if value was found, but either `update_fn` or post-update logic failed
    fn update<R, F: FnOnce(&mut Self::Value) -> Result<R>>(
        &mut self,
        key: &Self::Key,
        update_fn: F,
    ) -> Option<Result<R>>;
    /// Find value in map and supply its mutable reference to provided callback;
    /// creates new value if not found
    ///
    /// Saves value to backing storage if `update_fn` succeeds.
    ///
    /// # Arguments
    /// * `key` - key to look for
    /// * `factory_fn` - generates new value for container
    /// * `update_fn` - callback which receives mutable reference to value,
    ///   and boolean flag `exists` which is true if existing value was found;
    ///   can break update procedure by returning `Err(_)`
    ///
    /// # Returns
    /// * Ok(()) - if update or creation succeeded
    /// * Err(_) - if either `update_fn` or post-update logic failed
    fn update_or_insert<R, F, U>(
        &mut self,
        key: &Self::Key,
        factory_fn: F,
        update_fn: U,
    ) -> Result<R>
    where
        F: FnOnce() -> Result<Self::Value>,
        U: FnOnce(&mut Self::Value, /* exists */ bool) -> Result<R>;
    /// Insert new value into map container
    fn insert(&mut self, key: Self::Key, value: Self::Value);
}

pub trait MapRemoveKey: Map {
    /// Remove value from map container
    fn remove(&mut self, key: &Self::Key);
}

/// Defines location where to look for entry in ordered map
#[derive(Copy, Clone)]
pub enum KeyAt<T> {
    /// First key in map, the "smallest" one
    Min,
    /// Entry right before specified key
    Below(T),
    /// Entry right after specified key
    Above(T),
    /// Last key in map, the "largest" one
    Max,
}

/// Common trait for all iterable map-like constructs
pub trait OrderedMap: MapRemoveKey {
    /// Try inspect entry at specified position
    ///
    /// # Parameters
    /// * `at` - where to look for entry
    /// * `inspect_fn` - callback which receives immutable references to entry's key and value, if one found
    ///
    /// # Return
    /// * `Some(R)` - result of inspection callback, if entry was found
    /// * `None` otherwise
    fn inspect_at<R, F: FnOnce(&Self::Key, &Self::Value) -> R>(
        &self,
        at: KeyAt<&Self::Key>,
        inspect_fn: F,
    ) -> Option<R>;
    /// Try update entry at specified position
    ///
    /// # Parameters
    /// * `at` - where to look for entry
    /// * `update_fn` - callback which receives immutable reference to entry's key and mutable one to value, if one found;
    ///     entry is updated if callback returns `Ok(_)`, otherwise changes are discarded
    ///
    /// # Return
    /// * `Some(_)` - result of update callback, if entry was found
    /// * `None` otherwise
    fn update_at<R, F: FnOnce(&Self::Key, &mut Self::Value) -> Result<R>>(
        &mut self,
        at: KeyAt<&Self::Key>,
        update_fn: F,
    ) -> Option<Result<R>>;

    /// Find the minimum key in map and supply immutable references to key-value pair to provided callback
    ///
    /// # Arguments
    /// * `inspect_fn` - callback which will receive immutable reference to key and value, if map is not empty
    ///
    /// # Returns
    /// * Some(_) - callback's result if map is not empty
    /// * None - if map is empty
    fn inspect_min<R, F: FnOnce(&Self::Key, &Self::Value) -> R>(&self, inspect_fn: F) -> Option<R> {
        self.inspect_at(KeyAt::Min, inspect_fn)
    }

    /// Find the maximum key in map and supply immutable references to key-value pair to provided callback
    ///
    /// # Arguments
    /// * `inspect_fn` - callback which will receive immutable reference to key and value, if map is not empty
    ///
    /// # Returns
    /// * Some(_) - callback's result if map is not empty
    /// * None - if map is empty
    fn inspect_max<R, F: FnOnce(&Self::Key, &Self::Value) -> R>(&self, inspect_fn: F) -> Option<R> {
        self.inspect_at(KeyAt::Max, inspect_fn)
    }

    /// Find the smallest key bigger than provided and supply immutable references to key-value pair to provided callback
    ///
    /// # Arguments
    /// * `key` - key
    /// * `inspect_fn` - callback which will receive immutable reference to key and value, if found
    ///
    /// # Returns
    /// * Some(_) - callback's result if key above is found
    /// * None - if key above is not found
    fn inspect_above<R, F: FnOnce(&Self::Key, &Self::Value) -> R>(
        &self,
        key: &Self::Key,
        inspect_fn: F,
    ) -> Option<R> {
        self.inspect_at(KeyAt::Above(key), inspect_fn)
    }

    /// Find the biggest key smaller than provided and supply immutable references to key-value pair to provided callback
    ///
    /// # Arguments
    /// * `key` - key
    /// * `inspect_fn` - callback which will receive immutable reference to key and value, if found
    ///
    /// # Returns
    /// * Some(_) - callback's result if key below is found
    /// * None - if key below is not found
    fn inspect_below<R, F: FnOnce(&Self::Key, &Self::Value) -> R>(
        &self,
        key: &Self::Key,
        inspect_fn: F,
    ) -> Option<R> {
        self.inspect_at(KeyAt::Below(key), inspect_fn)
    }

    /// Find the biggest key smaller than provided and supply mutable references to key-value pair to provided callback
    ///
    /// Saves value to backing storage if `update_fn` succeeds.
    ///
    /// # Arguments
    /// * `key` - key
    /// * `update_fn` - callback which receives mutable reference to value, if one is found;
    ///   can break update procedure by returning `Err(_)`
    ///
    /// # Returns
    /// * Some(Ok(_)_ - if value was found
    /// * None - if value was not found
    /// * Some(Err(_)) - if value was found, but either `update_fn` or post-update logic failed
    fn update_above<R, F: FnOnce(&Self::Key, &mut Self::Value) -> Result<R>>(
        &mut self,
        key: &Self::Key,
        update_fn: F,
    ) -> Option<Result<R>> {
        self.update_at(KeyAt::Above(key), update_fn)
    }

    /// Find the smallest key bigger than provided and supply mutable references to key-value pair to provided callback
    ///
    /// Saves value to backing storage if `update_fn` succeeds.
    ///
    /// # Arguments
    /// * `key` - key
    /// * `update_fn` - callback which receives mutable reference to value, if one is found;
    ///   can break update procedure by returning `Err(_)`
    ///
    /// # Returns
    /// * Some(Ok(_)_ - if value was found
    /// * None - if value was not found
    /// * Some(Err(_)) - if value was found, but either `update_fn` or post-update logic failed
    fn update_below<R, F: FnOnce(&Self::Key, &mut Self::Value) -> Result<R>>(
        &mut self,
        key: &Self::Key,
        update_fn: F,
    ) -> Option<Result<R>> {
        self.update_at(KeyAt::Below(key), update_fn)
    }
}

/// `EventEmitter` hides platform-specific event API and/or custom event formatting.
pub trait Logger {
    fn log(&mut self, args: Arguments<'_>);
    fn log_deposit_event(
        &mut self,
        user: &AccountId,
        token: &TokenId,
        amount: &Amount,
        balance: &Amount,
    );
    fn log_withdraw_event(
        &mut self,
        user: &AccountId,
        token: &TokenId,
        amount: &Amount,
        balance: &Amount,
    );
    fn log_open_position_event(
        &mut self,
        user: &AccountId,
        pool: (&TokenId, &TokenId),
        amounts: (&Amount, &Amount),
        fee_rate: BasisPoints,
        position_id: PositionId,
    );
    fn log_harvest_fee_event(&mut self, position_id: PositionId, fee_amounts: (Amount, Amount));
    fn log_close_position_event(&mut self, position_id: PositionId, amounts: (Amount, Amount));
    fn log_swap_event(
        &mut self,
        user: &AccountId,
        tokens: (&TokenId, &TokenId),
        amounts: (&Amount, &Amount),
        fees: &[(&TokenId, &BasisPoints)],
    );
    fn log_update_pool_state_event(
        &mut self,
        reason: PoolUpdateReason,
        pool: (&TokenId, &TokenId),
        amounts_a: &RawFeeLevelsArray<Amount>,
        amounts_b: &RawFeeLevelsArray<Amount>,
        sqrt_prices: &RawFeeLevelsArray<Float>,
        liquidities: &RawFeeLevelsArray<Float>,
    );

    fn log_add_verified_tokens_event(&mut self, tokens: &[TokenId]);
    fn log_remove_verified_tokens_event(&mut self, tokens: &[TokenId]);

    fn log_add_guard_accounts_event(&mut self, tokens: &[AccountId]);
    fn log_remove_guard_accounts_event(&mut self, tokens: &[AccountId]);

    fn log_suspend_payable_api_event(&mut self, account: &AccountId);
    fn log_resume_payable_api_event(&mut self, account: &AccountId);
}
