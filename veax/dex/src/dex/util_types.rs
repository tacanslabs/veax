use super::utils::swap_if;
use super::{latest, BasisPoints, ErrorKind as DexErrorKind, Float, PositionId, WasmApi};
use crate::chain::wasm::WasmAmount;
use crate::chain::{Amount, Liquidity, TokenId};
use crate::dex::tick::Tick;
use crate::ensure;
use std::ops::{Deref, Index, IndexMut};

#[cfg(feature = "near")]
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
#[cfg(feature = "near")]
use near_sdk::serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "near", derive(BorshDeserialize, BorshSerialize))]
pub struct PoolId {
    // Field is named only to avoid conflicts with Deref
    pair: (TokenId, TokenId),
}
/// Allow `PoolId` users easily observe tokens from pair inside - but not modify them
impl Deref for PoolId {
    type Target = (TokenId, TokenId);

    fn deref(&self) -> &Self::Target {
        &self.pair
    }
}

impl PoolId {
    /// Construct pool identifier from pair of token identifiers, if possible
    ///
    /// # Return
    /// * `Err(ErrorKind::TokenDuplicates)` if tokens in pair are equal
    /// * `Ok((pool_id, swapped))` on success, where `swapped` is `true` if token identifiers were swapped
    pub fn try_from_pair(pair: (TokenId, TokenId)) -> Result<(PoolId, bool), DexErrorKind> {
        ensure!(pair.0 != pair.1, DexErrorKind::TokenDuplicates);
        let swapped = pair.0 < pair.1;
        let pair = swap_if(swapped, pair);
        Ok((Self { pair }, swapped))
    }
    /// Returns pair of references to stored token identifiers
    pub fn as_refs(&self) -> (&TokenId, &TokenId) {
        (&self.0, &self.1)
    }

    pub fn side(&self, input_token: &TokenId) -> Side {
        if *input_token == self.pair.0 {
            Side::Left
        } else {
            Side::Right
        }
    }
}

impl From<PoolId> for (TokenId, TokenId) {
    fn from(pool_id: PoolId) -> Self {
        (pool_id.pair.0, pool_id.pair.1)
    }
}

#[cfg_attr(feature = "near", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "near", serde(crate = "near_sdk::serde"))]
#[derive(Clone, Debug)]
pub struct PositionInit {
    pub amount_ranges: (Range<WasmAmount>, Range<WasmAmount>),
    pub ticks_range: (Option<i32>, Option<i32>),
}

impl PositionInit {
    pub fn new_full_range(
        min_a: impl Into<WasmAmount>,
        max_a: impl Into<WasmAmount>,
        min_b: impl Into<WasmAmount>,
        max_b: impl Into<WasmAmount>,
    ) -> Self {
        Self {
            amount_ranges: (
                Range {
                    min: min_a.into(),
                    max: max_a.into(),
                },
                Range {
                    min: min_b.into(),
                    max: max_b.into(),
                },
            ),
            ticks_range: (None, None),
        }
    }

    pub fn transpose_if(self, transposed: bool) -> PositionInit {
        PositionInit {
            amount_ranges: swap_if(transposed, self.amount_ranges),
            ticks_range: if transposed {
                (
                    // saturating_neg is just fine since valid range is much narrower than
                    self.ticks_range.1.map(i32::saturating_neg),
                    self.ticks_range.0.map(i32::saturating_neg),
                )
            } else {
                self.ticks_range
            },
        }
    }
}

#[cfg_attr(feature = "near", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "near",
    serde(
        crate = "near_sdk::serde",
        bound(deserialize = "T: for<'d> Deserialize<'d>")
    )
)]
#[derive(Copy, Clone, Debug)]
pub struct Range<T: std::fmt::Debug + WasmApi> {
    pub min: T,
    pub max: T,
}

#[derive(Copy, Clone, PartialEq, Eq, Default)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
#[cfg_attr(feature = "near", derive(BorshDeserialize, BorshSerialize))]
#[cfg_attr(feature = "test-utils", derive(serde::Serialize, serde::Deserialize))]
pub enum Side {
    #[default]
    Left,
    Right,
}

impl Side {
    pub fn opposite(&self) -> Side {
        match *self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }

    pub fn opposite_if(&self, cond: bool) -> Side {
        if cond {
            self.opposite()
        } else {
            *self
        }
    }

    pub fn from_swapped(swapped: bool) -> Side {
        if swapped {
            Side::Right
        } else {
            Side::Left
        }
    }
}

impl<T> Index<Side> for (T, T) {
    type Output = T;

    fn index(&self, side: Side) -> &Self::Output {
        match side {
            Side::Left => &self.0,
            Side::Right => &self.1,
        }
    }
}

impl<T> IndexMut<Side> for (T, T) {
    fn index_mut(&mut self, side: Side) -> &mut Self::Output {
        match side {
            Side::Left => &mut self.0,
            Side::Right => &mut self.1,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
#[cfg_attr(feature = "test-utils", derive(serde::Serialize, serde::Deserialize))]
pub enum Exact {
    In,
    Out,
}

impl Exact {
    pub fn opposite(self) -> Self {
        match self {
            Exact::In => Exact::Out,
            Exact::Out => Exact::In,
        }
    }
}

/// Single action. Allows to execute sequence of various actions initiated by an account.
/// This type of actions can be passed only as message payload during `deposit`.
#[cfg_attr(
    feature = "near",
    derive(Serialize, Deserialize),
    serde(
        crate = "near_sdk::serde",
        bound(deserialize = "E: for<'d> Deserialize<'d>")
    )
)]
#[derive(Clone, Debug)]
pub enum Action<E: 'static + Sized + super::WasmApi> {
    /// Request account registration; can occur at most once, as frst action in batch
    RegisterAccount,
    /// Register specified tokens for account
    RegisterTokens(Vec<TokenId>),
    /// Perform swap-in exchange of tokens
    SwapExactIn(SwapAction),
    /// Perform swap-out exchange of tokens
    SwapExactOut(SwapAction),
    /// Deposit token to account; account, token and amount are passed as part of call context;
    /// should appear exactly once in batch
    Deposit,
    /// Withdraw specified token from account
    Withdraw(TokenId, WasmAmount, E),
    /// Opens position with specified tokens and their specified amounts
    OpenPosition {
        tokens: (TokenId, TokenId),
        fee_rate: BasisPoints,
        position: PositionInit,
    },
    /// Closes specified position
    ClosePosition(PositionId),
    /// Withdraw fees collected on specific position. User must own it
    WithdrawFee(PositionId),
}

#[cfg_attr(feature = "near", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "near", serde(crate = "near_sdk::serde"))]
#[derive(Clone, Debug)]
pub struct SwapAction {
    // TODO: consider defining different structs for SwapExactIn and SwapExactOut with more explicit field names: amount_in/amount_out instead of amount; min_amount_out/max_amount_in instead of amount_limit
    /// Pool which should be used for swapping.
    pub token_in: TokenId,
    pub token_out: TokenId,
    /// Amount to exchange.
    /// If amount_in is None, it will take amount_out from previous step.
    /// Will fail if amount_in is None on the first step.
    pub amount: Option<WasmAmount>,
    /// LImit on the resulting amount.
    /// For exact-in swap this is min out amount.
    /// For exact-out swap this is max in amount.
    pub amount_limit: WasmAmount,
}

#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq))]
pub struct PositionInfo {
    pub tokens_ids: (TokenId, TokenId),
    pub balance: (Amount, Amount),
    pub init_sqrtprice: Float,
    pub range_ticks: (Tick, Tick),
    pub reward_since_last_withdraw: (Amount, Amount),
    pub reward_since_creation: (Amount, Amount),
}

#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct PoolInfo {
    /// Total amounts of tokens in the pool: sum of all positions and collected fees (LP and protocol).
    pub total_reserves: (Amount, Amount),

    /// Total amount of tokens locked in the pool (in positions)
    pub position_reserves: (Amount, Amount),

    /// Square root of the spot price on each of the fee levels, scaled by 2**64.
    /// The value is approximate, as interlally a different representation is used.
    pub spot_sqrtprices: latest::RawFeeLevelsArray<Float>,

    /// Liquidity on each of the fee levels.
    /// The value is approximate, as interlally a different representation is used.
    pub liquidities: latest::RawFeeLevelsArray<Liquidity>,

    /// Fee rate scaled up by fee_divisor.
    pub fee_rates: latest::RawFeeLevelsArray<BasisPoints>,

    /// Scale factor for the fee levels.
    pub fee_divisor: BasisPoints,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "near", derive(Serialize))]
pub enum PoolUpdateReason {
    #[cfg_attr(feature = "near", serde(rename = "add_pos"))]
    AddLiquidity,
    #[cfg_attr(feature = "near", serde(rename = "rm_pos"))]
    RemoveLiquidity,
    #[cfg_attr(feature = "near", serde(rename = "swap"))]
    Swap,
}

#[derive(Debug)]
#[cfg_attr(feature = "near", derive(serde::Serialize, serde::Deserialize))]
pub struct VersionInfo {
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
#[cfg_attr(feature = "near", derive(Serialize, Deserialize))]
pub struct Path {
    pub tokens: Vec<TokenId>,
    pub token_out: TokenId,
    pub amount: Amount,
}
