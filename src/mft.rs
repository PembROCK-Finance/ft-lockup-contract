use crate::*;
use near_sdk::{env, near_bindgen};

pub const GAS_FOR_MFT_TRANSFER: Gas = 45_000_000_000_000;
pub const GAS_FOR_MFT_CALLBACK: Gas = 50_000_000_000_000;

#[near_bindgen]
impl Contract {
    #[payable]
    pub fn mft_transfer(
        &mut self,
        token_id: String,
        receiver_id: ValidAccountId,
        amount: U128,
        memo: Option<String>,
    ) {
        let (contract_id, pool_id) = try_identify_contract_id_and_sub_token_id(&token_id)
            .unwrap_or_else(|error| panic!("{}", error));

        let shares = self
            .whitelisted_tokens
            .get(&(contract_id.clone(), pool_id))
            .unwrap_or_else(|| panic!("Contract or token not whitelisted"));

        self.whitelisted_tokens
            .insert(&(contract_id.clone(), pool_id), &(shares - amount.0));

        let gas_reserve = 50_000_000_000_000;
        let callback_gas =
            try_calculate_gas(GAS_FOR_MFT_TRANSFER, GAS_FOR_MFT_CALLBACK, gas_reserve)
                .unwrap_or_else(|error| panic!("{}", error));

        ext_mft::mft_transfer(
            token_id,
            receiver_id,
            amount,
            memo,
            &contract_id,
            NO_DEPOSIT,
            callback_gas,
        )
        .then(ext_mft::mft_transfer_callback(
            amount,
            contract_id,
            pool_id,
            &env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_MFT_CALLBACK,
        ));

        todo!()
    }

    #[payable]
    pub fn mft_transfer_call(
        &mut self,
        token_id: String,
        receiver_id: ValidAccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let (contract_id, pool_id) = try_identify_contract_id_and_sub_token_id(&token_id)
            .unwrap_or_else(|error| panic!("{}", error));

        let shares = self
            .whitelisted_tokens
            .get(&(contract_id.clone(), pool_id))
            .unwrap_or_else(|| panic!("Contract or token not whitelisted"));

        self.whitelisted_tokens
            .insert(&(contract_id.clone(), pool_id), &(shares - amount.0));

        let gas_reserve = 50_000_000_000_000;
        let callback_gas = try_calculate_gas(GAS_FOR_MFT_TRANSFER, 50_000_000_000_000, gas_reserve)
            .unwrap_or_else(|error| panic!("{}", error));

        ext_mft::mft_transfer_call(
            token_id,
            receiver_id,
            amount,
            memo,
            msg,
            &contract_id,
            NO_DEPOSIT,
            callback_gas,
        )
        .then(ext_mft::mft_transfer_call_callback(
            amount,
            contract_id,
            pool_id,
            &env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_MFT_CALLBACK,
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
        if is_promise_success() {
            return PromiseOrValue::Value(U128(0));
        }

        let shares = self
            .whitelisted_tokens
            .get(&(contract_id.clone(), pool_id))
            .unwrap_or_else(|| panic!("Contract or token not whitelisted"));

        // rollback
        self.whitelisted_tokens
            .insert(&(contract_id, pool_id), &(shares + amount.0));

        PromiseOrValue::Value(amount)
    }
}
