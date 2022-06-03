use crate::*;
use near_sdk::near_bindgen;

#[near_bindgen]
impl Contract {
    pub fn add_to_whitelist(&mut self, values: Vec<(u64, AccountId)>) {
        assert!(
            self.deposit_whitelist
                .contains(&env::predecessor_account_id()),
            "Not allowed"
        );

        self.whitelisted_tokens.extend(values);
    }

    pub fn remove_from_whitelist(&mut self, values: Vec<u64>) {
        assert!(
            self.deposit_whitelist
                .contains(&env::predecessor_account_id()),
            "Not allowed"
        );

        values.iter().for_each(|value| {
            self.whitelisted_tokens.remove(value);
        });
    }
}
