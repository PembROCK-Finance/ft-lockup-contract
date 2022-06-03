use crate::ref_integration::{ext_exchange, RefPoolInfo};
use crate::*;
use near_sdk::{log, near_bindgen, AccountId, Gas, PromiseOrValue};
use std::convert::TryFrom;

#[near_bindgen]
impl FungibleTokenReceiver for Contract {
    fn ft_on_transfer(
        &mut self,
        sender_id: ValidAccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        assert_eq!(
            env::predecessor_account_id(),
            self.token_account_id,
            "Invalid token ID"
        );
        self.assert_deposit_whitelist(sender_id.as_ref());
        let lockup: Lockup = serde_json::from_str(&msg).expect("Expected Lockup as msg");

        match lockup.for_incent {
            Some(true) => {
                self.for_incent += amount.0;
                log!("Write amount for incent");
                PromiseOrValue::Value(0.into())
            }
            _ => {
                let amount = amount.into();
                lockup.assert_new_valid(amount);
                let index = self.internal_add_lockup(&lockup);
                log!(
                    "Created new lockup for {} with index {}",
                    lockup.account_id.as_ref(),
                    index,
                );
                PromiseOrValue::Value(0.into())
            }
        }
    }
}

pub trait MFTTokenReceiver {
    fn mft_on_transfer(
        &mut self,
        token_id: String,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128>;
}

// TODO: move
pub const GAS_FOR_GET_REF_POOL_INFO: Gas = 10_000_000_000_000;
pub const NO_DEPOSIT: Balance = 0;

/// seed token deposit
#[near_bindgen]
impl MFTTokenReceiver for Contract {
    /// Callback on receiving tokens by this contract.
    fn mft_on_transfer(
        &mut self,
        token_id: String,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        // get_pool
        let pool_id = try_identify_sub_token_id(&token_id).unwrap_or_else(|err| panic!("{}", err));
        assert!(
            self.whitelisted_tokens
                .contains(&(env::predecessor_account_id(), pool_id)),
            "Contract or token not whitelisted"
        );

        let gas_reserve = 50_000_000_000_000;
        let callback_gas =
            try_calculate_gas(GAS_FOR_GET_REF_POOL_INFO, 50_000_000_000_000, gas_reserve)
                .unwrap_or_else(|error| panic!("{}", error));

        ext_exchange::get_pool(
            pool_id,
            &env::predecessor_account_id(),
            NO_DEPOSIT,
            GAS_FOR_GET_REF_POOL_INFO,
        )
        .then(ext_on_mft::on_mft_callback(
            sender_id,
            amount,
            &env::current_account_id(),
            NO_DEPOSIT,
            callback_gas,
        ))
        .into()
    }
}

#[ext_contract(ext_on_mft)]
pub trait OnMftTransfer {
    fn on_mft_callback(
        &mut self,
        sender_id: AccountId,
        user_shares: U128,
        #[callback] pool_info: RefPoolInfo,
    ) -> PromiseOrValue<U128>;
}

#[near_bindgen]
impl Contract {
    pub fn on_mft_callback(
        &mut self,
        sender_id: AccountId,
        user_shares: U128,
        #[callback] pool_info: RefPoolInfo,
    ) -> PromiseOrValue<U128> {
        let amount_index = pool_info
            .token_account_ids
            .iter()
            .position(|r| r == &String::from("token.pembrock.testnet"))
            .unwrap_or_else(|| panic!("No token.pembrock.near in PoolInfo"));
        let amount = pool_info.amounts[amount_index].0;

        let amount_for_lockup =
            calculate_for_lockup(user_shares.0, amount, pool_info.shares_total_supply.0);

        let timestamp = (env::block_timestamp() / 1_000_000_000) as u32;

        let lockup = Lockup {
            account_id: ValidAccountId::try_from(sender_id)
                .unwrap_or_else(|_| panic!("Invalid AccountID")),
            schedule: Schedule(vec![
                Checkpoint {
                    timestamp,
                    balance: 0,
                },
                Checkpoint {
                    timestamp: timestamp + 86400 * 180, // add half a year
                    balance: amount_for_lockup,
                },
            ]),
            claimed_balance: 0,
            termination_config: None,
            for_incent: None,
        };
        let index = self.internal_add_lockup(&lockup);

        self.for_incent = self
            .for_incent
            .checked_sub(amount_for_lockup)
            .unwrap_or_else(|| panic!("For incent is too low"));

        log!(
            "Created new lockup for {} with index {} with amount {}",
            lockup.account_id.as_ref(),
            index,
            amount_for_lockup,
        );

        PromiseOrValue::Value(U128(0))
    }
}
