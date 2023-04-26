use chain::{
    Amount, AmountSFP, AmountUFP, FeeLiquidityUFP, GrossLiquidityUFP, LPFeePerFeeLiquidity,
    Liquidity, NetLiquidityUFP,
};
use dex::dex_impl::{fee_levels, fee_rate_ticks, fee_rates_ticks};
use dex::errors::{Error, ErrorKind, Result};
use dex::tick::{EffTick, Tick, PRECALCULATED_TICKS};
use dex::traits::{Map, MapRemoveKey, OrderedMap};
use dex::util_types::{Exact, PoolId, PositionInit, Side};
use dex::utils::{swap_if, MinSome};
use dex::{
    BasisPoints, FeeLevel, Float, PoolInfo, PoolV0, Position, PositionId, PositionInfo, PositionV0,
    Range, TickState, Types, BASIS_POINT_DIVISOR,
};
use itertools::Itertools;
use num_traits::Zero;
use std::cmp::Ordering;
use std::ops::Neg;

use super::{EffectiveSqrtPrice, RawFeeLevelsArray, NUM_FEE_LEVELS};
use crate::dex::latest::FeeLevelsArray;
use crate::dex::v0::position_state_ex::eval_position_balance_ufp;
use crate::dex::PairExt;
use crate::dex::Side::{Left, Right};
use crate::{chain, dex, ensure_here, error_here, fp, LiquiditySFP, MAX_EFF_TICK, MIN_EFF_TICK};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AddOrRemove {
    Add,
    Remove,
}

/// Minimal liquidity required to open a position.
/// Should be not too large to enable opening positions with broad range.
/// Should be not too small to limit the error of truncation to 32 frational bits.
/// Current value is chosen to avoid precision loss in conversion to ...X64 types
/// ```
/// assert_eq!(((1 << (64 - f64::MANTISSA_DIGITS)) as f64).recip().to_bits(), 0x3f_40_00_00_00_00_00_00_u64);
/// ```
const MIN_LIQUIDITY: Float = Float::from_bits(0x3f_40_00_00_00_00_00_00_u64);

/// Maximum liquidity required to open a position.
///
/// Should be not too big to allow opening multiple positions without overflowing total liquidity.
///
/// Maximum liquidity is set to be 143 (128+15) bits.
/// This should allow to create 2^(192-143)=2^49 positions which should be enough.
/// ```
/// assert_eq!(143.0f64.exp2().to_bits(), 0x48_e0_00_00_00_00_00_00_u64);
/// ```
#[cfg(feature = "near")]
const MAX_LIQUIDITY: Float = Float::from_bits(0x48_e0_00_00_00_00_00_00_u64);

/// What fraction of amount-in may be underpaid by a trader in an exact-in swap.
/// ```
/// assert_eq!(((1u64<<49) as f64).recip().to_bits(), 0x3c_e0_00_00_00_00_00_00_u64);
/// assert_eq!(1.7763568394002505e-15_f64.to_bits(), 0x3c_e0_00_00_00_00_00_00_u64);
/// ```
const SWAP_MAX_UNDERPAY: Float = Float::from_bits(0x3c_e0_00_00_00_00_00_00_u64);

/// Fee rate on the given fee level
pub fn fee_rate(fee_level: FeeLevel) -> Float {
    let one_over_one_minus_fee_rate = one_over_one_minus_fee_rate(fee_level);
    (one_over_one_minus_fee_rate - Float::one()) / one_over_one_minus_fee_rate
}

/// `1 / sqrt(1 - fee_rate)` for a given fee level
/// This quantity originates from the calculation method and determines the fee rates on each level.
pub fn one_over_sqrt_one_minus_fee_rate(fee_level: FeeLevel) -> Float {
    let tick_index = i32::from(fee_rate_ticks(fee_level));
    // Unwrap must succeed as long as fee_level is valid.
    debug_assert!(Tick::is_valid(tick_index));
    unsafe { Tick::new_unchecked(tick_index) }.spot_sqrtprice()
}

pub fn one_over_one_minus_fee_rate(fee_level: FeeLevel) -> Float {
    let tick_index = i32::from(2 * fee_rate_ticks(fee_level));
    // Unwrap must succeed as long as fee_level is valid.
    debug_assert!(Tick::is_valid(tick_index));
    unsafe { Tick::new_unchecked(tick_index) }.spot_sqrtprice()
}

/// Effective sqrtprice in the opposite direction
///
/// Since the ticks are not precisely equidistant, we use pivot tick for the inversion.
/// TODO: describe in more details
/// Pivot tick may be provided as optional argument, and ideally, its spot sqrtprice
/// should be not more than 1 away from the `eff_sqrtprice`.
/// If pivot tick is not provided, or provided inaccurately, it is found or adjusted
/// (which requires extra computations).
pub fn eff_sqrtprice_opposite_side(
    eff_sqrtprice: Float,
    fee_level: FeeLevel,
    pivot: Option<EffTick>,
) -> Result<Float, ErrorKind> {
    let pivot = find_pivot(pivot.unwrap_or_default(), eff_sqrtprice)?;
    debug_assert!(
        pivot.index() == MAX_EFF_TICK || eff_sqrtprice <= pivot.shifted(1).unwrap().eff_sqrtprice()
    );
    debug_assert!(
        pivot.index() == MIN_EFF_TICK
            || pivot.shifted(-1).unwrap().eff_sqrtprice() <= eff_sqrtprice
    );
    Ok((pivot.eff_sqrtprice() / eff_sqrtprice) * pivot.opposite(fee_level).eff_sqrtprice())
}

/// Evaluate effective sqrtprice from spot sqrtprice
pub fn eff_sqrtprice_from_spot_sqrtprice(spot_sqrtprice: Float, fee_level: FeeLevel) -> Float {
    spot_sqrtprice * one_over_sqrt_one_minus_fee_rate(fee_level)
}

/// Gross liquidity is a factor connecting the total amount paid by a trader in a swap,
/// and the effective sqrtprice shift.
/// `
///     gross_liquidity = liquidity / sqrt(1 - fee_rate)
/// `
pub(crate) fn gross_liquidity_from_net_liquidity(
    net_liqudity: NetLiquidityUFP,
    fee_level: FeeLevel,
) -> GrossLiquidityUFP {
    // The conversion shall not fail, as long as `fee_level` is within the range.
    // See `conversion_one_over_one_minus_fee_rate_to_gross_liquidity_ufp_never_fails_and_within_64_fract_bits`
    let one_over_one_minus_fee_rate =
        GrossLiquidityUFP::try_from(one_over_one_minus_fee_rate(fee_level)).unwrap();
    GrossLiquidityUFP::from(net_liqudity) * one_over_one_minus_fee_rate
}

/// Fee liquidity is a factor connecting LP fee and effective sqrtprice shift
/// `
///     fee_liquidity =
///         = liquidity * fee_rate / sqrt(1-fee_rate) =
///         = net_liquidity * fee_rate / (1-fee_rate) =
///         = net_liquidity * [1/(1-fee_rate) - 1]
/// `
pub(crate) fn fee_liquidity_from_net_liquidity(
    net_liqudity: NetLiquidityUFP,
    fee_level: FeeLevel,
) -> FeeLiquidityUFP {
    let fee_rate_over_one_minus_fee_rate = one_over_one_minus_fee_rate(fee_level) - Float::one();
    // The conversion shall not fail, as long as `fee_level` is within the range.
    // See `conversion_one_minus_one_over_one_minus_fee_rate_to_fee_liquidity_ufp_never_fails_and_within_64_fract_bits`
    let fee_rate_over_one_minus_fee_rate =
        FeeLiquidityUFP::try_from(fee_rate_over_one_minus_fee_rate).unwrap();

    FeeLiquidityUFP::from(net_liqudity) * fee_rate_over_one_minus_fee_rate
}

#[derive(PartialEq, Eq, Clone, Copy)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
enum StepLimit {
    StepComplete,
    LevelActivation,
    TickCrossing,
}

impl<T: Types> PoolV0<T> {
    pub fn eff_sqrtprice(&self, side: Side, fee_level: FeeLevel) -> Float {
        self.eff_sqrtprices[fee_level].value(side)
    }

    pub(crate) fn spot_sqrtprice(&self, side: Side, fee_level: FeeLevel) -> Float {
        self.eff_sqrtprice(side, fee_level) / one_over_sqrt_one_minus_fee_rate(fee_level)
    }

    pub fn spot_price(&self, side: Side, fee_level: FeeLevel) -> Float {
        let spot_sqrtprice = self.spot_sqrtprice(side, fee_level);
        spot_sqrtprice * spot_sqrtprice
    }

    pub(crate) fn spot_sqrtprices(&self, side: Side) -> RawFeeLevelsArray<Float> {
        fee_levels().map(|fee_level| self.spot_sqrtprice(side, fee_level))
    }

    fn liquidity(&self, fee_level: FeeLevel) -> Liquidity {
        // Proven with test below that for any fee_level, less than NUM_FEE_LEVELS, conversion wont fail
        let one_over_sqrt_one_minus_fee_rate =
            Liquidity::try_from(one_over_sqrt_one_minus_fee_rate(fee_level)).unwrap();

        self.net_liquidities[fee_level] * one_over_sqrt_one_minus_fee_rate
    }

    pub(crate) fn liquidities(&self) -> RawFeeLevelsArray<Liquidity> {
        fee_levels().map(|fee_level| self.liquidity(fee_level))
    }

    pub fn gross_liquidity(&self, fee_level: FeeLevel) -> GrossLiquidityUFP {
        gross_liquidity_from_net_liquidity(self.net_liquidities[fee_level], fee_level)
    }

    pub fn fee_liquidity(&self, fee_level: FeeLevel) -> FeeLiquidityUFP {
        fee_liquidity_from_net_liquidity(self.net_liquidities[fee_level], fee_level)
    }

    /// Sum of gross liquidities on levels from 0 to `top_level` (including)
    fn sum_gross_liquidities(&self, top_level: FeeLevel) -> GrossLiquidityUFP {
        self.net_liquidities[..=top_level]
            .iter()
            .enumerate()
            .map(|(fee_level, net_liquidity)| {
                gross_liquidity_from_net_liquidity(
                    *net_liquidity,
                    FeeLevel::try_from(fee_level).unwrap(),
                )
            })
            .sum()
    }

    /// Sum of fee liquidities on levels from 0 to `top_level` (including)
    fn sum_fee_liquidities(&self, top_level: FeeLevel) -> FeeLiquidityUFP {
        self.net_liquidities[..=top_level]
            .iter()
            .enumerate()
            .map(|(fee_level, net_liquidity)| {
                fee_liquidity_from_net_liquidity(
                    *net_liquidity,
                    FeeLevel::try_from(fee_level).unwrap(),
                )
            })
            .sum()
    }

    fn sum_position_reserves(&self) -> (AmountUFP, AmountUFP) {
        let mut amounts = (AmountUFP::zero(), AmountUFP::zero());
        for level in 0..NUM_FEE_LEVELS {
            amounts.0 += self.position_reserves[level].0;
            amounts.1 += self.position_reserves[level].1;
        }
        amounts
    }

    pub(crate) fn position_reserves(&self) -> RawFeeLevelsArray<(AmountUFP, AmountUFP)> {
        RawFeeLevelsArray::from(self.position_reserves)
    }

    pub fn pool_info(&self, side: Side) -> Result<PoolInfo, Error> {
        let total_reserves = swap_if(side == Side::Right, self.total_reserves);
        let position_reserves_ufp = swap_if(side == Side::Right, self.sum_position_reserves());
        let position_reserves = position_reserves_ufp
            .try_map_into::<Amount, _>()
            .map_err(|e| error_here!(e))?;
        Ok(PoolInfo {
            total_reserves,
            position_reserves,
            spot_sqrtprices: self.spot_sqrtprices(side),
            liquidities: self.liquidities(),
            fee_rates: fee_rates_ticks(),
            fee_divisor: BASIS_POINT_DIVISOR,
        })
    }

    #[cfg(feature = "near")]
    #[cfg(feature = "smart-routing")]
    pub(crate) fn primitive_price(&self) -> Liquidity {
        let left_u128x128: Liquidity = From::from(self.total_reserves.0);
        let right_u128x128: Liquidity = From::from(self.total_reserves.1);
        left_u128x128 / right_u128x128
    }

    #[cfg(feature = "smart-routing")]
    pub(crate) fn total_liquidity(&self) -> Liquidity {
        self.liquidities().into_iter().sum()
    }

    pub(crate) fn get_position_info(
        &self,
        pool_id: &PoolId,
        position_id: PositionId,
    ) -> Result<PositionInfo> {
        self.positions
            .inspect(&position_id, |Position::V0(ref pos)| {
                Ok(PositionInfo {
                    tokens_ids: pool_id.as_refs().map(Clone::clone),
                    balance: self.eval_position_balance(pos)?,
                    init_sqrtprice: pos.init_sqrtprice,
                    range_ticks: pos.tick_bounds,
                    reward_since_last_withdraw: self.position_reward(pos, false)?,
                    reward_since_creation: self.position_reward(pos, true)?,
                })
            })
            .ok_or(error_here!(ErrorKind::PositionDoesNotExist))?
    }

    /// Fast check if pool is not empty. Relies on that `eff_sqrtprices` are reset.
    pub(crate) fn is_spot_price_set(&self) -> bool {
        // When pool is just created, or all positions are deleted,
        // we set all eff_sqrtprices to zero, which is otherwise invalid.
        !self.eff_sqrtprice(Side::Left, 0).is_zero()
    }

    /// Reliable check if pool is not empty. Relies on that when last position is closed,
    /// no active ticks remain.
    pub(crate) fn contains_any_positions(&self) -> bool {
        self.tick_states
            .iter()
            .any(|tick_states| tick_states.inspect_min(|_, _| ()).is_some())
    }

    /// Initialize effective prices, pivot, top active level and active side,
    /// based on parameters of the first position.
    pub fn init_pool_from_position(
        &mut self,
        left_max: Float,
        right_max: Float,
        tick_low: Tick,
        tick_high: Tick,
        fee_level: FeeLevel,
    ) -> Result<()> {
        let (eff_sqrtprice, side) =
            eval_initial_eff_sqrtprice(left_max, right_max, tick_low, tick_high, fee_level)?;
        self.init_pool_from_eff_sqrtprice(eff_sqrtprice, side, fee_level)?;
        Ok(())
    }

    /// Initialize effective prices, pivot, top active level and active side,
    /// based on a given effective price on a given level.
    pub fn init_pool_from_eff_sqrtprice(
        &mut self,
        eff_sqrtprice: Float,
        side: Side,
        fee_level: FeeLevel,
    ) -> Result<()> {
        self.pivot = find_pivot(EffTick::default(), eff_sqrtprice).map_err(|e| error_here!(e))?;
        for i_fee_level in 0..NUM_FEE_LEVELS {
            let pivot_opposite_this_level = EffTick::new(
                self.pivot.index() - i32::from(fee_rate_ticks(fee_level))
                    + i32::from(fee_rate_ticks(i_fee_level)),
            )
            .map_err(|e| error_here!(e))?;
            let eff_sqrtprice_this_level = (eff_sqrtprice / self.pivot.eff_sqrtprice())
                * pivot_opposite_this_level.eff_sqrtprice();
            self.eff_sqrtprices[i_fee_level] = EffectiveSqrtPrice::from_value(
                eff_sqrtprice_this_level,
                side,
                i_fee_level,
                Some(self.pivot),
            )
            .map_err(|e| error_here!(e))?;
        }

        self.top_active_level = 0;
        self.active_side = side;

        Ok(())
    }

    /// Evaluate net liquidity corresponding to `max_amounts` and `tick_bounds` on the given `fee_level`.
    /// Notice: `self.next_active_ticks` must be already updated with `tick_bounds`.
    fn eval_accounted_net_liquidity(
        &self,
        max_amounts: (Float, Float),
        (tick_low, tick_high): (Tick, Tick),
        fee_level: FeeLevel,
    ) -> Result<NetLiquidityUFP> {
        // Determine if the spot price is below, between, or above the position bounds.
        // Here we determine it based on the next ticks to cross. However, if the spot
        // price is exactly on one of the ticks, `spot_price_wrt_position_bounds`
        // is not well defined:
        let spot_price_wrt_position_bounds =
            self.cmp_spot_price_to_position_range(fee_level, (tick_low, tick_high))?;

        // Handle the cases when the spot price is exactly on one of the bounds:
        let is_on_low_tick = self.eff_sqrtprice(Side::Left, fee_level)
            == tick_low.eff_sqrtprice(fee_level, Side::Left)
            || self.eff_sqrtprice(Side::Right, fee_level)
                == tick_low.eff_sqrtprice(fee_level, Side::Right);
        let is_on_high_tick = self.eff_sqrtprice(Side::Left, fee_level)
            == tick_high.eff_sqrtprice(fee_level, Side::Left)
            || self.eff_sqrtprice(Side::Right, fee_level)
                == tick_high.eff_sqrtprice(fee_level, Side::Right);
        let spot_price_wrt_position_bounds = if is_on_low_tick {
            Ordering::Less
        } else if is_on_high_tick {
            Ordering::Greater
        } else {
            spot_price_wrt_position_bounds
        };

        let net_liquidity_float = match spot_price_wrt_position_bounds {
            Ordering::Less => {
                // Spot price is below or at tick_low -- position consists of right token only
                ensure_here!(
                    max_amounts[Side::Right] > Float::zero(),
                    ErrorKind::WrongRatio
                );
                let eff_sqrtprice_right_high = tick_low.eff_sqrtprice(fee_level, Side::Right);
                let eff_sqrtprice_right_low = tick_high.eff_sqrtprice(fee_level, Side::Right);
                ensure_here!(
                    eff_sqrtprice_right_high > eff_sqrtprice_right_low,
                    ErrorKind::InternalLogicError
                );
                let liquidity_right = max_amounts[Side::Right]
                    / next_up(eff_sqrtprice_right_high - eff_sqrtprice_right_low);
                ensure_here!(liquidity_right.is_normal(), ErrorKind::InternalLogicError); // implies != 0

                liquidity_right
            }
            Ordering::Equal => {
                // Spot price is between tick_low and tick_high (and not either of the bounds)
                // -- position consists of both tokens.
                ensure_here!(
                    max_amounts[Side::Right] > Float::zero(),
                    ErrorKind::WrongRatio
                );
                ensure_here!(
                    max_amounts[Side::Left] > Float::zero(),
                    ErrorKind::WrongRatio
                );
                let eff_sqrtprice_left = self.eff_sqrtprice(Side::Left, fee_level);
                let eff_sqrtprice_left_low = tick_low.eff_sqrtprice(fee_level, Side::Left);
                ensure_here!(
                    eff_sqrtprice_left > eff_sqrtprice_left_low,
                    ErrorKind::InternalLogicError
                );
                let liquidity_left =
                    max_amounts[Side::Left] / next_up(eff_sqrtprice_left - eff_sqrtprice_left_low);

                let eff_sqrtprice_right = self.eff_sqrtprice(Side::Right, fee_level);
                let eff_sqrtprice_right_low = tick_high.eff_sqrtprice(fee_level, Side::Right);
                ensure_here!(
                    eff_sqrtprice_right > eff_sqrtprice_right_low,
                    ErrorKind::InternalLogicError
                );
                let liquidity_right = max_amounts[Side::Right]
                    / next_up(eff_sqrtprice_right - eff_sqrtprice_right_low);

                ensure_here!(liquidity_left.is_normal(), ErrorKind::InternalLogicError); // implies != 0
                ensure_here!(liquidity_right.is_normal(), ErrorKind::InternalLogicError); // implies != 0

                liquidity_left.min(liquidity_right)
            }
            Ordering::Greater => {
                // Spot price is above tick_high -- position consists of left token only
                ensure_here!(
                    max_amounts[Side::Left] > Float::zero(),
                    ErrorKind::WrongRatio
                );
                let eff_sqrtprice_left_high = tick_high.eff_sqrtprice(fee_level, Side::Left);
                let eff_sqrtprice_left_low = tick_low.eff_sqrtprice(fee_level, Side::Left);
                ensure_here!(
                    eff_sqrtprice_left_high > eff_sqrtprice_left_low,
                    ErrorKind::InternalLogicError
                );
                let liquidity_left = max_amounts[Side::Left]
                    / next_up(eff_sqrtprice_left_high - eff_sqrtprice_left_low);
                ensure_here!(liquidity_left.is_normal(), ErrorKind::InternalLogicError); // implies != 0

                liquidity_left
            }
        };

        ensure_here!(
            net_liquidity_float >= MIN_LIQUIDITY,
            ErrorKind::LiquidityTooSmall
        );

        ensure_here!(
            net_liquidity_float <= MAX_LIQUIDITY,
            ErrorKind::LiquidityTooBig
        );

        let net_liquidity_ufp =
            Liquidity::try_from(net_liquidity_float).map_err(|e| error_here!(e))?;

        Ok(net_liquidity_ufp)
    }

    fn find_next_active_tick_on_level(
        &self,
        begin_excluding: Tick,
        fee_level: FeeLevel,
        side: Side,
    ) -> Option<Tick> {
        match side {
            Side::Left => {
                self.tick_states[fee_level].inspect_above(&begin_excluding, |tick: &Tick, _| *tick)
            }
            Side::Right => {
                self.tick_states[fee_level].inspect_below(&begin_excluding, |tick: &Tick, _| *tick)
            }
        }
    }

    /// Update `self.next_active_ticks_left` and `self.next_active_ticks_right`
    /// with newly inserted tick (upon opening a position).
    fn update_next_active_ticks(&mut self, new_tick: Tick, fee_level: FeeLevel) -> Result<()> {
        // The implementation must account, among other, for cases when:
        //  - one of the prices (left or right) is exactly on the new tick, and the other
        //    price is very close to it, but not exactly equal to it
        //  - when both prices exactly on the tick, but this tick was already active --
        //    in such case the next active ticks must not change

        if self.eff_sqrtprice(Side::Left, fee_level) < new_tick.eff_sqrtprice(fee_level, Side::Left)
        {
            ensure_here!(
                self.eff_sqrtprice(Side::Right, fee_level)
                    >= new_tick.eff_sqrtprice(fee_level, Side::Right),
                ErrorKind::InternalLogicError
            );
            ensure_here!(
                Some(new_tick) > self.next_active_ticks_right[fee_level],
                ErrorKind::InternalLogicError
            );
            self.next_active_ticks_left[fee_level] =
                self.next_active_ticks_left[fee_level].min_some(Some(new_tick));
        } else {
            ensure_here!(
                self.eff_sqrtprice(Side::Right, fee_level)
                    <= new_tick.eff_sqrtprice(fee_level, Side::Right),
                ErrorKind::InternalLogicError
            );

            if self.next_active_ticks_left[fee_level] == Some(new_tick) {
                ensure_here!(
                    self.eff_sqrtprice(Side::Left, fee_level)
                        == new_tick.eff_sqrtprice(fee_level, Side::Left),
                    ErrorKind::InternalLogicError
                );
            } else {
                self.next_active_ticks_right[fee_level] =
                    self.next_active_ticks_right[fee_level].max(Some(new_tick));
            }
        }
        Ok(())
    }

    /// Determine if current spot price on `fee_level` is within `ticks_range`, lesser than or greater than `ticks_range`.
    ///
    /// Comparing the prices is not reliable, as the price may lay exactly on a tick,
    /// and we must clearly and unambiguously distinguish, whether a tick was already crossed.
    /// Therefore, we compare next active ticks.
    ///
    /// Notice: `self.next_active_ticks_left` and `self.next_active_ticks_right` must already be updated with `ticks_range` ticks.
    ///
    /// # Arguments
    ///
    /// - `fee_level` - fee level, spot price may be different on different fee levels
    /// - `ticks_range` - must be initialized ticks, or may return an error
    ///
    /// # Returns
    /// - `Ok(Ordering::Equal)` if spot price is within `ticks_range`
    /// - `Ok(Ordering::Less)` if spot price is less than lower bound of `ticks_range`
    /// - `Ok(Ordering::Greater)` if spot price is greater than upper bound of `ticks_range`
    /// - `Err(_)` if some ticks were not found
    pub(crate) fn cmp_spot_price_to_position_range(
        &self,
        fee_level: FeeLevel,
        ticks_range: (Tick, Tick),
    ) -> Result<Ordering> {
        match (
            self.next_active_ticks_left[fee_level],
            self.next_active_ticks_right[fee_level],
        ) {
            (Some(next_active_tick_left), Some(next_active_tick_right)) => {
                if ticks_range[Left] <= next_active_tick_right
                    && next_active_tick_left <= ticks_range[Right]
                {
                    Ok(Ordering::Equal)
                } else if ticks_range[Right] <= next_active_tick_right {
                    Ok(Ordering::Greater)
                } else if next_active_tick_left <= ticks_range[Left] {
                    Ok(Ordering::Less)
                } else {
                    Err(error_here!(ErrorKind::InternalLogicError))
                }
            }
            (Some(next_active_tick_left), None) if next_active_tick_left <= ticks_range[Left] => {
                Ok(Ordering::Less)
            }
            (None, Some(next_active_tick_right))
                if ticks_range[Right] <= next_active_tick_right =>
            {
                Ok(Ordering::Greater)
            }
            _ => Err(error_here!(ErrorKind::InternalTickNotFound)),
        }
    }

    /// Evaluate amounts of tokens to be deposited in the pool,
    /// and actually accunted net liquidity of the position.
    #[allow(clippy::too_many_lines)] // Refactor?
    #[allow(clippy::needless_pass_by_value)] // `position` is actually deconstructed, no idea why Clippy complains
    pub fn open_position(
        &mut self,
        position: PositionInit,
        fee_level: FeeLevel,
        position_id: PositionId,
        factory: &mut dyn dex::ItemFactory<T>,
    ) -> Result<((Amount, Amount), NetLiquidityUFP)> {
        let PositionInit {
            amount_ranges:
                (
                    Range {
                        min: left_min,
                        max: left_max,
                    },
                    Range {
                        min: right_min,
                        max: right_max,
                    },
                ),
            ticks_range,
        } = position;
        let left_min: Amount = left_min.into();
        let left_max: Amount = left_max.into();
        let right_min: Amount = right_min.into();
        let right_max: Amount = right_max.into();

        let (tick_low, tick_high) = Tick::unwrap_range(ticks_range).map_err(|e| error_here!(e))?;

        ensure_here!(left_max >= left_min, ErrorKind::InvalidParams);
        ensure_here!(right_max >= right_min, ErrorKind::InvalidParams);
        ensure_here!(tick_high > tick_low, ErrorKind::InvalidParams);

        let left_max_float: Float = next_down(left_max.into());
        let right_max_float: Float = next_down(right_max.into());

        if !self.is_spot_price_set() {
            self.init_pool_from_position(
                left_max_float,
                right_max_float,
                tick_low,
                tick_high,
                fee_level,
            )?;
        }

        // Check if any of the newly activated ticks should become the next tick to cross:
        for new_tick in [tick_low, tick_high] {
            self.update_next_active_ticks(new_tick, fee_level)?;
        }

        let accounted_net_liquidity = self.eval_accounted_net_liquidity(
            (left_max_float, right_max_float),
            (tick_low, tick_high),
            fee_level,
        )?;

        let init_acc_lp_fees_per_fee_liquidity =
            self.acc_range_lp_fees_per_fee_liquidity(fee_level, (tick_low, tick_high))?;
        let init_sqrtprice = self.spot_sqrtprice(Side::Right, fee_level);

        self.positions.update_or_insert(
            &position_id,
            || {
                factory.new_position(
                    fee_level,
                    accounted_net_liquidity,
                    init_acc_lp_fees_per_fee_liquidity,
                    (tick_low, tick_high),
                    init_sqrtprice,
                )
            },
            |_, position_exists| {
                if position_exists {
                    Err(error_here!(ErrorKind::PositionAlreadyExists))
                } else {
                    Ok(())
                }
            },
        )?;

        self.tick_states[fee_level].update_or_insert(
            &tick_low,
            || factory.new_default_tick(),
            |TickState::V0(tick_state), _exists| {
                tick_state.net_liquidity_change += LiquiditySFP::from(accounted_net_liquidity);
                tick_state.reference_counter += 1;
                Ok(())
            },
        )?;

        self.tick_states[fee_level].update_or_insert(
            &tick_high,
            || factory.new_default_tick(),
            |TickState::V0(tick_state), _exists| {
                tick_state.net_liquidity_change =
                    tick_state.net_liquidity_change - LiquiditySFP::from(accounted_net_liquidity); // TODO: imlement SubAssign
                tick_state.reference_counter += 1;
                Ok(())
            },
        )?;

        let accounted_deposit_ufp = eval_position_balance_ufp(
            accounted_net_liquidity,
            tick_low,
            tick_high,
            self.eff_sqrtprice(Side::Left, fee_level),
            self.eff_sqrtprice(Side::Right, fee_level),
            fee_level,
        )?;

        // Add the accounted deposit to the pool:
        self.position_reserves[fee_level].0 += accounted_deposit_ufp.0;
        self.position_reserves[fee_level].1 += accounted_deposit_ufp.1;

        // In case the spot price is within the position range, we need to add up the deposited liquidity
        // to the current active liquidity.
        if self
            .cmp_spot_price_to_position_range(fee_level, (tick_low, tick_high))?
            .is_eq()
        {
            self.net_liquidities[fee_level] += accounted_net_liquidity;
        }

        // We can't charge LP with a non-integer amount of tokens, so we round the amounts up.
        // The difference will effectively go into the protocol fee.
        let actual_deposit = (
            Amount::try_from(accounted_deposit_ufp.0.ceil()).map_err(|e| error_here!(e))?,
            Amount::try_from(accounted_deposit_ufp.1.ceil()).map_err(|e| error_here!(e))?,
        );

        // Accounted deposit must never exceed the actual one:
        ensure_here!(actual_deposit.0 <= left_max, ErrorKind::InternalLogicError);
        ensure_here!(actual_deposit.1 <= right_max, ErrorKind::InternalLogicError);

        // Check if token ranges are consistent with the current spot price:
        ensure_here!(actual_deposit.0 >= left_min, ErrorKind::WrongRatio);
        ensure_here!(actual_deposit.1 >= right_min, ErrorKind::WrongRatio);

        // At least one of the tokens must be deposited:
        ensure_here!(
            actual_deposit.0 >= Amount::zero() || actual_deposit.1 >= Amount::zero(),
            ErrorKind::InternalLogicError
        );

        ensure_here!(
            AmountSFP::from(accounted_deposit_ufp.0) <= AmountSFP::from(actual_deposit.0),
            ErrorKind::InternalDepositMoreThanMax
        );
        ensure_here!(
            AmountSFP::from(accounted_deposit_ufp.1) <= AmountSFP::from(actual_deposit.1),
            ErrorKind::InternalDepositMoreThanMax
        );

        self.total_reserves.0 = self
            .total_reserves
            .0
            .checked_add(actual_deposit.0)
            .ok_or(error_here!(ErrorKind::DepositWouldOverflow))?;
        self.total_reserves.1 = self
            .total_reserves
            .1
            .checked_add(actual_deposit.1)
            .ok_or(error_here!(ErrorKind::DepositWouldOverflow))?;

        Ok((actual_deposit, accounted_net_liquidity))
    }

    /// Withdraw LP reward fees and close position.
    ///
    /// We intentionally prohibit closing position without withdrawing the reward.
    ///
    /// Returns: (`lp_reward_fees`, `position_balance`)
    pub(crate) fn withdraw_fee_and_close_position(
        &mut self,
        position_id: u64,
    ) -> Result<((Amount, Amount), (Amount, Amount))> {
        let fees = self.withdraw_fee(position_id)?;

        let (fee_level, balance_ufp, net_liquidity, ticks_range) = self
            .positions
            .inspect(&position_id, |Position::V0(ref pos)| {
                Ok((
                    pos.fee_level,
                    pos.eval_position_balance_ufp(
                        self.eff_sqrtprice(Side::Left, pos.fee_level),
                        self.eff_sqrtprice(Side::Right, pos.fee_level),
                    )?,
                    pos.net_liquidity,
                    pos.tick_bounds,
                ))
            })
            .transpose()?
            .ok_or(error_here!(ErrorKind::PositionDoesNotExist))?;

        self.positions.remove(&position_id);

        let balance = balance_ufp
            .try_map_into::<Amount, _>()
            .map_err(|e| error_here!(e))?;

        self.total_reserves.0 -= balance.0;
        self.total_reserves.1 -= balance.1;
        self.position_reserves[fee_level].0 -= balance_ufp.0;
        self.position_reserves[fee_level].1 -= balance_ufp.1;

        if self
            .cmp_spot_price_to_position_range(fee_level, ticks_range)?
            .is_eq()
        {
            self.net_liquidities[fee_level] -= net_liquidity;
        }

        for (tick, is_lower) in [(ticks_range.0, true), (ticks_range.1, false)] {
            let tick_deactivated = self.tick_states[fee_level].try_update(
                &tick,
                |TickState::V0(ref mut tick_state)| {
                    if is_lower {
                        tick_state.net_liquidity_change =
                            tick_state.net_liquidity_change - LiquiditySFP::from(net_liquidity);
                    } else {
                        tick_state.net_liquidity_change += LiquiditySFP::from(net_liquidity);
                    }
                    tick_state.reference_counter -= 1;
                    Ok(tick_state.reference_counter == 0)
                },
            )?;

            if tick_deactivated {
                self.tick_states[fee_level].remove(&tick);
                // cross-check of correct storage of ticks
                ensure_here!(
                    !self.tick_states[fee_level].contains_key(&tick),
                    ErrorKind::InternalTickNotDeleted
                );

                // update next active tick:
                if self.next_active_ticks_left[fee_level] == Some(tick) {
                    self.next_active_ticks_left[fee_level] =
                        self.find_next_active_tick_on_level(tick, fee_level, Side::Left);
                }
                if self.next_active_ticks_right[fee_level] == Some(tick) {
                    self.next_active_ticks_right[fee_level] =
                        self.find_next_active_tick_on_level(tick, fee_level, Side::Right);
                }
            }
        }

        // Reset pool state if the last position is closed
        if !self.contains_any_positions() {
            self.eff_sqrtprices = FeeLevelsArray::default();
            self.top_active_level = 0;
            ensure_here!(
                self.next_active_ticks_left.into_iter().all(Option::is_none),
                ErrorKind::InternalLogicError
            );
            ensure_here!(
                self.next_active_ticks_right
                    .into_iter()
                    .all(Option::is_none),
                ErrorKind::InternalLogicError
            );
        }

        Ok((fees, balance))
    }

    /// Amount of tokens locked in position
    fn eval_position_balance(&self, pos: &PositionV0<T>) -> Result<(Amount, Amount), Error> {
        let balances_ufp = pos.eval_position_balance_ufp(
            self.eff_sqrtprice(Side::Left, pos.fee_level),
            self.eff_sqrtprice(Side::Right, pos.fee_level),
        )?;

        let balance = balances_ufp
            .try_map_into::<Amount, _>()
            .map_err(|e| error_here!(e))?;

        Ok(balance)
    }

    pub(crate) fn withdraw_fee(&mut self, position_id: u64) -> Result<(Amount, Amount)> {
        let (reward_ufp, acc_lp_fees_per_fee_liquidity) = self
            .positions
            .inspect(&position_id, |Position::V0(ref pos)| {
                let acc_lp_fees_per_fee_liquidity =
                    self.acc_range_lp_fees_per_fee_liquidity(pos.fee_level, pos.tick_bounds)?;
                let reward_ufp = self.position_reward_ufp(pos, false)?;
                Ok((reward_ufp, acc_lp_fees_per_fee_liquidity))
            })
            .ok_or(error_here!(ErrorKind::PositionDoesNotExist))??;

        let reward = reward_ufp
            .try_map_into::<Amount, _>()
            .map_err(|e| error_here!(e))?;

        self.positions
            .update(&position_id, |Position::V0(ref mut pos)| {
                self.total_reserves.0 -= reward.0;
                self.total_reserves.1 -= reward.1;
                self.acc_lp_fee.0 -= reward_ufp.0;
                self.acc_lp_fee.1 -= reward_ufp.1;

                pos.unwithdrawn_acc_lp_fees_per_fee_liquidity = acc_lp_fees_per_fee_liquidity;

                Ok(())
            })
            .ok_or(error_here!(ErrorKind::PositionDoesNotExist))??;

        Ok(reward)
    }

    fn accumulate_lp_fee_per_fee_liquidity(
        &mut self,
        side: Side,
        top_active_level: FeeLevel,
        lp_fee_per_fee_liquidity: LPFeePerFeeLiquidity,
    ) {
        self.acc_lp_fees_per_fee_liquidity[top_active_level][side] += lp_fee_per_fee_liquidity;
    }

    /// Global accumulated LP fee (one side) per net liquidity, since the very beginning of dex operation.
    fn acc_lp_fee_per_fee_liquidity(
        &self,
        side: Side,
        fee_level: FeeLevel,
    ) -> LPFeePerFeeLiquidity {
        self.acc_lp_fees_per_fee_liquidity[fee_level..NUM_FEE_LEVELS]
            .iter()
            .map(|acc_lp_fees_per_fee_liquidity| acc_lp_fees_per_fee_liquidity[side])
            .sum()
    }

    /// Global accumulated LP fees (both sides) per net liquidity, since the very beginning of dex operation.
    pub(crate) fn acc_lp_fees_per_fee_liquidity(
        &self,
        fee_level: FeeLevel,
    ) -> (LPFeePerFeeLiquidity, LPFeePerFeeLiquidity) {
        (
            self.acc_lp_fee_per_fee_liquidity(Side::Left, fee_level),
            self.acc_lp_fee_per_fee_liquidity(Side::Right, fee_level),
        )
    }

    fn accumulate_lp_fee(
        &mut self,
        side: Side,
        fee_level: FeeLevel,
        lp_fee_per_fee_liquidity: LPFeePerFeeLiquidity,
    ) -> Result<()> {
        ensure_here!(
            lp_fee_per_fee_liquidity.non_negative,
            ErrorKind::InternalLogicError
        );
        let lp_fee_per_fee_liquidity = AmountUFP::from(lp_fee_per_fee_liquidity.value);
        let sum_fee_liquidities =
            AmountUFP::try_from(self.sum_fee_liquidities(fee_level)).map_err(|e| error_here!(e))?;
        // TODO: provide comments and check on other blockchains
        ensure_here!(
            sum_fee_liquidities.0 .0[0] == 0,
            ErrorKind::InternalLogicError
        );
        ensure_here!(
            sum_fee_liquidities.0 .0[1] == 0,
            ErrorKind::InternalLogicError
        );
        self.acc_lp_fee[side] += lp_fee_per_fee_liquidity * sum_fee_liquidities;
        Ok(())
    }

    fn nearest_active_ticks(
        &self,
        side: Side,
        top_active_level: FeeLevel,
    ) -> Vec<(FeeLevel, Tick)> {
        // pick next active ticks for the given swap direction:
        let next_active_ticks = match side {
            Side::Left => self.next_active_ticks_left,
            Side::Right => self.next_active_ticks_right,
        };

        // consider only ticks on the active levels (optimization):
        let next_active_ticks_on_active_levels =
            next_active_ticks.iter().take((top_active_level + 1).into());

        // filter out levels where next active tick is None
        #[allow(clippy::cast_possible_truncation)]
        let available_next_active_ticks = next_active_ticks_on_active_levels
            .enumerate()
            .filter_map(|(level, next_tick)| {
                next_tick.map(|next_tick| (level as FeeLevel, next_tick))
            });

        // function to compare ticks by effective (sqrt)price in the current swap direction:
        let compare_ticks_by_eff_price =
            |&(level_a, tick_a): &(FeeLevel, Tick), &(level_b, tick_b): &(FeeLevel, Tick)| {
                let eff_tick_a = EffTick::from_tick(tick_a, level_a, side);
                let eff_tick_b = EffTick::from_tick(tick_b, level_b, side);
                eff_tick_a.cmp(&eff_tick_b)
            };

        // select ticks with the lowest effective price in the direction of current swap
        available_next_active_ticks.min_set_by(compare_ticks_by_eff_price)
    }

    /// TODO: COMMENT
    /// `self.active_side` and `self.top_active_level` must be set
    fn eval_required_new_eff_sqrtprice_exact_in(
        &self,
        amount: Float,
        sum_gross_liquidities: Float,
    ) -> Float {
        let eff_sqrtprice = self.eff_sqrtprice(self.active_side, self.top_active_level);

        if sum_gross_liquidities.is_zero() {
            return Float::MAX;
        }

        let eff_sqrtprice_shift = amount / sum_gross_liquidities;

        if eff_sqrtprice > eff_sqrtprice_shift {
            next_down(eff_sqrtprice) + eff_sqrtprice_shift
        } else {
            eff_sqrtprice + next_down(eff_sqrtprice_shift)
        }
        .max(eff_sqrtprice)
    }

    /// TODO: COMMENT
    /// `self.active_side` and `self.top_active_level` must be set
    fn eval_required_new_eff_sqrtprice_exact_out(
        &self,
        amount: Float,
        sum_gross_liquidities: Float,
    ) -> Result<Float> {
        let eff_sqrtprice = self.eff_sqrtprice(self.active_side, self.top_active_level);

        if sum_gross_liquidities.is_zero() {
            return Ok(Float::MAX);
        }

        let inverse_eff_sqrtprice = eff_sqrtprice.recip();
        let required_inverse_eff_sqrtprice_shift = amount / sum_gross_liquidities;

        // Required shift of inverse_eff_sqrtprice may exceed its current value.
        // This corresponds to the case when current active liquidity would not
        // be sufficient to fulfill the swap, even if price is shifted to infinity.
        if required_inverse_eff_sqrtprice_shift >= next_down(inverse_eff_sqrtprice) {
            // Swap amount exceeds available liquidity on the current set of active levels
            // (assuming the same liquidity up to infinite price)
            return Ok(Float::MAX);
        }

        // There is enough active liquidity, assuming liquidity would remain
        // the same towards infinite price.

        // Lower bits of `required_inverse_eff_sqrtprice_shift` will be lost in subtraction,
        // because `required_inverse_eff_sqrtprice_shift` < `inverse_eff_sqrtprice`.
        // We need to subtract _at_least_ `required_inverse_eff_sqrtprice_shift` (as trader
        // must pay for _at_least_ `amount` tokens), therefore we do next_down:
        let new_inverse_eff_sqrtprice =
            next_down(inverse_eff_sqrtprice) - required_inverse_eff_sqrtprice_shift;
        // As `required_inverse_eff_sqrtprice_shift` is strictly less than `next_down(inverse_eff_sqrtprice)`,
        // the minimal difference equals to the significance of the lowest bit of `next_down(inverse_eff_sqrtprice)`,
        // which should still be normal:
        ensure_here!(
            new_inverse_eff_sqrtprice.is_normal(),
            ErrorKind::InternalLogicError
        );

        // Cross-check that the price shift is sufficient to swap the required amount:
        ensure_here!(
            (eff_sqrtprice.recip() - new_inverse_eff_sqrtprice) * sum_gross_liquidities >= amount,
            ErrorKind::InternalLogicError
        );

        // Invert the price back with rounding up:
        let new_eff_sqrtprice = next_up(new_inverse_eff_sqrtprice.recip());

        // Ensure that the price did change, at least by the LSB:
        let new_eff_sqrtprice = new_eff_sqrtprice.max(next_up(eff_sqrtprice));

        // Cross-check that the price changed in both directions
        ensure_here!(
            new_eff_sqrtprice > eff_sqrtprice,
            ErrorKind::InternalLogicError
        );

        Ok(new_eff_sqrtprice)
    }

    fn accumulate_fees(
        &mut self,
        eff_sqrtprice_shift: Float,
        protocol_fee_fraction: BasisPoints,
    ) -> Result<()> {
        // TODO: optimize lossy conversion:
        // TODO: optimize division by BASIS_POINT_DIVISOR
        // let eff_sqrtprice_shift_for_fee = next_down(new_eff_sqrtprice).max(eff_sqrtprice) - eff_sqrtprice;
        let eff_sqrtprice_shift_for_fee = eff_sqrtprice_shift;
        let lp_fee_per_fee_liquidity = if eff_sqrtprice_shift_for_fee > Float::one() {
            LPFeePerFeeLiquidity::try_from(eff_sqrtprice_shift_for_fee)
                .map_err(|e| error_here!(e))?
                * LPFeePerFeeLiquidity::from(u128::from(
                    BASIS_POINT_DIVISOR - protocol_fee_fraction,
                ))
                / LPFeePerFeeLiquidity::from(u128::from(BASIS_POINT_DIVISOR))
        } else {
            LPFeePerFeeLiquidity::try_from(eff_sqrtprice_shift_for_fee * Float::from(1u128 << 48))
                .map_err(|e| error_here!(e))?
                * LPFeePerFeeLiquidity::from(u128::from(
                    BASIS_POINT_DIVISOR - protocol_fee_fraction,
                ))
                / LPFeePerFeeLiquidity::from(u128::from(BASIS_POINT_DIVISOR))
                / LPFeePerFeeLiquidity::from(1u128 << 48)
        };

        self.accumulate_lp_fee(
            self.active_side,
            self.top_active_level,
            lp_fee_per_fee_liquidity,
        )?;
        self.accumulate_lp_fee_per_fee_liquidity(
            self.active_side,
            self.top_active_level,
            lp_fee_per_fee_liquidity,
        );

        Ok(())
    }

    /// Returns: `amount_in`, `amount_out`, `step_limit`
    fn try_step_to_price(
        &mut self,
        mut new_eff_sqrtprice: Float,
        sum_gross_liquidities: Float,
        protocol_fee_fraction: BasisPoints,
    ) -> Result<(Float, AmountUFP, StepLimit)> {
        ensure_here!(
            new_eff_sqrtprice >= self.eff_sqrtprice(self.active_side, self.top_active_level),
            ErrorKind::InternalLogicError
        );

        // TODO: change to AmountUFP?
        // Check if new level is activated earlier
        let mut limit_kind = StepLimit::StepComplete;
        if self.top_active_level < NUM_FEE_LEVELS - 1 {
            let next_level_eff_sqrtprice =
                self.eff_sqrtprice(self.active_side, self.top_active_level + 1);
            if next_level_eff_sqrtprice <= new_eff_sqrtprice {
                new_eff_sqrtprice = next_level_eff_sqrtprice;
                limit_kind = StepLimit::LevelActivation;
            }
        }

        // Check if tick crossing happens earlier
        let nearest_active_ticks =
            self.nearest_active_ticks(self.active_side, self.top_active_level);
        ensure_here!(
            !nearest_active_ticks.is_empty() || self.top_active_level < NUM_FEE_LEVELS - 1,
            // Insufficient liquidity to complete the swap, and no tick crossing or level activation ahead
            ErrorKind::InsufficientLiquidity
        );

        if let Some((level, tick)) = nearest_active_ticks.first() {
            let next_active_tick_eff_sqrtprice_swap_side =
                tick.eff_sqrtprice(*level, self.active_side);
            if next_active_tick_eff_sqrtprice_swap_side <= new_eff_sqrtprice {
                new_eff_sqrtprice = next_active_tick_eff_sqrtprice_swap_side;
                limit_kind = StepLimit::TickCrossing;
            }
        }

        let init_eff_sqrtprice = self.eff_sqrtprice(self.active_side, self.top_active_level);

        let eff_sqrtprice_shift = new_eff_sqrtprice - init_eff_sqrtprice;

        let in_amount_change = eff_sqrtprice_shift * sum_gross_liquidities;

        self.pivot = find_pivot(self.pivot, new_eff_sqrtprice).map_err(|e| error_here!(e))?;

        let mut out_amount_change = AmountUFP::zero();
        for level in 0..=self.top_active_level {
            let mut new_eff_sqrtprices = if limit_kind == StepLimit::TickCrossing {
                let (tick_level, tick) = nearest_active_ticks[0];
                EffectiveSqrtPrice::from_tick(
                    &tick
                        .with_same_eff_price(tick_level, level, self.active_side)
                        .map_err(|e| match e {
                            ErrorKind::PriceTickOutOfBounds => ErrorKind::InsufficientLiquidity,
                            other => other,
                        })
                        .map_err(|e| error_here!(e))?,
                    level,
                )
            } else {
                EffectiveSqrtPrice::from_value(
                    new_eff_sqrtprice,
                    self.active_side,
                    level,
                    Some(self.pivot),
                )
                .map_err(|e| error_here!(e))?
            };

            match self.active_side {
                Side::Left => {
                    new_eff_sqrtprices.1 = new_eff_sqrtprices
                        .1
                        .min(self.eff_sqrtprice(self.active_side.opposite(), level));
                }
                Side::Right => {
                    new_eff_sqrtprices.0 = new_eff_sqrtprices
                        .0
                        .min(self.eff_sqrtprice(self.active_side.opposite(), level));
                }
            }

            let out_amount_change_this_level = self
                .update_prices_and_position_reserves(level, new_eff_sqrtprices)?
                [self.active_side.opposite()];
            ensure_here!(
                out_amount_change_this_level <= AmountSFP::zero(),
                ErrorKind::InternalLogicError
            );
            out_amount_change += out_amount_change_this_level.value;
        }

        out_amount_change = out_amount_change.min(
            AmountUFP::try_from(in_amount_change / init_eff_sqrtprice / new_eff_sqrtprice)
                .map_err(|e| error_here!(e))?,
        );

        self.accumulate_fees(eff_sqrtprice_shift, protocol_fee_fraction)?;

        if limit_kind == StepLimit::LevelActivation {
            self.top_active_level += 1;
        }

        if limit_kind == StepLimit::TickCrossing {
            self.tick_crossing(nearest_active_ticks, self.active_side);
        }

        Ok((in_amount_change, out_amount_change, limit_kind))
    }

    pub(crate) fn swap(
        &mut self,
        side: Side,
        exact_in_or_out: Exact,
        amount: Amount,
        protocol_fee_fraction: BasisPoints,
    ) -> Result<Amount> {
        match exact_in_or_out {
            Exact::In => self.swap_exact_in(side, amount, protocol_fee_fraction),
            Exact::Out => self.swap_exact_out(side, amount, protocol_fee_fraction),
        }
    }

    pub(crate) fn swap_exact_in(
        &mut self,
        side: Side,
        amount_in: Amount,
        protocol_fee_fraction: BasisPoints,
    ) -> Result<Amount> {
        Ok(self
            .swap_exact_in_impl(side, amount_in, protocol_fee_fraction, None)?
            .1)
    }

    #[allow(unused)]
    pub(crate) fn swap_to_price(
        &mut self,
        side: Side,
        max_amount_in: Amount,
        max_eff_sqrtprice: Float,
        protocol_fee_fraction: BasisPoints,
    ) -> Result<(Amount, Amount)> {
        if max_eff_sqrtprice <= self.eff_sqrtprice(side, 0) {
            return Ok((Amount::zero(), Amount::zero()));
        }
        self.swap_exact_in_impl(
            side,
            max_amount_in,
            protocol_fee_fraction,
            Some(max_eff_sqrtprice),
        )
    }

    pub(crate) fn swap_exact_in_impl(
        &mut self,
        side: Side,
        amount_in: Amount,
        protocol_fee_fraction: BasisPoints,
        eff_sqrtprice_limit: Option<Float>,
    ) -> Result<(Amount, Amount)> {
        ensure_here!(!amount_in.is_zero(), ErrorKind::InvalidParams);
        ensure_here!(self.is_spot_price_set(), ErrorKind::InsufficientLiquidity);

        if side != self.active_side {
            self.top_active_level = 0;
            self.active_side = side;
            self.pivot = self.pivot.opposite(0);
        }
        let init_eff_sqrtprice = self.eff_sqrtprice(side, 0);

        let amount_in_float = Float::from(amount_in);
        let mut actual_amount_in_float = Float::zero();
        let mut remaining_amount_in_float = amount_in_float;
        let mut amount_out_ufp = AmountUFP::zero();

        loop {
            let sum_gross_liquidities =
                Float::from(self.sum_gross_liquidities(self.top_active_level));

            let mut new_eff_sqrtprice = self.eval_required_new_eff_sqrtprice_exact_in(
                remaining_amount_in_float,
                sum_gross_liquidities,
            );

            if let Some(eff_sqrtprice_limit) = eff_sqrtprice_limit {
                new_eff_sqrtprice = new_eff_sqrtprice.min(eff_sqrtprice_limit);
            }

            let (in_amount_change, out_amount_change, limit_kind) = self.try_step_to_price(
                new_eff_sqrtprice,
                sum_gross_liquidities,
                protocol_fee_fraction,
            )?;

            remaining_amount_in_float -= in_amount_change;
            actual_amount_in_float += in_amount_change;
            amount_out_ufp += out_amount_change;

            if limit_kind == StepLimit::StepComplete {
                break;
            }
        }

        // Amount-in corresponding to the actual price shift may slightly exceed specified amount_in
        // due to numberic errors. The difference will be covered from the protocol fee.
        ensure_here!(
            remaining_amount_in_float >= -amount_in_float * SWAP_MAX_UNDERPAY,
            ErrorKind::InternalLogicError
        );
        let actual_amount_in = Amount::try_from(actual_amount_in_float)
            .map_err(|e| error_here!(e))?
            .min(amount_in);

        // implicit rounding-down
        let amount_out = Amount::try_from(amount_out_ufp)
            .map_err(|e| match e {
                fp::Error::Overflow => ErrorKind::SwapAmountTooLarge,
                other => ErrorKind::from(other),
            })
            .map_err(|e| error_here!(e))?;
        ensure_here!(amount_out > Amount::zero(), ErrorKind::SwapAmountTooSmall);

        ensure_here!(
            actual_amount_in_float / Float::from(amount_out)
                >= (Float::one() - SWAP_MAX_UNDERPAY) * init_eff_sqrtprice * init_eff_sqrtprice,
            ErrorKind::InternalLogicError
        );

        ensure_here!(
            Amount::MAX - self.total_reserves[side] >= amount_in,
            ErrorKind::DepositWouldOverflow
        );
        self.total_reserves[side] += amount_in;
        self.total_reserves[side.opposite()] -= amount_out;

        Ok((actual_amount_in, amount_out))
    }

    pub(crate) fn swap_exact_out(
        &mut self,
        side: Side,
        amount_out: Amount,
        protocol_fee_fraction: BasisPoints,
    ) -> Result<Amount> {
        ensure_here!(!amount_out.is_zero(), ErrorKind::InvalidParams);
        ensure_here!(self.is_spot_price_set(), ErrorKind::InsufficientLiquidity);

        if side != self.active_side {
            self.top_active_level = 0;
            self.active_side = side;
            self.pivot = self.pivot.opposite(0);
        }

        let init_eff_sqrtprice = self.eff_sqrtprice(side, 0);

        let mut amount_in_float = Float::zero();
        let mut amount_out_sfp = AmountSFP::from(amount_out);

        while amount_out_sfp > AmountSFP::zero() {
            let sum_gross_liquidities =
                Float::from(self.sum_gross_liquidities(self.top_active_level));

            let new_eff_sqrtprice = self.eval_required_new_eff_sqrtprice_exact_out(
                Float::from(amount_out_sfp),
                sum_gross_liquidities,
            )?;
            let (in_amount_change, out_amount_change, _limit_kind) = self.try_step_to_price(
                new_eff_sqrtprice,
                sum_gross_liquidities,
                protocol_fee_fraction,
            )?;

            amount_in_float += in_amount_change;
            amount_out_sfp += -AmountSFP::from(out_amount_change); // TODO IMPLEMENT `-=`
        }

        // round the amount-to-pay in favor of dex:
        amount_in_float = amount_in_float.ceil();

        let amount_in = Amount::try_from(amount_in_float)
            .map_err(|e: fp::Error| match e {
                fp::Error::Overflow => ErrorKind::SwapAmountTooLarge,
                other => ErrorKind::from(other),
            })
            .map_err(|e| error_here!(e))?;

        ensure_here!(amount_in > Amount::zero(), ErrorKind::SwapAmountTooSmall);
        ensure_here!(
            Amount::MAX - self.total_reserves[side] >= amount_in,
            ErrorKind::DepositWouldOverflow
        );
        ensure_here!(
            amount_in_float / Float::from(amount_out)
                >= (Float::one() - SWAP_MAX_UNDERPAY) * init_eff_sqrtprice * init_eff_sqrtprice,
            ErrorKind::InternalLogicError
        );

        self.total_reserves[side] += amount_in;
        self.total_reserves[side.opposite()] -= amount_out;

        Ok(amount_in)
    }

    pub(crate) fn update_prices_and_position_reserves(
        &mut self,
        fee_level: FeeLevel,
        eff_sqrtprice: EffectiveSqrtPrice,
    ) -> Result<(AmountSFP, AmountSFP)> {
        let old_eff_sqrtprices = (
            self.eff_sqrtprices[fee_level].0,
            self.eff_sqrtprices[fee_level].1,
        )
            .try_map_into::<AmountSFP, _>()
            .map_err(|e| error_here!(e))?;

        let new_eff_sqrtprices = (eff_sqrtprice.0, eff_sqrtprice.1)
            .try_map_into::<AmountSFP, _>()
            .map_err(|e| error_here!(e))?;

        let net_liquidity =
            AmountSFP::try_from(self.net_liquidities[fee_level]).map_err(|e| error_here!(e))?;

        let balance_change = (
            (new_eff_sqrtprices.0 - old_eff_sqrtprices.0) * net_liquidity,
            (new_eff_sqrtprices.1 - old_eff_sqrtprices.1) * net_liquidity,
        );

        self.position_reserves[fee_level] = (
            (AmountSFP::from(self.position_reserves[fee_level].0) + balance_change.0)
                .try_into_unsigned()
                .map_err(|e: fp::Error| error_here!(e))?,
            (AmountSFP::from(self.position_reserves[fee_level].1) + balance_change.1)
                .try_into_unsigned()
                .map_err(|e: fp::Error| error_here!(e))?,
        );
        self.eff_sqrtprices[fee_level] = eff_sqrtprice;

        Ok(balance_change)
    }

    pub(crate) fn tick_crossing(
        &mut self,
        crossed_ticks: Vec<(FeeLevel, Tick)>,
        swap_direction: Side,
    ) {
        for (level, tick) in crossed_ticks {
            let acc_lp_fees_per_fee_liquidity = self.acc_lp_fees_per_fee_liquidity(level);

            // Update liquidities
            self.tick_states[level].update(&tick, |TickState::V0(tick_state)| {
                let net_liquidity_change = match swap_direction {
                    Side::Left => tick_state.net_liquidity_change,
                    Side::Right => tick_state.net_liquidity_change.neg(),
                };

                if net_liquidity_change.non_negative {
                    self.net_liquidities[level] += net_liquidity_change.value;
                } else {
                    self.net_liquidities[level] -= net_liquidity_change.value;
                };

                tick_state.acc_lp_fees_per_fee_liquidity_outside.0 = acc_lp_fees_per_fee_liquidity
                    .0
                    - tick_state.acc_lp_fees_per_fee_liquidity_outside.0;
                tick_state.acc_lp_fees_per_fee_liquidity_outside.1 = acc_lp_fees_per_fee_liquidity
                    .1
                    - tick_state.acc_lp_fees_per_fee_liquidity_outside.1;

                Ok(())
            });

            // Update next active ticks:
            let next_active_tick = self.find_next_active_tick_on_level(tick, level, swap_direction);

            match swap_direction {
                Side::Left => {
                    self.next_active_ticks_right[level] = self.next_active_ticks_left[level];
                    self.next_active_ticks_left[level] = next_active_tick;
                }
                Side::Right => {
                    self.next_active_ticks_left[level] = self.next_active_ticks_right[level];
                    self.next_active_ticks_right[level] = next_active_tick;
                }
            };
        }
    }

    pub(crate) fn withdraw_protocol_fee(&mut self) -> Result<(Amount, Amount)> {
        let total_reserves = self.total_reserves.map_into::<AmountUFP>();
        let sum_position_reserves = self.sum_position_reserves();

        let payout_x = Amount::try_from(
            (total_reserves.0 - sum_position_reserves.0 - self.acc_lp_fee.0).floor(),
        )
        .map_err(|e| error_here!(e))?;
        let payout_y = Amount::try_from(
            (total_reserves.1 - sum_position_reserves.1 - self.acc_lp_fee.1).floor(),
        )
        .map_err(|e| error_here!(e))?;

        self.total_reserves.0 -= payout_x;
        self.total_reserves.1 -= payout_y;

        Ok((payout_x, payout_y))
    }

    pub fn get_ticks_liquidity_change(
        &self,
        fee_level: FeeLevel,
        side: Side,
    ) -> Vec<(Tick, Float)> {
        let mut ticks = self.tick_states[fee_level]
            .iter()
            .map(|(tick, tick_state)| {
                let TickState::V0(ref tick_state) = *tick_state;
                (*tick, Float::from(tick_state.net_liquidity_change))
            })
            .collect::<Vec<_>>();

        if side == Side::Right {
            ticks = ticks
                .into_iter()
                .rev()
                .map(|(tick, liq_change)| (tick.opposite(), -liq_change))
                .collect();
        }

        ticks
    }

    /// LP fee per net liquidity, accumulated from the very beginning of dex operation, in the given range.
    pub(crate) fn acc_range_lp_fees_per_fee_liquidity(
        &self,
        fee_level: FeeLevel,
        tick_bounds: (Tick, Tick),
    ) -> Result<(LPFeePerFeeLiquidity, LPFeePerFeeLiquidity)> {
        // `unwrap_or_default` is used to evaluate `acc_lp_fees_per_fee_liquidity_outside` for new position when some ticks are not yet initialized.
        let lower_tick_acc_lp_fees_per_fee_liquidity_outside = self.tick_states[fee_level]
            .inspect(&tick_bounds.0, |TickState::V0(tick_state)| {
                tick_state.acc_lp_fees_per_fee_liquidity_outside
            })
            .unwrap_or_default();

        let upper_tick_acc_lp_fees_per_fee_liquidity_outside = self.tick_states[fee_level]
            .inspect(&tick_bounds.1, |TickState::V0(tick_state)| {
                tick_state.acc_lp_fees_per_fee_liquidity_outside
            })
            .unwrap_or_default();

        let acc_range_lp_fees_per_fee_liquidity =
            match self.cmp_spot_price_to_position_range(fee_level, tick_bounds)? {
                Ordering::Equal => {
                    // global:        ////////.////////.////////
                    // lower_outside: ////////.        .
                    // upper_outside:         .        .////////
                    // position  = global - lower_outside - upper_outside
                    (
                        self.acc_lp_fee_per_fee_liquidity(Side::Left, fee_level)
                            - lower_tick_acc_lp_fees_per_fee_liquidity_outside.0
                            - upper_tick_acc_lp_fees_per_fee_liquidity_outside.0,
                        self.acc_lp_fee_per_fee_liquidity(Side::Right, fee_level)
                            - lower_tick_acc_lp_fees_per_fee_liquidity_outside.1
                            - upper_tick_acc_lp_fees_per_fee_liquidity_outside.1,
                    )
                }
                Ordering::Less => {
                    // global:        ////////.////////.////////
                    // lower_outside:         .////////.////////
                    // upper_outside:         .        .////////
                    // position  = lower_outside - upper_outside
                    (
                        lower_tick_acc_lp_fees_per_fee_liquidity_outside.0
                            - upper_tick_acc_lp_fees_per_fee_liquidity_outside.0,
                        lower_tick_acc_lp_fees_per_fee_liquidity_outside.1
                            - upper_tick_acc_lp_fees_per_fee_liquidity_outside.1,
                    )
                }
                Ordering::Greater => {
                    // global:        ////////.////////.////////
                    // lower_outside: ////////.        .
                    // upper_outside: ////////.////////.
                    // position  = upper_outside - lower_outside
                    (
                        upper_tick_acc_lp_fees_per_fee_liquidity_outside.0
                            - lower_tick_acc_lp_fees_per_fee_liquidity_outside.0,
                        upper_tick_acc_lp_fees_per_fee_liquidity_outside.1
                            - lower_tick_acc_lp_fees_per_fee_liquidity_outside.1,
                    )
                }
            };
        Ok(acc_range_lp_fees_per_fee_liquidity)
    }

    pub(crate) fn position_reward_ufp(
        &self,
        pos: &PositionV0<T>,
        since_creation: bool,
    ) -> Result<(AmountUFP, AmountUFP)> {
        let pos_acc_lp_fees_per_fee_liquidity =
            self.acc_range_lp_fees_per_fee_liquidity(pos.fee_level, pos.tick_bounds)?;

        let initial_acc_lp_fees_per_fee_liquidity = if since_creation {
            pos.init_acc_lp_fees_per_fee_liquidity
        } else {
            pos.unwithdrawn_acc_lp_fees_per_fee_liquidity
        };

        let acc_lp_fees_per_fee_liquidity_diff = (
            pos_acc_lp_fees_per_fee_liquidity.0 - initial_acc_lp_fees_per_fee_liquidity.0,
            pos_acc_lp_fees_per_fee_liquidity.1 - initial_acc_lp_fees_per_fee_liquidity.1,
        );

        ensure_here!(
            acc_lp_fees_per_fee_liquidity_diff.0 >= LPFeePerFeeLiquidity::zero(),
            ErrorKind::InternalLogicError
        );
        ensure_here!(
            acc_lp_fees_per_fee_liquidity_diff.1 >= LPFeePerFeeLiquidity::zero(),
            ErrorKind::InternalLogicError
        );

        let fee_liquidity = AmountUFP::try_from(pos.fee_liquidity()).map_err(|e| error_here!(e))?;
        let position_reward_ufp =
            acc_lp_fees_per_fee_liquidity_diff.map(|d| fee_liquidity * AmountUFP::from(d.value));

        Ok(position_reward_ufp)
    }

    pub(crate) fn position_reward(
        &self,
        pos: &PositionV0<T>,
        since_creation: bool,
    ) -> Result<(Amount, Amount)> {
        self.position_reward_ufp(pos, since_creation)?
            .try_map_into::<Amount, _>()
            .map_err(|e| error_here!(e))
    }
}

/// Evaluate initial effective sqrtprice
fn eval_initial_eff_sqrtprice(
    amount_left: Float,
    amount_right: Float,
    tick_low: Tick,
    tick_high: Tick,
    fee_level: FeeLevel,
) -> Result<(Float, Side)> {
    if amount_left > Float::zero() && amount_right > Float::zero() {
        // The position consists of both left and right tokens. The price is determined
        // from the requirement that net_liquidity evaluated from left and right
        // token amounts is the same:
        // ```
        //     amount_left / (eff_sqrtprice_left - tick_low.eff_sqrtprice_left) =
        //         = amount_right / (eff_sqrtprice_right - tick_high.eff_sqrtprice_right)
        // ```
        // This leads to a quadratic equation, which can be solved either w.r.t. eff_sqrtprice_left
        // or w.r.t. eff_sqrtprice_right. Due to the limited numberic precision, one solution is more
        // accurate than the other. For the solution w.r.t. eff_sqrtprice_left the terms
        // of the quadratic equation are:
        // ```
        //   a = 1
        //   b = amount_left / amount_right * tick_high.eff_sqrtprice_right
        //          - tick_low.eff_sqrtprice_left
        //   c = - (amount_left / amount_right) / (1 - fee_rate)
        // ```
        // For the solution w.r.t eff_sqrtprice_right the terms are:
        // ```
        //   a = 1
        //   b = amount_right / amount_left * tick_low.eff_sqrtprice_left
        //          - tick_high.eff_sqrtprice_right
        //   c = - (amount_right / amount_left) / (1 - fee_rate)
        // ```
        // In both cases the only positive solution is:
        // ```
        //     eff_sqrtprice_(left|right) = [sqrt(b*b - 4*a*c) - b] / 2a
        // ```
        // The solution is more accurate when b term is negative. So we prefer the solution
        // w.r.t. eff_sqrtprice_left if amount_left * eff_sqrtprice_low_right <= amount_right * eff_sqrtprice_low_left
        // and the solution w.r.t. eff_sqrtprice_right otherwise.

        let eff_sqrtprice_low_left = tick_low.eff_sqrtprice(fee_level, Side::Left);
        let eff_sqrtprice_low_right = tick_high.eff_sqrtprice(fee_level, Side::Right);

        let is_eval_left =
            amount_left * eff_sqrtprice_low_right <= amount_right * eff_sqrtprice_low_left;

        let amount_ratio = if is_eval_left {
            amount_left / amount_right
        } else {
            amount_right / amount_left
        };
        let minus_b_term = if is_eval_left {
            AmountUFP::try_from(eff_sqrtprice_low_left)
                .map_err(|_| error_here!(ErrorKind::InternalLogicError))?
                - AmountUFP::try_from(amount_ratio * eff_sqrtprice_low_right)
                    .map_err(|_| error_here!(ErrorKind::InternalLogicError))?
        } else {
            AmountUFP::try_from(eff_sqrtprice_low_right)
                .map_err(|_| error_here!(ErrorKind::InternalLogicError))?
                - AmountUFP::try_from(amount_ratio * eff_sqrtprice_low_left)
                    .map_err(|_| error_here!(ErrorKind::InternalLogicError))?
        };
        let one_over_one_minus_fee_rate = if is_eval_left {
            eff_sqrtprice_low_left * tick_low.eff_sqrtprice(fee_level, Side::Right)
        } else {
            eff_sqrtprice_low_right * tick_high.eff_sqrtprice(fee_level, Side::Left)
        };

        // The conversion will never fail on VEAX because AmountUFP has many more bits than Amount,
        // whereas amount_ratio can not exceed Amount::MAX, and the factor 4*one_over_one_minus_fee_rate is O(4),
        // On CDEX and DX25 the conversion may fail when amount_ratio >~ Amount::MAX/4 which
        // requires at least O(Amout::MAX/4) tokens in the position.
        let minus_four_a_c =
            AmountUFP::try_from(Float::from(4) * amount_ratio * one_over_one_minus_fee_rate)
                .map_err(|_| error_here!(ErrorKind::LiquidityTooBig))?;

        let discriminant = minus_b_term * minus_b_term + minus_four_a_c;
        let eff_sqrtprice = next_up(Float::from(discriminant.integer_sqrt() + minus_b_term))
            * Float::from(2).recip();

        if is_eval_left {
            ensure_here!(
                eff_sqrtprice >= tick_low.eff_sqrtprice(fee_level, Side::Left),
                ErrorKind::InternalLogicError
            );
            ensure_here!(
                eff_sqrtprice <= tick_high.eff_sqrtprice(fee_level, Side::Left),
                ErrorKind::InternalLogicError
            );
            Ok((eff_sqrtprice, Side::Left))
        } else {
            ensure_here!(
                eff_sqrtprice >= tick_high.eff_sqrtprice(fee_level, Side::Right),
                ErrorKind::InternalLogicError
            );
            ensure_here!(
                eff_sqrtprice <= tick_low.eff_sqrtprice(fee_level, Side::Right),
                ErrorKind::InternalLogicError
            );

            Ok((eff_sqrtprice, Side::Right))
        }
    } else if amount_left > Float::zero() {
        // The position consists of left token only.
        // We set spot price to the upper bound of position range.

        // Protection against occasional trader's error: it is unlikely
        // that trader wants to create a position setting spot price to TICK::MAX
        // Therefore we return an error. If the trader intends to create a postion
        // at price close to Tick::MAX price, he may explicitly specify e.g. Tick::MAX-1
        // as the upper position range bound.
        ensure_here!(tick_high < Tick::MAX, ErrorKind::WrongRatio);

        Ok((tick_high.eff_sqrtprice(fee_level, Side::Right), Side::Right))
    } else if amount_right > Float::zero() {
        // The position consists of right token only.
        // We set spot price to the lower bound of position range.

        // Protection against occasional trader's error: it is unlikely
        // that trader wants to create a position setting spot price to TICK::MIN
        // Therefore we return an error. If the trader intends to create a postion
        // at price close to Tick::MIN price, he may explicitly specify e.g. Tick::MIN+1
        // as the lower position range bound.
        ensure_here!(tick_low > Tick::MIN, ErrorKind::WrongRatio);

        Ok((tick_low.eff_sqrtprice(fee_level, Side::Left), Side::Left))
    } else {
        // Both amounts are zero
        Err(error_here!(ErrorKind::InvalidParams))
    }
}

fn find_pivot(init_pivot: EffTick, eff_sqrtprice: Float) -> Result<EffTick, ErrorKind> {
    /// Min and max "distance" between `pivot.spot_sqrtprice`() and `eff_sqrtprice`, expressed as factor.
    /// This "distance" must not exceed 1 tick in order to achive sufficiently accurate price inversion.
    /// Currently chosen values are +/- 0.625 ticks.
    /// ```
    /// // TODO: implement for cdex and dx25
    /// #[cfg(feature = "near")]
    /// {
    /// use crate::veax_dex::dex::{Float, tick::Tick};
    /// let base_pow_0625 = Tick::BASE.sqrt() * (Tick::BASE.sqrt().sqrt().sqrt());
    /// assert_eq!(base_pow_0625.recip().to_bits(), 0x3FEF_FFBE_77E2_8A1D);
    /// assert_eq!(base_pow_0625.to_bits(), 0x3FF0_0020_C451_D518);
    /// }
    /// ```
    const DIST_MIN: Float = Float::from_bits(0x3FEF_FFBE_77E2_8A1D);
    const DIST_MAX: Float = Float::from_bits(0x3FF0_0020_C451_D518);

    /// If `distance_factor` (see below) is within this range, we calculate `log(distance_factor)`
    /// approximately, otherwise we use `PRECALCULATED_TICKS` LUT.
    /// ```
    /// // TODO: implement for cdex and dx25
    /// #[cfg(feature = "near")]
    /// {
    /// use crate::veax_dex::dex::{Float, tick::PRECALCULATED_TICKS};
    /// let MIN_APPROXIMATE_LOG = Float::from_bits(PRECALCULATED_TICKS[12]).recip();
    /// assert_eq!(MIN_APPROXIMATE_LOG, Float::from_bits(0x3FEA_12FE_77BF_A405));
    /// }
    /// ```
    const MAX_APPROXIMATE_LOG_INDEX: u32 = 12;
    const MAX_APPROXIMATE_LOG: Float =
        Float::from_bits(PRECALCULATED_TICKS[MAX_APPROXIMATE_LOG_INDEX as usize]);
    const MIN_APPROXIMATE_LOG: Float = Float::from_bits(0x3FEA_12FE_77BF_A405);

    let mut pivot = init_pivot;
    loop {
        // "distance" between eff_sqrtprice and pivot spot sqrtprice, expressed as factor.
        // `log(distance_factor)` is the actual distance between eff_sqrtprice
        // and pivot spot sqrtprice in units of log base.
        let distance_factor = eff_sqrtprice / pivot.eff_sqrtprice();

        if DIST_MIN < distance_factor && distance_factor < DIST_MAX {
            break;
        }

        let step_ticks = if distance_factor > MAX_APPROXIMATE_LOG {
            // log(distance_factor) is a large positive number: step by one of the PRECALCULATED_TICKS
            let step_ticks_log2: u32 = PRECALCULATED_TICKS
                .iter()
                .rposition(|&sqrtprice_bits| distance_factor >= Float::from_bits(sqrtprice_bits))
                .unwrap() // will always succeed because distance_factor > MAX_APPROXIMATE_LOG so the index can not be smaller than MAX_APPROXIMATE_LOG_INDEX
                .try_into()
                .unwrap(); // will always succeed as the index is limited to PRECALCULATED_TICKS.len()

            2i32.pow(step_ticks_log2)
        } else if distance_factor < MIN_APPROXIMATE_LOG {
            // log(distance_factor) is a large negative number: step by one of the PRECALCULATED_TICKS
            let step_ticks_log2: u32 = PRECALCULATED_TICKS
                .iter()
                .rposition(|&sqrtprice_bits| {
                    distance_factor.recip() >= Float::from_bits(sqrtprice_bits)
                })
                .unwrap() // will always succeed because distance_factor < MIN_APPROXIMATE_LOG, so distance_factor.recip() > MAX_APPROXIMATE_LOG, so the index can not be smaller than MAX_APPROXIMATE_LOG_INDEX
                .try_into()
                .unwrap(); // will always succeed as the index is limited to PRECALCULATED_TICKS.len()

            -(2i32.pow(step_ticks_log2))
        } else {
            // distance factor is small: use approximation for small x: (1+x)^n ~= 1+n*x
            let step_ticks_float =
                ((distance_factor - Float::one()) / (Tick::BASE - Float::one())).round();
            // Unwrap will always succeed because distance_factor can not exceed +/- 2^MAX_APPROXIMATE_LOG_INDEX (== 4096) ticks
            // and due to the approximation, step_ticks_float can only be slightly larger than that.
            let step_ticks: i32 = step_ticks_float.try_into().unwrap();

            // We limit the step to +/-2^MAX_APPROXIMATE_LOG_INDEX (== 4096) ticks
            // in order to make sure that pivot stays within valid tick range.
            step_ticks
                .clamp(
                    -(2i32.pow(MAX_APPROXIMATE_LOG_INDEX)),
                    2i32.pow(MAX_APPROXIMATE_LOG_INDEX),
                )
                .clamp(MIN_EFF_TICK - pivot.index(), MAX_EFF_TICK - pivot.index())
        };

        if step_ticks == 0 {
            return Err(ErrorKind::InternalLogicError);
        }

        pivot = pivot.shifted(step_ticks)?;
    }

    Ok(pivot)
}

pub fn next_down(a: Float) -> Float {
    // We must use strictly integer arithmetic to prevent denormals from
    // flushing to zero after an arithmetic operation on some platforms.
    const NEG_TINY_BITS: u64 = 0x8000_0000_0000_0001; // Smallest (in magnitude) negative f64.
    const CLEAR_SIGN_MASK: u64 = 0x7fff_ffff_ffff_ffff;
    const NEG_INFINITY_BITS: u64 = 0xfff0_0000_0000_0000;

    let bits = a.to_bits();
    if a.is_nan() || bits == NEG_INFINITY_BITS {
        return a;
    }

    let abs = bits & CLEAR_SIGN_MASK;
    let next_bits = if abs == 0 {
        NEG_TINY_BITS
    } else if bits == abs {
        bits - 1
    } else {
        bits + 1
    };
    Float::from_bits(next_bits)
}

pub fn next_up(a: Float) -> Float {
    // We must use strictly integer arithmetic to prevent denormals from
    // flushing to zero after an arithmetic operation on some platforms.
    const TINY_BITS: u64 = 0x1; // Smallest positive f64.
    const CLEAR_SIGN_MASK: u64 = 0x7fff_ffff_ffff_ffff;
    const INFINITY_BITS: u64 = 0x7ff0_0000_0000_0000;

    let bits = a.to_bits();
    if a.is_nan() || bits == INFINITY_BITS {
        return a;
    }

    let abs = bits & CLEAR_SIGN_MASK;
    let next_bits = if abs == 0 {
        TINY_BITS
    } else if bits == abs {
        bits + 1
    } else {
        bits - 1
    };
    Float::from_bits(next_bits)
}
