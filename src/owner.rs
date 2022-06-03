use crate::*;
use near_sdk::near_bindgen;

#[near_bindgen]
impl Contract {
    pub fn add_to_whitelist(&mut self, values: Vec<(AccountId, u64)>) {
        assert!(
            self.deposit_whitelist
                .contains(&env::predecessor_account_id()),
            "Not allowed"
        );

        self.whitelisted_tokens.extend(values);
    }

    pub fn remove_from_whitelist(&mut self, values: Vec<(AccountId, u64)>) {
        assert!(
            self.deposit_whitelist
                .contains(&env::predecessor_account_id()),
            "Not allowed"
        );

        values.iter().for_each(|value| {
            self.whitelisted_tokens.remove(value);
        });
    }

    pub fn set_state(&mut self, enabled: bool) {
        assert!(
            self.deposit_whitelist
                .contains(&env::predecessor_account_id()),
            "Not allowed"
        );

        self.enabled = enabled;

        log!("Contract {}", if enabled { "enabled" } else { "disabled" });
    }
}
