use crate::serde_json::Value;
use crate::*;
use near_sdk::{log, near_bindgen, PromiseOrValue};

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

        let value: Value = serde_json::from_str(&msg).expect("Invalid JSON in msg");
        if let Some(Some(true)) = value.get("for_incent").map(|value| value.as_bool()) {
            self.incent_total_amount += amount.0;
            log!("Write amount for incent");
            return PromiseOrValue::Value(0.into());
        }

        let lockup: Lockup = serde_json::from_value(value).expect("Expected Lockup as msg");
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
