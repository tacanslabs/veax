use std::fmt::{format, Arguments};

use crate::chain::log::log_str;
use crate::chain::{AccountId, Amount, TokenId};
use crate::dex;
use crate::dex::latest::RawFeeLevelsArray;
use crate::dex::PoolUpdateReason;
use near_sdk::json_types::{U128, U64};
use serde::Serialize;

pub(super) struct Logger;

#[derive(Serialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
struct NearEventWrapper<'a> {
    pub standard: &'static str,
    pub version: &'static str,

    #[serde(flatten)]
    pub event: Event<'a>,
}

fn emit<'a>(event: impl Into<Event<'a>>) {
    let event = NearEventWrapper {
        standard: "veax",
        version: "1.0.0",
        event: event.into(),
    };
    let Ok(json_string) = serde_json::to_string(&event) else { unreachable!() };
    log_str(&format!("EVENT_JSON:{json_string}"));
}

pub fn log_storage_balance_event(user: &AccountId, available: &Amount, total: &Amount) {
    emit(Event::StorageBalance {
        user,
        available: (*available).into(),
        total: (*total).into(),
    });
}

impl dex::Logger for Logger {
    fn log(&mut self, args: Arguments<'_>) {
        log_str(&format(args));
    }

    fn log_deposit_event(
        &mut self,
        user: &AccountId,
        token: &TokenId,
        amount: &Amount,
        balance: &Amount,
    ) {
        emit(Event::Deposit {
            user,
            token_id: token,
            amount: (*amount).into(),
            balance: (*balance).into(),
        });
    }

    fn log_withdraw_event(
        &mut self,
        user: &AccountId,
        token: &TokenId,
        amount: &Amount,
        balance: &Amount,
    ) {
        emit(Event::Withdraw {
            user,
            token_id: token,
            amount: (*amount).into(),
            balance: (*balance).into(),
        });
    }

    fn log_open_position_event(
        &mut self,
        user: &AccountId,
        pool: (&TokenId, &TokenId),
        amounts: (&Amount, &Amount),
        fee_rate: dex::BasisPoints,
        position_id: dex::PositionId,
    ) {
        emit(Event::OpenPosition {
            user,
            pool,
            amounts: ((*amounts.0).into(), (*amounts.1).into()),
            fee_rate: u64::from(fee_rate).into(),
            position_id: position_id.into(),
        });
    }

    fn log_harvest_fee_event(
        &mut self,
        position_id: dex::PositionId,
        fee_amounts: (Amount, Amount),
    ) {
        emit(Event::HarvestFee {
            position_id: position_id.into(),
            amounts: (fee_amounts.0.into(), fee_amounts.1.into()),
        });
    }

    fn log_close_position_event(
        &mut self,
        position_id: dex::PositionId,
        amounts: (Amount, Amount),
    ) {
        emit(Event::ClosePosition {
            position_id: position_id.into(),
            amounts: (amounts.0.into(), amounts.1.into()),
        });
    }

    fn log_swap_event(
        &mut self,
        user: &AccountId,
        tokens: (&TokenId, &TokenId),
        amounts: (&Amount, &Amount),
        fees: &[(&TokenId, &dex::BasisPoints)],
    ) {
        let fees = fees
            .iter()
            .copied()
            .map(|(token, fee)| (token, (u128::from(*fee)).into()))
            .collect::<Vec<_>>();

        emit(Event::Swap {
            user,
            tokens,
            amounts: ((*amounts.0).into(), (*amounts.1).into()),
            fees: &fees,
        });
    }

    fn log_update_pool_state_event(
        &mut self,
        reason: PoolUpdateReason,
        pool: (&TokenId, &TokenId),
        amounts_a: &RawFeeLevelsArray<Amount>,
        amounts_b: &RawFeeLevelsArray<Amount>,
        spot_sqrtprices: &RawFeeLevelsArray<dex::Float>,
        liquidities: &RawFeeLevelsArray<dex::Float>,
    ) {
        emit(Event::UpdatePoolState {
            pool,
            r#type: reason,
            amounts_a: &amounts_a.map(Into::into),
            amounts_b: &amounts_b.map(Into::into),
            sqrt_prices: &spot_sqrtprices.map(Into::into),
            liquidities: &liquidities.map(Into::into),
        });
    }

    fn log_add_verified_tokens_event(&mut self, tokens: &[TokenId]) {
        emit(Event::AddVerifiedTokens { tokens });
    }

    fn log_remove_verified_tokens_event(&mut self, tokens: &[TokenId]) {
        emit(Event::RemoveVerifiedTokens { tokens });
    }

    fn log_add_guard_accounts_event(&mut self, accounts: &[AccountId]) {
        emit(Event::AddGuardAccounts { accounts });
    }

    fn log_remove_guard_accounts_event(&mut self, accounts: &[AccountId]) {
        emit(Event::RemoveGuardAccounts { accounts });
    }

    fn log_suspend_payable_api_event(&mut self, account: &AccountId) {
        emit(Event::SuspendPayableAPI { account });
    }

    fn log_resume_payable_api_event(&mut self, account: &AccountId) {
        emit(Event::ResumePayableAPI { account });
    }
}

#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "event", content = "data")]
enum Event<'a> {
    Deposit {
        user: &'a AccountId,
        token_id: &'a TokenId,
        amount: U128,
        balance: U128,
    },
    Withdraw {
        user: &'a AccountId,
        token_id: &'a TokenId,
        amount: U128,
        balance: U128,
    },
    OpenPosition {
        user: &'a AccountId,
        pool: (&'a TokenId, &'a TokenId),
        amounts: (U128, U128),
        fee_rate: U64,
        position_id: U64,
    },
    ClosePosition {
        position_id: U64,
        amounts: (U128, U128),
    },
    HarvestFee {
        position_id: U64,
        amounts: (U128, U128),
    },
    Swap {
        user: &'a AccountId,
        tokens: (&'a TokenId, &'a TokenId),
        amounts: (U128, U128),
        fees: &'a [(&'a TokenId, U128)],
    },
    UpdatePoolState {
        pool: (&'a TokenId, &'a TokenId),
        r#type: PoolUpdateReason,
        amounts_a: &'a RawFeeLevelsArray<U128>,
        amounts_b: &'a RawFeeLevelsArray<U128>,
        sqrt_prices: &'a RawFeeLevelsArray<f64>,
        liquidities: &'a RawFeeLevelsArray<f64>,
    },
    StorageBalance {
        user: &'a AccountId,
        available: U128,
        total: U128,
    },
    AddVerifiedTokens {
        tokens: &'a [TokenId],
    },
    RemoveVerifiedTokens {
        tokens: &'a [TokenId],
    },
    AddGuardAccounts {
        accounts: &'a [AccountId],
    },
    RemoveGuardAccounts {
        accounts: &'a [AccountId],
    },
    SuspendPayableAPI {
        account: &'a AccountId,
    },
    ResumePayableAPI {
        account: &'a AccountId,
    },
}
