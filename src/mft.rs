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

#[cfg(test)]
pub mod tests {
    #[test]
    #[should_panic = "Contract or token not whitelisted"]
    fn proxy_mft_transfer_not_whitelisted_contract() {
        todo!()
    }

    #[test]
    #[should_panic = "Contract or token not whitelisted"]
    fn proxy_mft_transfer_call_not_whitelisted_contract() {
        todo!()
    }

    #[test]
    fn proxy_mft_transfer_cross_call_fail() {
        todo!() // check that state not changed
    }

    #[test]
    fn proxy_mft_transfer_call_cross_call_fail() {
        todo!() // check that state not changed
    }
}
