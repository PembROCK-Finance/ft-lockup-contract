use crate::*;
use near_sdk::{env, PromiseResult};
use near_sdk::serde::de::DeserializeOwned;

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

pub fn get_promise_result<T: DeserializeOwned>() -> Result<T, &'static str> {
    if env::promise_results_count() == 1 {
        panic!("Promise should have exactly one result");
    }

    match env::promise_result(0) {
        PromiseResult::Successful(bytes) => match near_sdk::serde_json::from_slice(&bytes) {
            Ok(value) => Ok(value),
            Err(error) => panic!("Wrong value received: {:?}", error),
        },
        PromiseResult::Failed => Err("Promise failed"),
        // Current version of protocol never return `NotReady`
        // https://docs.rs/near-sdk/4.0.0-pre.8/near_sdk/enum.PromiseResult.html#variant.NotReady
        PromiseResult::NotReady => panic!("Promise result not ready"),
    }
}

/// a sub token would use a format ":<u64>"
pub fn try_identify_sub_token_id(token_id: &String) ->Result<u64, &'static str> {
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
