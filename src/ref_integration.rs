use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{ext_contract, AccountId};

#[ext_contract(ext_exchange)]
pub trait ExtExchange {
    fn get_pool(&self, pool_id: u64) -> RefPoolInfo;
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(not(target_arch = "wasm32"), derive(Clone, Debug, PartialEq))]
pub struct RefPoolInfo {
    /// List of tokens in the pool.
    pub token_account_ids: Vec<AccountId>,
    /// How much NEAR this contract has.
    pub amounts: Vec<U128>,
    /// Fee charged for swap.
    /// In ref contract total_fee has type u32 but it shouldn't be more than 100% (10000). Typical value is 0.3% (30)
    pub total_fee: u16,
    /// Total number of shares.
    pub shares_total_supply: U128,
}
