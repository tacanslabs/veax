use crate::chain::NetLiquiditySFP;
use crate::dex::{TickStateV0, Types};
use crate::LPFeePerFeeLiquidity;
use num_traits::Zero;
use std::marker::PhantomData;

impl<T: Types> Default for TickStateV0<T> {
    fn default() -> TickStateV0<T> {
        TickStateV0 {
            net_liquidity_change: NetLiquiditySFP::zero(),
            reference_counter: u32::zero(),
            acc_lp_fees_per_fee_liquidity_outside: (
                LPFeePerFeeLiquidity::zero(),
                LPFeePerFeeLiquidity::zero(),
            ),
            phantom_t: PhantomData,
        }
    }
}
