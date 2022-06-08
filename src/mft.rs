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

pub trait MFTTokenReceiver {
    fn mft_on_transfer(
        &mut self,
        token_id: String,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128>;
}

pub const GAS_FOR_GET_POOL: Gas = 10_000_000_000_000;
pub const GAS_FOR_MFT_ON_TRANSFER_ONLY: Gas = 20_000_000_000_000; // without cross call and callback gas

pub const NO_DEPOSIT: Balance = 0;
pub const LP_LOCKUP_DURATION: u32 = 86400 * 180; // add half a year

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
        msg: String,
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
                    timestamp: timestamp + LP_LOCKUP_DURATION,
                    balance: amount_for_lockup,
                },
            ]),
            claimed_balance: 0,
            termination_config: None,
        };
        let index = self.internal_add_lockup(&lockup);

        self.incent_locked_amount += amount_for_lockup;

        assert!(
            self.incent_locked_amount <= self.incent_total_amount,
            "Incent total amount is too low"
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
mod tests {
    use crate::mft::MFTTokenReceiver;
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
        let contract_id = "exchange.testnet".to_owned();
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
        let contract_id = "exchange.testnet".to_owned();
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
        contract.proxy_mft_transfer(
            "exchange.testnet@0".to_owned(),
            accounts(0),
            U128(1000),
            None,
        );
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
            "exchange.testnet@0".to_owned(),
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
        let contract_id = "exchange.testnet".to_owned();
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
        let contract_id = "exchange.testnet".to_owned();
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
        let contract_id = "exchange.testnet".to_owned();
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
        let contract_id = "exchange.testnet".to_owned();
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
        let contract_id = "exchange.testnet".to_owned();
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
        let contract_id = "exchange.testnet".to_owned();
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

        let owner_id = accounts(0);
        let sender_id = accounts(1);
        let exchange_contract_id = accounts(2);
        let pool_id = 0;
        let token1_id: ValidAccountId = contract.token_account_id.clone().try_into().unwrap();
        let token2_id = accounts(3);
        let token1_amount = U128(15171821497385474264559);
        let token2_amount = U128(229955497070989231115133755);
        let shares_total_supply = U128(1965922955983163067462272);
        let incent_total_amount = U128(1000000000000000000000000);
        let user_shares = U128(611350868216586967105518);

        let amount_for_lockup = 11323299786443666399999; // calculate_for_lockup(user_shares.0, token1_amount.0, shares_total_supply.0);

        let ref_pool_info = RefPoolInfo {
            token_account_ids: vec![token1_id.into(), token2_id.into()],
            amounts: vec![token1_amount, token2_amount],
            total_fee: 30,
            shares_total_supply,
        };

        testing_env!(context.predecessor_account_id(owner_id.clone()).build());
        contract.add_to_whitelist(vec![(exchange_contract_id.to_string(), pool_id)]);
        assert_eq!(
            contract
                .whitelisted_tokens
                .get(&(exchange_contract_id.to_string(), pool_id))
                .unwrap(),
            0
        );

        testing_env!(context
            .predecessor_account_id(contract.token_account_id.clone().try_into().unwrap())
            .build());
        contract.ft_on_transfer(
            owner_id.clone(),
            incent_total_amount,
            json!({
                "for_incent": true
            })
            .to_string(),
        );
        assert_eq!(contract.incent_total_amount, incent_total_amount.0);
        assert_eq!(contract.incent_locked_amount, 0);

        testing_env!(context
            .predecessor_account_id(exchange_contract_id.clone().try_into().unwrap())
            .build());
        contract.mft_on_transfer(
            format!(":{}", pool_id),
            sender_id.to_string(),
            user_shares,
            "".to_owned(),
        );
        testing_env_with_promise_results(
            context.build(),
            PromiseResult::Successful(serde_json::to_vec(&ref_pool_info).unwrap()),
        );
        contract.on_mft_callback(
            sender_id.to_string(),
            user_shares,
            exchange_contract_id.to_string(),
            pool_id,
            ref_pool_info,
        );

        assert_eq!(
            contract
                .whitelisted_tokens
                .get(&(exchange_contract_id.to_string(), pool_id))
                .unwrap(),
            user_shares.0
        );

        assert_eq!(contract.incent_total_amount, incent_total_amount.0);
        assert_eq!(contract.incent_locked_amount, amount_for_lockup);
        let lockup_index = 0; // First lockup
        let lockup = contract.lockups.get(lockup_index).unwrap();

        assert_eq!(lockup.schedule.0[1].balance, amount_for_lockup);

        testing_env!(context
            .predecessor_account_id(owner_id.clone())
            .attached_deposit(ONE_YOCTO)
            .build());
        contract.proxy_mft_transfer(
            format!("{}@{}", exchange_contract_id, pool_id),
            owner_id,
            user_shares,
            None,
        );

        assert_eq!(
            contract
                .whitelisted_tokens
                .get(&(exchange_contract_id.to_string(), pool_id))
                .unwrap(),
            0
        );
    }
}
