use crate::ref_integration::{ext_exchange, RefPoolInfo};
use crate::*;
use near_sdk::{env, near_bindgen};

pub const GAS_FOR_MFT_TRANSFER: Gas = 20_000_000_000_000;
pub const GAS_FOR_MFT_TRANSFER_CALLBACK: Gas = 15_000_000_000_000;
pub const GAS_FOR_MFT_TRANSFER_CALL: Gas = 70_000_000_000_000; // 25 for mft_on_transfer
pub const GAS_FOR_MFT_TRANSFER_CALL_CALLBACK: Gas = 20_000_000_000_000;

#[near_bindgen]
impl Contract {
    #[payable]
    pub fn proxy_mft_transfer(
        &mut self,
        token_id: String,
        receiver_id: ValidAccountId,
        amount: U128,
        memo: Option<String>,
    ) {
        assert_one_yocto();
        assert!(
            self.deposit_whitelist
                .contains(&env::predecessor_account_id()),
            "Not allowed"
        );

        let (contract_id, pool_id) = try_identify_contract_id_and_sub_token_id(&token_id)
            .unwrap_or_else(|error| panic!("{}", error));

        let shares = self
            .whitelisted_tokens
            .get(&(contract_id.clone(), pool_id))
            .unwrap_or_else(|| panic!("Contract or token not whitelisted"));

        self.whitelisted_tokens
            .insert(&(contract_id.clone(), pool_id), &(shares - amount.0));

        ext_mft::mft_transfer(
            token_id,
            receiver_id,
            amount,
            memo,
            &contract_id,
            NO_DEPOSIT,
            GAS_FOR_MFT_TRANSFER,
        )
        .then(ext_mft::mft_transfer_callback(
            amount,
            contract_id,
            pool_id,
            &env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_MFT_TRANSFER_CALLBACK,
        ));
    }

    #[payable]
    pub fn proxy_mft_transfer_call(
        &mut self,
        token_id: String,
        receiver_id: ValidAccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        assert_one_yocto();
        assert!(
            self.deposit_whitelist
                .contains(&env::predecessor_account_id()),
            "Not allowed"
        );

        let (contract_id, pool_id) = try_identify_contract_id_and_sub_token_id(&token_id)
            .unwrap_or_else(|error| panic!("{}", error));

        let shares = self
            .whitelisted_tokens
            .get(&(contract_id.clone(), pool_id))
            .unwrap_or_else(|| panic!("Contract or token not whitelisted"));

        self.whitelisted_tokens
            .insert(&(contract_id.clone(), pool_id), &(shares - amount.0));

        ext_mft::mft_transfer_call(
            token_id,
            receiver_id,
            amount,
            memo,
            msg,
            &contract_id,
            NO_DEPOSIT,
            GAS_FOR_MFT_TRANSFER_CALL,
        )
        .then(ext_mft::mft_transfer_call_callback(
            amount,
            contract_id,
            pool_id,
            &env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_MFT_TRANSFER_CALL_CALLBACK,
        ))
        .into()
    }
}

#[ext_contract(ext_mft)]
pub trait MftTransfer {
    fn mft_transfer(
        &mut self,
        token_id: String,
        receiver_id: ValidAccountId,
        amount: U128,
        memo: Option<String>,
    );

    fn mft_transfer_call(
        &mut self,
        token_id: String,
        receiver_id: ValidAccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128>;

    fn mft_transfer_callback(&mut self, amount: U128, contract_id: AccountId, pool_id: u64);

    fn mft_transfer_call_callback(
        &mut self,
        amount: U128,
        contract_id: AccountId,
        pool_id: u64,
    ) -> PromiseOrValue<U128>;
}

#[near_bindgen]
impl Contract {
    #[private]
    pub fn mft_transfer_callback(&mut self, amount: U128, contract_id: AccountId, pool_id: u64) {
        if is_promise_success() {
            return;
        }

        let shares = self
            .whitelisted_tokens
            .get(&(contract_id.clone(), pool_id))
            .unwrap_or_else(|| panic!("Contract or token not whitelisted"));

        // rollback
        self.whitelisted_tokens
            .insert(&(contract_id, pool_id), &(shares + amount.0));
    }

    #[private]
    pub fn mft_transfer_call_callback(
        &mut self,
        amount: U128,
        contract_id: AccountId,
        pool_id: u64,
    ) -> PromiseOrValue<U128> {
        let unused_amount = match get_promise_result::<U128>() {
            Ok(unused_amount) => unused_amount.0,
            Err(error) => {
                log!("Mft transfer call fail: {}", error);
                amount.0
            }
        };

        if unused_amount > 0 {
            let shares = self
                .whitelisted_tokens
                .get(&(contract_id.clone(), pool_id))
                .unwrap_or_else(|| panic!("Contract or token not whitelisted"));

            // rollback
            self.whitelisted_tokens
                .insert(&(contract_id, pool_id), &(shares + unused_amount));
        }

        PromiseOrValue::Value(unused_amount.into())
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
pub const GAS_FOR_GET_POOL: Gas = 10_000_000_000_000;
pub const NO_DEPOSIT: Balance = 0;

pub const GAS_FOR_MFT_ON_TRANSFER_ONLY: Gas = 20_000_000_000_000; // without cross call and callback gas

/// seed token deposit
#[near_bindgen]
impl MFTTokenReceiver for Contract {
    /// Callback on receiving tokens by this contract.
    ///  - total gas = GAS_FOR_MFT_ON_TRANSFER + gas for get_pool
    fn mft_on_transfer(
        &mut self,
        token_id: String,
        sender_id: AccountId,
        amount: U128,
        _msg: String,
    ) -> PromiseOrValue<U128> {
        assert!(self.enabled, "Contract paused");
        // get_pool
        let pool_id = try_identify_sub_token_id(&token_id).unwrap_or_else(|err| panic!("{}", err));
        let exchange_contract_id = env::predecessor_account_id();
        assert!(
            self.whitelisted_tokens
                .get(&(exchange_contract_id.clone(), pool_id))
                .is_some(),
            "Contract or token not whitelisted"
        );

        ext_exchange::get_pool(pool_id, &exchange_contract_id, NO_DEPOSIT, GAS_FOR_GET_POOL)
            .then(ext_on_mft::on_mft_callback(
                sender_id,
                amount,
                exchange_contract_id,
                pool_id,
                &env::current_account_id(),
                NO_DEPOSIT,
                env::prepaid_gas() - GAS_FOR_MFT_ON_TRANSFER_ONLY - GAS_FOR_GET_POOL,
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
        exchange_contract_id: AccountId,
        pool_id: u64,
        #[callback] pool_info: RefPoolInfo,
    ) -> PromiseOrValue<U128>;
}

#[near_bindgen]
impl Contract {
    pub fn on_mft_callback(
        &mut self,
        sender_id: AccountId,
        user_shares: U128,
        exchange_contract_id: AccountId,
        pool_id: u64,
        #[callback] pool_info: RefPoolInfo,
    ) -> PromiseOrValue<U128> {
        let amount_index = pool_info
            .token_account_ids
            .iter()
            .position(|r| r == &self.token_account_id)
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
        };
        let index = self.internal_add_lockup(&lockup);

        self.incent_locked_amount += amount_for_lockup;

        assert!(
            self.incent_locked_amount > self.incent_total_amount,
            "For incent is too low"
        );

        let shares = self
            .whitelisted_tokens
            .get(&(exchange_contract_id.clone(), pool_id))
            .unwrap_or_else(|| panic!("Contract or token not whitelisted"));
        self.whitelisted_tokens
            .insert(&(exchange_contract_id, pool_id), &(shares + user_shares.0));

        log!(
            "Created new lockup for {} with index {} with amount {}",
            lockup.account_id.as_ref(),
            index,
            amount_for_lockup,
        );

        PromiseOrValue::Value(U128(0))
    }
}

#[cfg(test)]
pub mod tests {
    #[test]
    #[ignore]
    #[should_panic = "Contract or token not whitelisted"]
    fn proxy_mft_transfer_not_whitelisted_contract() {
        todo!()
    }

    #[test]
    #[ignore]
    #[should_panic = "Contract or token not whitelisted"]
    fn proxy_mft_transfer_call_not_whitelisted_contract() {
        todo!()
    }

    #[test]
    #[ignore]
    fn proxy_mft_transfer_cross_call_fail() {
        todo!() // check that state not changed
    }

    #[test]
    #[ignore]
    fn proxy_mft_transfer_call_cross_call_fail() {
        todo!() // check that state not changed
    }
}
