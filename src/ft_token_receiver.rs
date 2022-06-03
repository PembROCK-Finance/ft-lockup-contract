use crate::*;

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

        match lockup.flag {
            Some(true) => {
                self.for_incent += amount.0;
                PromiseOrValue::Value(0.into()) // is this right??
            }
            _ => {
                let amount = amount.into();
                lockup.assert_new_valid(amount);
                let index = self.internal_add_lockup(&lockup);
                log!(
                    "Created new lockup for {} with index {}",
                    lockup.account_id.as_ref(),
                    index
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
        // TODO: get_pool
        // TODO: calculate amount_for_lockup = user_shares * amounts2 * 24 / shares_total_supply / 10
        // TODO: create Lockup with args
        // TODO: self.for_incent -= amount_for_lockup
        PromiseOrValue::Value(U128(0))
    }
}

