use super::TokenId;
use crate::dex::latest::RawFeeLevelsArray;
use crate::dex::tick::Tick;
use crate::dex::{self, BasisPoints, PairExt};
use crate::error_here;
use crate::fp::U128X128;
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::AccountId;

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct AmountInOut {
    pub amount_in: U128,
    pub amount_out: U128,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(not(target_arch = "wasm32"), derive(Deserialize, Debug))]
pub struct ContractMetadata {
    /// Account that is allowed to change DEX configuration and withdraw protocol fee.
    /// Normally it is the account with the governance smart contract.
    pub owner: AccountId,

    /// Number of existing pools.
    pub pool_count: u64,

    /// Fraction of fee which goes to DEX.
    pub protocol_fee_fraction: BasisPoints,

    /// Fee rate scaled up by fee_divisor.
    pub fee_rates: dex::latest::RawFeeLevelsArray<BasisPoints>,

    /// Scale factor for the fee rates and protocol fee fraction.
    pub fee_divisor: BasisPoints,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct RefStorageState {
    pub deposit: U128,
    pub usage: U128,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct PositionInfo {
    pub tokens_ids: (TokenId, TokenId),
    pub balance: (U128, U128),
    pub range_ticks: (Option<i32>, Option<i32>),
    pub reward_since_last_withdraw: (U128, U128),
    pub reward_since_creation: (U128, U128),
    #[serde(with = "super::utils::serde_as_str")]
    pub init_sqrt_price: f64,
}

impl From<dex::PositionInfo> for PositionInfo {
    fn from(info: dex::PositionInfo) -> Self {
        Self {
            tokens_ids: info.tokens_ids,
            balance: info.balance.map_into(),
            // FIXME: Yeah, looks extremely ugly.
            // There are plans to replace all this array-tuple-pair-whatever mess
            // with a single universal pair type.
            // This temporary workaround exists only to not provoke unnecessary changes ATM
            range_ticks: Tick::wrap_range(info.range_ticks),
            reward_since_last_withdraw: info.reward_since_last_withdraw.map_into(),
            reward_since_creation: info.reward_since_creation.map_into(),
            init_sqrt_price: info.init_sqrtprice.into(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
#[serde(crate = "near_sdk::serde")]
pub struct PoolInfo {
    /// Total amounts of tokens in the pool: sum of all positions and collected fees (LP and protocol).
    pub total_reserves: (U128, U128),

    /// Total amount of tokens locked in the pool (in positions)
    pub position_reserves: (U128, U128),

    /// Square root of the spot price on each of the fee levels, scaled by 2**64.
    /// The value is approximate, as interlally a different representation is used.
    /// If level is empty hence the price is undefined, the returned value is zero.
    pub sqrt_spot_prices: dex::latest::RawFeeLevelsArray<U128>,

    /// Liquidity on each of the fee levels.
    /// The value is approximate, as interlally a different representation is used.
    pub liquidities: dex::latest::RawFeeLevelsArray<U128>,

    /// Fee rate scaled up by fee_divisor.
    pub fee_rates: dex::latest::RawFeeLevelsArray<BasisPoints>, // TODO: consider removing, as it is global to DEX

    /// Scale factor for the fee levels.
    pub fee_divisor: BasisPoints,
}

impl PoolInfo {
    pub fn spot_price(&self, fee_level: usize) -> Option<f64> {
        let spot_sqrtprice_u128 = self.sqrt_spot_prices[fee_level].0;
        if spot_sqrtprice_u128 > 0 {
            #[allow(clippy::cast_precision_loss)]
            let spot_sqrtprice = (spot_sqrtprice_u128 as f64) / ((1u128 << 64) as f64);
            Some(spot_sqrtprice * spot_sqrtprice)
        } else {
            None
        }
    }
}

impl TryFrom<dex::PoolInfo> for PoolInfo {
    type Error = dex::Error;

    fn try_from(info: dex::PoolInfo) -> Result<Self, Self::Error> {
        let mut sqrt_spot_prices: RawFeeLevelsArray<U128> = std::array::from_fn(|_| U128::from(0));
        #[allow(clippy::cast_precision_loss)]
        for (in_price, out_price) in info
            .spot_sqrtprices
            .into_iter()
            .zip(sqrt_spot_prices.iter_mut())
        {
            *out_price = U128::from(
                U128X128::try_from(in_price * ((1u128 << 64) as f64).into())
                    .map_err(|e| error_here!(e))?
                    .upper_part(),
            );
        }
        Ok(Self {
            total_reserves: (info.total_reserves.0.into(), info.total_reserves.1.into()),
            position_reserves: (
                info.position_reserves.0.into(),
                info.position_reserves.1.into(),
            ),
            sqrt_spot_prices,
            liquidities: info
                .liquidities
                .map(|liquidity| U128::from(u128::try_from(liquidity).unwrap())),
            fee_rates: info.fee_rates,
            fee_divisor: info.fee_divisor,
        })
    }
}
