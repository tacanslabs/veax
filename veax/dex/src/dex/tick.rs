use bitvec::macros::internal::funty::Fundamental;
use bitvec::prelude::*;
use itertools::Itertools;

use super::Float;
use crate::chain::{MAX_TICK, MIN_TICK};

#[cfg(feature = "near")]
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
#[cfg(feature = "near")]
use serde::{Deserialize, Serialize};

use crate::dex::dex_impl::fee_rate_ticks;
use crate::dex::{ErrorKind, FeeLevel, Side};
use crate::{MAX_EFF_TICK, MIN_EFF_TICK};

/// generated with test:
///
///   ```bash
///   cd core/veax/dex
///   cargo test test_precalculate_ticks_bit_repr -- --nocapture
///   ```
///
#[allow(clippy::unreadable_literal)]
pub const PRECALCULATED_TICKS: [u64; 21] = [
    4607182643974369558,
    4607182869159980145,
    4607183319564978878,
    4607184220510102349,
    4607186022940979433,
    4607189629966263589,
    4607196852679033204,
    4607211332818125533,
    4607240432470062669,
    4607299193450302128,
    4607418995971640537,
    4607668000704051496,
    4608205938457857923,
    4609462070376259803,
    4612290832146940624,
    4617480469329378893,
    4628148512120721768,
    4649381992504848318,
    4692198734602598674,
    4777248888797670312,
    4947442543280771895,
];

#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Copy, Clone, Debug, Default, Hash, Ord, PartialOrd, Eq, PartialEq)]
#[cfg_attr(
    feature = "near",
    derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize)
)]
#[repr(transparent)]
/// A point on the price scale which corresponds to a specific _spot_ price
pub struct Tick(i32);

#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Copy, Clone, Debug, Default, Hash, Ord, PartialOrd, Eq, PartialEq)]
#[cfg_attr(
    feature = "near",
    derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize)
)]
#[repr(transparent)]
/// A point on the price scale which corresponds to a specific _effective_ price
pub struct EffTick(i32);

impl Tick {
    pub const BASE: Float = Float::from_bits(PRECALCULATED_TICKS[0]);
    pub const MIN: Self = Self(MIN_TICK);
    pub const MAX: Self = Self(MAX_TICK);

    pub fn new(value: i32) -> Result<Self, ErrorKind> {
        if Self::is_valid(value) {
            Ok(Self(value))
        } else {
            Err(ErrorKind::PriceTickOutOfBounds)
        }
    }

    /// # Safety
    ///
    /// This function should be called only with values for which `Tick::is_valid` return true
    pub const unsafe fn new_unchecked(value: i32) -> Self {
        Self(value)
    }

    pub const fn is_valid(value: i32) -> bool {
        MIN_TICK <= value && value <= MAX_TICK
    }

    pub const fn index(&self) -> i32 {
        self.0
    }

    pub const fn to_opt_index(&self) -> Option<i32> {
        if MIN_TICK < self.index() && self.index() < MAX_TICK {
            Some(self.index())
        } else {
            None
        }
    }

    /// For a given `swap_direction`, returns tick with same effective price on `other_level`
    /// as this tick has on `this_level`.
    pub(crate) fn with_same_eff_price(
        self,
        this_level: FeeLevel,
        other_level: FeeLevel,
        swap_direction: Side,
    ) -> Result<Self, ErrorKind> {
        EffTick::from_tick(self, this_level, swap_direction).to_tick(other_level, swap_direction)
    }

    /// Spot sqrtprice corresponding to a tick, for a left-side (i.e. forward direction) swap.
    pub fn spot_sqrtprice(&self) -> Float {
        self.index()
            .abs()
            .as_u32()
            .view_bits::<Lsb0>() // least significant bit has position 0 as opposite to Msb0
            .iter_ones()
            // safe because tick values are validated when tick created
            // so bit index cannot exceed range of precalculated ticks
            .map(|index| unsafe { *PRECALCULATED_TICKS.get_unchecked(index) })
            .map(Float::from_bits)
            .product1()
            .map_or(Float::one(), |scale_by| {
                if self.index().is_positive() {
                    scale_by
                } else {
                    scale_by.recip()
                }
            })
    }

    /// Effective sqrtprice corresponding to a tick, for a given fee level and swap direciton.
    pub fn eff_sqrtprice(&self, fee_level: FeeLevel, side: Side) -> Float {
        EffTick::from_tick(*self, fee_level, side).eff_sqrtprice()
    }

    /// Tick corresponding to the opposite spot sqrtprice
    pub fn opposite(&self) -> Self {
        // unwrap will succeed as long as tick itself is valid and the range of valid ticks is symmetric
        Tick::new(-self.index()).unwrap()
    }

    /// Convenience function allowing to take the opposite tick conditionally
    pub fn opposite_if(&self, is_opposite: bool) -> Self {
        if is_opposite {
            self.opposite()
        } else {
            *self
        }
    }

    pub fn unwrap_range(as_options: (Option<i32>, Option<i32>)) -> Result<(Tick, Tick), ErrorKind> {
        Ok((
            match as_options.0 {
                Some(tick_low) => Tick::new(tick_low)?,
                None => Tick::MIN,
            },
            match as_options.1 {
                Some(tick_high) => Tick::new(tick_high)?,
                None => Tick::MAX,
            },
        ))
    }

    pub fn wrap_range(as_ticks: (Tick, Tick)) -> (Option<i32>, Option<i32>) {
        (
            if as_ticks.0 <= Tick::MIN {
                None
            } else {
                Some(as_ticks.0.index())
            },
            if as_ticks.1 >= Tick::MAX {
                None
            } else {
                Some(as_ticks.1.index())
            },
        )
    }
}

impl EffTick {
    pub const fn is_valid(index: i32) -> bool {
        MIN_EFF_TICK <= index && index <= MAX_EFF_TICK
    }

    pub fn new(index: i32) -> Result<Self, ErrorKind> {
        if Self::is_valid(index) {
            Ok(Self(index))
        } else {
            Err(ErrorKind::PriceTickOutOfBounds)
        }
    }

    pub const fn index(&self) -> i32 {
        self.0
    }

    pub fn from_tick(tick: Tick, fee_level: FeeLevel, side: Side) -> EffTick {
        let eff_tick_index = match side {
            Side::Left => tick.index() + i32::from(fee_rate_ticks(fee_level)),
            Side::Right => -tick.index() + i32::from(fee_rate_ticks(fee_level)),
        };
        // Unwrap will succeed as long as `tick` is valid.
        // See `test_eff_tick_from_tick_and_opposite_succeeds`
        EffTick::new(eff_tick_index).unwrap()
    }

    pub fn to_tick(&self, fee_level: FeeLevel, side: Side) -> Result<Tick, ErrorKind> {
        let tick_index = match side {
            Side::Left => self.index() - i32::from(fee_rate_ticks(fee_level)),
            Side::Right => -self.index() + i32::from(fee_rate_ticks(fee_level)),
        };
        Tick::new(tick_index)
    }

    pub fn eff_sqrtprice(&self) -> Float {
        // The constructed tick is not strictly valid, but as long as `self.index()` is within
        // MIN_EFF_TICK..=MAX_EFF_TICK range, the spot price is still calculateable.
        // See `test_eff_sqrtprice_for_extreme_eff_ticks_succeed`.
        Tick(self.index()).spot_sqrtprice()
    }

    pub fn opposite(&self, fee_level: FeeLevel) -> Self {
        let opposite_eff_tick_index = -self.index() + 2_i32.pow(u32::from(fee_level) + 1);
        debug_assert!(Self::is_valid(opposite_eff_tick_index));
        // unwrap will succeed as long as the effective tick itself is valid
        // See `test_eff_tick_from_tick_and_opposite_succeeds`
        EffTick::new(opposite_eff_tick_index).unwrap()
    }

    pub fn shifted(&self, step: i32) -> Result<Self, ErrorKind> {
        EffTick::new(self.index() + step)
    }
}
