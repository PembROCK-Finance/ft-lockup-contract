use crate::*;
use near_sdk::env;
use primitive_types::U256;

pub(crate) fn nano_to_sec(timestamp: Timestamp) -> TimestampSec {
    (timestamp / 10u64.pow(9)) as _
}

pub(crate) fn current_timestamp_sec() -> TimestampSec {
    nano_to_sec(env::block_timestamp())
}

pub mod u128_dec_format {
    use near_sdk::serde::de;
    use near_sdk::serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(num: &u128, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&num.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u128, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

/// a value: "<contract_id>@<u64>"
pub fn try_identify_contract_id_and_sub_token_id(
    token_id: &String,
) -> Result<(AccountId, u64), &'static str> {
    let mut values = token_id.split('@');

    let contract_id = values.next().ok_or("Missing contract id")?;
    let pool_id = values
        .next()
        .ok_or("Missing pool id")?
        .parse()
        .map_err(|_| "Illegal pool id")?;

    Ok((contract_id.to_owned(), pool_id))
}

/// a sub token would use a format ":<u64>"
pub fn try_identify_sub_token_id(token_id: &String) -> Result<u64, &'static str> {
    if token_id.starts_with(":") {
        if let Ok(pool_id) = str::parse::<u64>(&token_id[1..token_id.len()]) {
            Ok(pool_id)
        } else {
            Err("Illegal pool id")
        }
    } else {
        Err("Illegal pool id")
    }
}

// TODO: use U256 for better math
pub fn calculate_for_lockup(user_shares: u128, amount: u128, shares_total_supply: u128) -> u128 {
    (U256::from(user_shares) * U256::from(amount) * U256::from(2_u64) * U256::from(12_u64)
        / U256::from(shares_total_supply)
        / U256::from(10_u64))
    .as_u128()
}

pub fn try_calculate_gas(
    gas_for_cross_call: Gas,
    minimum_gas_for_callback: Gas,
    gas_reserve: Gas,
) -> Result<Gas, &'static str> {
    #[cfg(feature = "debug")]
    log!(
        "Gas: prepaid - {:?}, used - {:?}, for cross-call - {:?}, minimum for callback - {:?}, gas reserve - {:?}",
        env::prepaid_gas(),
        env::used_gas(),
        gas_for_cross_call,
        minimum_gas_for_callback,
        gas_reserve,
    );
    match env::prepaid_gas().checked_sub(env::used_gas() + gas_reserve + gas_for_cross_call) {
        Some(gas_left) if gas_left >= minimum_gas_for_callback => Ok(gas_left),
        _ => Err("Not enough gas"),
    }
}
