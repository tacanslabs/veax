use itertools::multizip;
#[allow(unused)]
use num_traits::Zero;

use crate::dex::v0::{fee_liquidity_from_net_liquidity, gross_liquidity_from_net_liquidity};
use crate::dex::{Error, FeeLevel, PositionV0, Side, Tick, Types};
use crate::{
    error_here, fp, AmountUFP, FeeLiquidityUFP, Float, GrossLiquidityUFP, NetLiquidityUFP,
};

impl<T: Types> PositionV0<T> {
    pub fn net_liquidity(&self) -> NetLiquidityUFP {
        self.net_liquidity
    }

    pub fn gross_liquidity(&self) -> GrossLiquidityUFP {
        gross_liquidity_from_net_liquidity(self.net_liquidity, self.fee_level)
    }

    pub fn fee_liquidity(&self) -> FeeLiquidityUFP {
        fee_liquidity_from_net_liquidity(self.net_liquidity, self.fee_level)
    }

    pub fn eval_position_balance_ufp(
        &self,
        eff_sqrtprice_left: Float,
        eff_sqrtprice_right: Float,
    ) -> Result<(AmountUFP, AmountUFP), Error> {
        eval_position_balance_ufp(
            self.net_liquidity,
            self.tick_bounds.0,
            self.tick_bounds.1,
            eff_sqrtprice_left,
            eff_sqrtprice_right,
            self.fee_level,
        )
    }
}

pub fn eval_position_balance_ufp(
    net_liquidity: NetLiquidityUFP,
    tick_low: Tick,
    tick_high: Tick,
    eff_sqrtprice_left: Float,
    eff_sqrtprice_right: Float,
    fee_level: FeeLevel,
) -> Result<(AmountUFP, AmountUFP), Error> {
    let lower_bounds = [
        tick_low.eff_sqrtprice(fee_level, Side::Left),
        tick_high.eff_sqrtprice(fee_level, Side::Right),
    ];
    let upper_bounds = [
        tick_high.eff_sqrtprice(fee_level, Side::Left),
        tick_low.eff_sqrtprice(fee_level, Side::Right),
    ];

    let eff_sqrtprices = [eff_sqrtprice_left, eff_sqrtprice_right];

    let mut balances_ufp = [AmountUFP::zero(), AmountUFP::zero()];

    for (eff_sqrtprice, lower_bound, upper_bound, balance_ufp) in multizip((
        eff_sqrtprices,
        lower_bounds,
        upper_bounds,
        &mut balances_ufp,
    )) {
        if eff_sqrtprice <= lower_bound {
            *balance_ufp = AmountUFP::zero();
        } else if eff_sqrtprice < upper_bound {
            let lower_bound =
                AmountUFP::try_from(lower_bound).map_err(|e: fp::Error| error_here!(e))?;
            let eff_sqrtprice =
                AmountUFP::try_from(eff_sqrtprice).map_err(|e: fp::Error| error_here!(e))?;
            let net_liquidity = AmountUFP::try_from(net_liquidity).map_err(|e| error_here!(e))?;
            *balance_ufp = net_liquidity * (eff_sqrtprice - lower_bound);
        } else {
            let lower_bound =
                AmountUFP::try_from(lower_bound).map_err(|e: fp::Error| error_here!(e))?;
            let upper_bound =
                AmountUFP::try_from(upper_bound).map_err(|e: fp::Error| error_here!(e))?;
            let net_liquidity = AmountUFP::try_from(net_liquidity).map_err(|e| error_here!(e))?;
            *balance_ufp = net_liquidity * (upper_bound - lower_bound);
        }
    }

    Ok((balances_ufp[0], balances_ufp[1]))
}
