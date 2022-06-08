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
            .unwrap_or_else(|| panic!("Contract or token not whitelisted"))
            .checked_sub(amount.0)
            .unwrap_or_else(|| panic!("Not enough shares"));

        self.whitelisted_tokens
            .insert(&(contract_id.clone(), pool_id), &shares);

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
            .unwrap_or_else(|| panic!("Contract or token not whitelisted"))
            .checked_sub(amount.0)
            .unwrap_or_else(|| panic!("Not enough shares"));

        self.whitelisted_tokens
            .insert(&(contract_id.clone(), pool_id), &shares);

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
mod tests {
    use crate::ft_token_receiver::MFTTokenReceiver;
    use crate::ref_integration::RefPoolInfo;
    use crate::*;
    use near_sdk::json_types::{ValidAccountId, U128};
    use near_sdk::test_utils::{accounts, testing_env_with_promise_results, VMContextBuilder};
    use near_sdk::{testing_env, MockedBlockchain, PromiseResult};
    use serde_json::json;

    pub const ONE_YOCTO: u128 = 1;

    pub fn setup_contract() -> (VMContextBuilder, Contract) {
        let mut context = VMContextBuilder::new();

        testing_env!(context
            .block_timestamp(0)
            .predecessor_account_id(accounts(0))
            .build());

        (
            context,
            Contract::new(
                ValidAccountId::try_from("token.pembrock.testnet").unwrap(),
                vec![accounts(0)],
            ),
        )
    }

    // ---

    #[test]
    #[should_panic = "Not allowed"]
    fn proxy_mft_transfer_can_call_only_owner() {
        let (mut context, mut contract) = setup_contract();

        let amount = U128(1000);
        let contract_id = "token.testnet".to_owned();
        let pool_id = 0;
        let shares = 10_000;

        contract
            .whitelisted_tokens
            .insert(&(contract_id.clone(), pool_id), &shares);
        testing_env!(context.attached_deposit(ONE_YOCTO).build());
        contract.proxy_mft_transfer(
            format!("{}@{}", contract_id, pool_id),
            accounts(0),
            amount,
            None,
        );

        testing_env!(context
            .predecessor_account_id(accounts(1))
            .attached_deposit(ONE_YOCTO)
            .build());
        contract.proxy_mft_transfer(
            format!("{}@{}", contract_id, pool_id),
            accounts(0),
            amount,
            None,
        );
    }

    #[test]
    #[should_panic = "Not allowed"]
    fn proxy_mft_transfer_call_can_call_only_owner() {
        let (mut context, mut contract) = setup_contract();

        let amount = U128(1000);
        let contract_id = "token.testnet".to_owned();
        let pool_id = 0;
        let shares = 10_000;

        contract
            .whitelisted_tokens
            .insert(&(contract_id.clone(), pool_id), &shares);
        testing_env!(context.attached_deposit(ONE_YOCTO).build());
        contract.proxy_mft_transfer_call(
            format!("{}@{}", contract_id, pool_id),
            accounts(0),
            amount,
            None,
            "".to_owned(),
        );

        testing_env!(context
            .predecessor_account_id(accounts(1))
            .attached_deposit(ONE_YOCTO)
            .build());
        contract.proxy_mft_transfer_call(
            format!("{}@{}", contract_id, pool_id),
            accounts(0),
            amount,
            None,
            "".to_owned(),
        );
    }

    #[test]
    #[should_panic = "Contract or token not whitelisted"]
    fn proxy_mft_transfer_not_whitelisted_contract() {
        let (mut context, mut contract) = setup_contract();

        testing_env!(context
            .attached_deposit(ONE_YOCTO)
            .predecessor_account_id(accounts(0))
            .build());
        contract.proxy_mft_transfer("token.testnet@0".to_owned(), accounts(0), U128(1000), None);
    }

    #[test]
    #[should_panic = "Contract or token not whitelisted"]
    fn proxy_mft_transfer_call_not_whitelisted_contract() {
        let (mut context, mut contract) = setup_contract();

        testing_env!(context
            .attached_deposit(ONE_YOCTO)
            .predecessor_account_id(accounts(0))
            .build());
        contract.proxy_mft_transfer_call(
            "token.testnet@0".to_owned(),
            accounts(0),
            U128(1000),
            None,
            "".to_owned(),
        );
    }

    #[test]
    fn proxy_mft_transfer_cross_call_fail() {
        let (mut context, mut contract) = setup_contract();

        let amount = U128(1000);
        let contract_id = "token.testnet".to_owned();
        let pool_id = 0;
        let shares = 10_000;

        contract
            .whitelisted_tokens
            .insert(&(contract_id.clone(), pool_id), &shares);
        assert_eq!(
            shares,
            contract
                .whitelisted_tokens
                .get(&(contract_id.clone(), pool_id))
                .unwrap(),
            "Fail to init contract"
        );

        testing_env!(context.attached_deposit(ONE_YOCTO).build());
        contract.proxy_mft_transfer(
            format!("{}@{}", contract_id, pool_id),
            accounts(0),
            amount,
            None,
        );
        testing_env_with_promise_results(
            context.predecessor_account_id(accounts(0)).build(),
            PromiseResult::Failed,
        );
        contract.mft_transfer_callback(amount, contract_id.clone(), pool_id);

        assert_eq!(
            shares,
            contract
                .whitelisted_tokens
                .get(&(contract_id.clone(), pool_id))
                .unwrap(),
            "State not reverted on fail"
        );
    }

    #[test]
    fn proxy_mft_transfer_call_cross_call_fail() {
        let (mut context, mut contract) = setup_contract();

        let amount = U128(1000);
        let contract_id = "token.testnet".to_owned();
        let pool_id = 0;
        let shares = 10_000;

        contract
            .whitelisted_tokens
            .insert(&(contract_id.clone(), pool_id), &shares);
        assert_eq!(
            shares,
            contract
                .whitelisted_tokens
                .get(&(contract_id.clone(), pool_id))
                .unwrap(),
            "Fail to init contract"
        );

        testing_env!(context.attached_deposit(ONE_YOCTO).build());
        contract.proxy_mft_transfer_call(
            format!("{}@{}", contract_id, pool_id),
            accounts(0),
            amount,
            None,
            "".to_owned(),
        );
        testing_env_with_promise_results(
            context.predecessor_account_id(accounts(0)).build(),
            PromiseResult::Failed,
        );
        contract.mft_transfer_call_callback(amount, contract_id.clone(), pool_id);

        assert_eq!(
            shares,
            contract
                .whitelisted_tokens
                .get(&(contract_id.clone(), pool_id))
                .unwrap(),
            "State not reverted on fail"
        );
    }

    #[test]
    #[should_panic = "Not enough shares"]
    fn proxy_mft_transfer_not_enough_shares() {
        let (mut context, mut contract) = setup_contract();

        let amount = U128(1000);
        let contract_id = "token.testnet".to_owned();
        let pool_id = 0;
        let shares = 0;

        contract
            .whitelisted_tokens
            .insert(&(contract_id.clone(), pool_id), &shares);
        assert_eq!(
            shares,
            contract
                .whitelisted_tokens
                .get(&(contract_id.clone(), pool_id))
                .unwrap(),
            "Fail to init contract"
        );

        testing_env!(context.attached_deposit(ONE_YOCTO).build());
        contract.proxy_mft_transfer(
            format!("{}@{}", contract_id, pool_id),
            accounts(0),
            amount,
            None,
        );
    }

    #[test]
    #[should_panic = "Not enough shares"]
    fn proxy_mft_transfer_call_not_enough_shares() {
        let (mut context, mut contract) = setup_contract();

        let amount = U128(1000);
        let contract_id = "token.testnet".to_owned();
        let pool_id = 0;
        let shares = 0;

        contract
            .whitelisted_tokens
            .insert(&(contract_id.clone(), pool_id), &shares);
        assert_eq!(
            shares,
            contract
                .whitelisted_tokens
                .get(&(contract_id.clone(), pool_id))
                .unwrap(),
            "Fail to init contract"
        );

        testing_env!(context.attached_deposit(ONE_YOCTO).build());
        contract.proxy_mft_transfer_call(
            format!("{}@{}", contract_id, pool_id),
            accounts(0),
            amount,
            None,
            "".to_owned(),
        );
    }

    #[test]
    fn proxy_mft_transfer_success() {
        let (mut context, mut contract) = setup_contract();

        let amount = U128(1000);
        let contract_id = "token.testnet".to_owned();
        let pool_id = 0;
        let shares = 10_000;

        contract
            .whitelisted_tokens
            .insert(&(contract_id.clone(), pool_id), &shares);
        assert_eq!(
            shares,
            contract
                .whitelisted_tokens
                .get(&(contract_id.clone(), pool_id))
                .unwrap(),
            "Fail to init contract"
        );

        testing_env!(context.attached_deposit(ONE_YOCTO).build());
        contract.proxy_mft_transfer(
            format!("{}@{}", contract_id, pool_id),
            accounts(0),
            amount,
            None,
        );
        testing_env_with_promise_results(
            context.predecessor_account_id(accounts(0)).build(),
            PromiseResult::Successful(vec![]),
        );
        contract.mft_transfer_callback(amount, contract_id.clone(), pool_id);

        assert_eq!(
            shares - amount.0,
            contract
                .whitelisted_tokens
                .get(&(contract_id.clone(), pool_id))
                .unwrap(),
        );
    }

    #[test]
    fn proxy_mft_transfer_call_success() {
        let (mut context, mut contract) = setup_contract();

        let amount = U128(1000);
        let contract_id = "token.testnet".to_owned();
        let pool_id = 0;
        let shares = 10_000;

        contract
            .whitelisted_tokens
            .insert(&(contract_id.clone(), pool_id), &shares);
        assert_eq!(
            shares,
            contract
                .whitelisted_tokens
                .get(&(contract_id.clone(), pool_id))
                .unwrap(),
            "Fail to init contract"
        );

        testing_env!(context.attached_deposit(ONE_YOCTO).build());
        contract.proxy_mft_transfer_call(
            format!("{}@{}", contract_id, pool_id),
            accounts(0),
            amount,
            None,
            "".to_owned(),
        );
        testing_env_with_promise_results(
            context.predecessor_account_id(accounts(0)).build(),
            PromiseResult::Successful(serde_json::to_vec(&U128(0)).unwrap()),
        );
        contract.mft_transfer_call_callback(amount, contract_id.clone(), pool_id);

        assert_eq!(
            shares - amount.0,
            contract
                .whitelisted_tokens
                .get(&(contract_id.clone(), pool_id))
                .unwrap(),
        );
    }

    #[test]
    fn full_lp_flow() {
        let (mut context, mut contract) = setup_contract();

        let total_supply = U128(1000);
        let tokens_amount = U128(10_000);
        let incent = U128(1000);
        let contract_id = "token.testnet".to_owned();
        let pool_id = 0;
        let owner = accounts(0);
        let sender_id = accounts(1);
        let user_shares = U128(100);

        testing_env!(context.predecessor_account_id(owner.clone()).build());
        contract.add_to_whitelist(vec![(contract_id.clone(), pool_id)]);
        assert_eq!(
            0,
            contract
                .whitelisted_tokens
                .get(&(contract_id.clone(), pool_id))
                .unwrap(),
            "Fail to init contract"
        );

        testing_env!(context
            .predecessor_account_id(contract.token_account_id.clone().try_into().unwrap())
            .build());
        contract.ft_on_transfer(
            owner.clone(),
            incent,
            json!({
                "for_incent": true
            })
            .to_string(),
        );
        assert_eq!(incent.0, contract.incent_total_amount,);
        assert_eq!(0, contract.incent_locked_amount,);

        contract.mft_on_transfer(
            format!(":{}", pool_id),
            sender_id.to_string(),
            user_shares,
            "".to_owned(),
        );
        let ref_pool_info = RefPoolInfo {
            token_account_ids: vec![],
            amounts: vec![],
            total_fee: 10,
            shares_total_supply: total_supply,
        };
        testing_env_with_promise_results(
            context.predecessor_account_id(accounts(0)).build(),
            PromiseResult::Successful(serde_json::to_vec(&ref_pool_info).unwrap()),
        );
        contract.on_mft_callback(
            sender_id.to_string(),
            user_shares,
            contract_id.clone(),
            pool_id,
            ref_pool_info,
        );
        let amount_for_lockup =
            calculate_for_lockup(user_shares.0, tokens_amount.0, total_supply.0);
        assert_eq!(incent.0, contract.incent_total_amount,);
        assert_eq!(amount_for_lockup, contract.incent_locked_amount,);
        let lockup_index = 0; // First lockup
        let lockup = contract.lockups.get(lockup_index).unwrap();

        // TODO: check lockup

        contract.proxy_mft_transfer(
            format!("{}@{}", contract_id, pool_id),
            accounts(2),
            U128(amount_for_lockup),
            None,
        )
    }
}
