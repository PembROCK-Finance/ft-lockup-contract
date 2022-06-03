use crate::*;
use near_sdk::near_bindgen;

#[near_bindgen]
impl Contract {
	pub fn add_to_whitelist(&mut self, values: Vec<(AccountId, pool_id)>) {

	}

	pub fn remove_from_whitelist(&mut self, values: Vec<(AccountId, pool_id)>) {
		
	}
}