mod setup;

use crate::setup::*;
use ft_lockup::lockup::Lockup;
use ft_lockup::schedule::{Checkpoint, Schedule};
use ft_lockup::termination::{HashOrSchedule, TerminationConfig};
use near_sdk::json_types::WrappedBalance;

const ONE_DAY_SEC: TimestampSec = 24 * 60 * 60;
const ONE_YEAR_SEC: TimestampSec = 365 * ONE_DAY_SEC;

const GENESIS_TIMESTAMP_SEC: TimestampSec = 1_600_000_000;

#[test]
fn test_init_env() {
    let e = Env::init(None);
    let _users = Users::init(&e);
}

#[test]
fn test_lockup_claim_logic() {
    let e = Env::init(None);
    let users = Users::init(&e);
    let amount = d(10000, TOKEN_DECIMALS);
    e.set_time_sec(GENESIS_TIMESTAMP_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());
    let lockup = Lockup {
        account_id: users.alice.valid_account_id(),
        schedule: Schedule(vec![
            Checkpoint {
                timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC - 1,
                balance: 0,
            },
            Checkpoint {
                timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC,
                balance: amount,
            },
        ]),
        claimed_balance: 0,
        termination_config: None,
    };
    let balance: WrappedBalance = e.add_lockup(&e.owner, amount, &lockup).unwrap_json();
    assert_eq!(balance.0, amount);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups.len(), 1);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // Claim attempt before unlock.
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, 0);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.claimed_balance, 0);

    // Set time to the first checkpoint.
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC - 1);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // Set time to the second checkpoint.
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, amount);

    // Attempt to claim. No storage deposit for Alice.
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, 0);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, amount);

    ft_storage_deposit(&users.alice, TOKEN_ID, &users.alice.account_id);

    let balance = e.ft_balance_of(&users.alice);
    assert_eq!(balance, 0);

    // Claim tokens.
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, amount);
    // User's lockups should be empty, since fully claimed.
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());

    // Manually checking the lockup by index
    let lockup = e.get_lockup(0);
    assert_eq!(lockup.claimed_balance, amount);
    assert_eq!(lockup.unclaimed_balance, 0);

    let balance = e.ft_balance_of(&users.alice);
    assert_eq!(balance, amount);
}

#[test]
fn test_lockup_linear() {
    let e = Env::init(None);
    let users = Users::init(&e);
    let amount = d(60000, TOKEN_DECIMALS);
    e.set_time_sec(GENESIS_TIMESTAMP_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());
    let lockup = Lockup {
        account_id: users.alice.valid_account_id(),
        schedule: Schedule(vec![
            Checkpoint {
                timestamp: GENESIS_TIMESTAMP_SEC,
                balance: 0,
            },
            Checkpoint {
                timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC,
                balance: amount,
            },
        ]),
        claimed_balance: 0,
        termination_config: None,
    };
    let balance: WrappedBalance = e.add_lockup(&e.owner, amount, &lockup).unwrap_json();
    assert_eq!(balance.0, amount);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups.len(), 1);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // 1/3 unlock
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC / 3);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, amount / 3);

    // Claim tokens
    ft_storage_deposit(&users.alice, TOKEN_ID, &users.alice.account_id);
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, amount / 3);
    let balance = e.ft_balance_of(&users.alice);
    assert_eq!(balance, amount / 3);

    // Check lockup after claim
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, amount / 3);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // 1/2 unlock
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC / 2);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, amount / 3);
    assert_eq!(lockups[0].1.unclaimed_balance, amount / 6);

    // Remove storage from token to verify claim refund.
    // Note, this burns `amount / 3` tokens.
    storage_force_unregister(&users.alice, TOKEN_ID);
    let balance = e.ft_balance_of(&users.alice);
    assert_eq!(balance, 0);

    // Trying to claim, should fail and refund the amount back to the lockup
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, 0);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, amount / 3);
    assert_eq!(lockups[0].1.unclaimed_balance, amount / 6);

    // Claim again but with storage deposit
    ft_storage_deposit(&users.alice, TOKEN_ID, &users.alice.account_id);
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, amount / 6);
    let balance = e.ft_balance_of(&users.alice);
    assert_eq!(balance, amount / 6);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, amount / 2);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // 2/3 unlock
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 2 / 3);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.claimed_balance, amount / 2);
    assert_eq!(lockups[0].1.unclaimed_balance, amount / 6);

    // Claim tokens
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, amount / 6);
    let balance = e.ft_balance_of(&users.alice);
    assert_eq!(balance, amount / 3);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.claimed_balance, amount * 2 / 3);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // Claim again with no unclaimed_balance
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, 0);
    let balance = e.ft_balance_of(&users.alice);
    assert_eq!(balance, amount / 3);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.claimed_balance, amount * 2 / 3);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // full unlock
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.claimed_balance, amount * 2 / 3);
    assert_eq!(lockups[0].1.unclaimed_balance, amount / 3);

    // Final claim
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, amount / 3);
    let balance = e.ft_balance_of(&users.alice);
    assert_eq!(balance, amount * 2 / 3);

    // User's lockups should be empty, since fully claimed.
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());

    // Manually checking the lockup by index
    let lockup = e.get_lockup(0);
    assert_eq!(lockup.claimed_balance, amount);
    assert_eq!(lockup.unclaimed_balance, 0);
}

#[test]
fn test_lockup_cliff_amazon() {
    let e = Env::init(None);
    let users = Users::init(&e);
    let amount = d(60000, TOKEN_DECIMALS);
    e.set_time_sec(GENESIS_TIMESTAMP_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());
    let lockup = Lockup {
        account_id: users.alice.valid_account_id(),
        schedule: Schedule(vec![
            Checkpoint {
                timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC - 1,
                balance: 0,
            },
            Checkpoint {
                timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC,
                balance: amount / 10,
            },
            Checkpoint {
                timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 2,
                balance: 3 * amount / 10,
            },
            Checkpoint {
                timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 3,
                balance: 6 * amount / 10,
            },
            Checkpoint {
                timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 4,
                balance: amount,
            },
        ]),
        claimed_balance: 0,
        termination_config: None,
    };
    let balance: WrappedBalance = e.add_lockup(&e.owner, amount, &lockup).unwrap_json();
    assert_eq!(balance.0, amount);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups.len(), 1);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // 1/12 time. pre-cliff unlock
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC / 3);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // 1/4 time. cliff unlock
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, amount / 10);

    // 3/8 time. cliff unlock + 1/2 of 2nd year.
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC + ONE_YEAR_SEC / 2);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.unclaimed_balance, 2 * amount / 10);

    // 1/2 time.
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 2);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.unclaimed_balance, 3 * amount / 10);

    // 1/2 + 1/12 time.
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 2 + ONE_YEAR_SEC / 3);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.unclaimed_balance, 4 * amount / 10);

    // 1/2 + 2/12 time.
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 2 + ONE_YEAR_SEC * 2 / 3);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.unclaimed_balance, 5 * amount / 10);

    // 3/4 time.
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 3);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.unclaimed_balance, 6 * amount / 10);

    // 7/8 time.
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 3 + ONE_YEAR_SEC / 2);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.unclaimed_balance, 8 * amount / 10);

    // full unlock.
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 4);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.unclaimed_balance, amount);

    // after unlock.
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 5);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.unclaimed_balance, amount);

    // attempt to claim without storage.
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, 0);
    let balance = e.ft_balance_of(&users.alice);
    assert_eq!(balance, 0);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.unclaimed_balance, amount);

    // Claim tokens
    ft_storage_deposit(&users.alice, TOKEN_ID, &users.alice.account_id);
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, amount);
    let balance = e.ft_balance_of(&users.alice);
    assert_eq!(balance, amount);

    // Check lockup after claim
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());
    let lockup = e.get_lockup(0);
    assert_eq!(lockup.claimed_balance, amount);
    assert_eq!(lockup.unclaimed_balance, 0);
}

#[test]
fn test_lockup_terminate_no_vesting_schedule() {
    let e = Env::init(None);
    let users = Users::init(&e);
    let amount = d(60000, TOKEN_DECIMALS);
    e.set_time_sec(GENESIS_TIMESTAMP_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());
    let lockup = Lockup {
        account_id: users.alice.valid_account_id(),
        schedule: Schedule(vec![
            Checkpoint {
                timestamp: GENESIS_TIMESTAMP_SEC,
                balance: 0,
            },
            Checkpoint {
                timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC,
                balance: amount,
            },
        ]),
        claimed_balance: 0,
        termination_config: Some(TerminationConfig {
            terminator_id: users.eve.valid_account_id(),
            vesting_schedule: None,
        }),
    };

    let balance: WrappedBalance = e.add_lockup(&e.owner, amount, &lockup).unwrap_json();
    assert_eq!(balance.0, amount);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups.len(), 1);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // 1/3 unlock
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC / 3);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, amount / 3);

    // Claim tokens
    ft_storage_deposit(&users.alice, TOKEN_ID, &users.alice.account_id);
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, amount / 3);
    let balance = e.ft_balance_of(&users.alice);
    assert_eq!(balance, amount / 3);

    // Check lockup after claim
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, amount / 3);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // 1/2 unlock
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC / 2);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, amount / 3);
    assert_eq!(lockups[0].1.unclaimed_balance, amount / 6);

    let lockup_index = lockups[0].0;

    // TERMINATE
    ft_storage_deposit(&users.eve, TOKEN_ID, &users.eve.account_id);
    let res: WrappedBalance = e.terminate(&users.eve, lockup_index).unwrap_json();
    assert_eq!(res.0, amount / 2);

    let terminator_balance = e.ft_balance_of(&users.eve);
    assert_eq!(terminator_balance, amount / 2);

    // full unlock 2 / 3 period after termination before initial timestamp
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 2 / 3);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.claimed_balance, amount / 3);
    assert_eq!(lockups[0].1.unclaimed_balance, amount / 6);

    // Final claim
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, amount / 6);
    let balance = e.ft_balance_of(&users.alice);
    assert_eq!(balance, amount / 2);

    // User's lockups should be empty, since fully claimed.
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());

    // Manually checking the lockup by index
    let lockup = e.get_lockup(0);
    assert_eq!(lockup.claimed_balance, amount / 2);
    assert_eq!(lockup.unclaimed_balance, 0);
}

#[test]
fn test_lockup_terminate_no_termination_config() {
    let e = Env::init(None);
    let users = Users::init(&e);
    let amount = d(60000, TOKEN_DECIMALS);
    e.set_time_sec(GENESIS_TIMESTAMP_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());
    let lockup = Lockup {
        account_id: users.alice.valid_account_id(),
        schedule: Schedule(vec![
            Checkpoint {
                timestamp: GENESIS_TIMESTAMP_SEC,
                balance: 0,
            },
            Checkpoint {
                timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC,
                balance: amount,
            },
        ]),
        claimed_balance: 0,
        termination_config: None,
    };

    let balance: WrappedBalance = e.add_lockup(&e.owner, amount, &lockup).unwrap_json();
    assert_eq!(balance.0, amount);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups.len(), 1);
    let lockup_index = lockups[0].0;
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // TERMINATE
    ft_storage_deposit(&users.eve, TOKEN_ID, &users.eve.account_id);
    let res = e.terminate(&users.eve, lockup_index);
    assert!(!res.is_ok());
    assert!(format!("{:?}", res.status()).contains("No termination config"));
}

#[test]
fn test_lockup_terminate_wrong_terminator() {
    let e = Env::init(None);
    let users = Users::init(&e);
    let amount = d(60000, TOKEN_DECIMALS);
    e.set_time_sec(GENESIS_TIMESTAMP_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());
    let lockup = Lockup {
        account_id: users.alice.valid_account_id(),
        schedule: Schedule(vec![
            Checkpoint {
                timestamp: GENESIS_TIMESTAMP_SEC,
                balance: 0,
            },
            Checkpoint {
                timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC,
                balance: amount,
            },
        ]),
        claimed_balance: 0,
        termination_config: Some(TerminationConfig {
            terminator_id: users.eve.valid_account_id(),
            vesting_schedule: None,
        }),
    };

    let balance: WrappedBalance = e.add_lockup(&e.owner, amount, &lockup).unwrap_json();
    assert_eq!(balance.0, amount);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups.len(), 1);
    let lockup_index = lockups[0].0;
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // TERMINATE
    ft_storage_deposit(&users.dude, TOKEN_ID, &users.dude.account_id);
    let res = e.terminate(&users.dude, lockup_index);
    assert!(!res.is_ok());
    assert!(format!("{:?}", res.status()).contains("Unauthorized"));
}

#[test]
fn test_lockup_terminate_no_storage() {
    let e = Env::init(None);
    let users = Users::init(&e);
    let amount = d(60000, TOKEN_DECIMALS);
    e.set_time_sec(GENESIS_TIMESTAMP_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());
    let lockup = Lockup {
        account_id: users.alice.valid_account_id(),
        schedule: Schedule(vec![
            Checkpoint {
                timestamp: GENESIS_TIMESTAMP_SEC,
                balance: 0,
            },
            Checkpoint {
                timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC,
                balance: amount,
            },
        ]),
        claimed_balance: 0,
        termination_config: Some(TerminationConfig {
            terminator_id: users.eve.valid_account_id(),
            vesting_schedule: None,
        }),
    };

    let balance: WrappedBalance = e.add_lockup(&e.owner, amount, &lockup).unwrap_json();
    assert_eq!(balance.0, amount);
    let lockups = e.get_account_lockups(&users.alice);
    let lockup_index = lockups[0].0;

    // 1/3 unlock, terminate
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC / 3);
    // Claim tokens
    // TERMINATE, without deposit must create unlocked lockup for terminator
    let res: WrappedBalance = e.terminate(&users.eve, lockup_index).unwrap_json();
    assert_eq!(res.0, 0);

    ft_storage_deposit(&users.eve, TOKEN_ID, &users.eve.account_id);
    let terminator_balance = e.ft_balance_of(&users.eve);
    assert_eq!(terminator_balance, 0);

    {
        let lockups = e.get_account_lockups(&users.eve);
        assert_eq!(lockups.len(), 1);
        assert_eq!(lockups[0].1.claimed_balance, 0);
        assert_eq!(lockups[0].1.unclaimed_balance, amount * 2 / 3);
        let terminator_lockup_index = lockups[0].0;

        // Claim from lockup refund
        let res: WrappedBalance = e.claim(&users.eve).unwrap_json();
        assert_eq!(res.0, amount * 2 / 3);
        let balance = e.ft_balance_of(&users.eve);
        assert_eq!(balance, amount * 2 / 3);

        // Terminator's lockups should be empty, since fully claimed.
        let lockups = e.get_account_lockups(&users.eve);
        assert!(lockups.is_empty());

        // Manually checking the terminator's lockup by index
        let lockup = e.get_lockup(terminator_lockup_index);
        assert_eq!(lockup.claimed_balance, amount * 2 / 3);
        assert_eq!(lockup.unclaimed_balance, 0);
    }

    {
        let lockups = e.get_account_lockups(&users.alice);
        assert_eq!(lockups.len(), 1);
        assert_eq!(lockups[0].1.claimed_balance, 0);
        assert_eq!(lockups[0].1.unclaimed_balance, amount / 3);

        // Claim by user
        ft_storage_deposit(&users.alice, TOKEN_ID, &users.alice.account_id);
        let balance = e.ft_balance_of(&users.alice);
        assert_eq!(balance, 0);

        let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
        assert_eq!(res.0, amount / 3);
        let balance = e.ft_balance_of(&users.alice);
        assert_eq!(balance, amount / 3);

        // User's lockups should be empty, since fully claimed.
        let lockups = e.get_account_lockups(&users.alice);
        assert!(lockups.is_empty());

        // Manually checking the terminator's lockup by index
        let lockup = e.get_lockup(lockup_index);
        assert_eq!(lockup.claimed_balance, amount / 3);
        assert_eq!(lockup.unclaimed_balance, 0);
    }
}

fn lockup_vesting_schedule(amount: u128) -> (Schedule, Schedule) {
    let lockup_schedule = Schedule(vec![
        Checkpoint {
            timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 2,
            balance: 0,
        },
        Checkpoint {
            timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 4,
            balance: amount * 3 / 4,
        },
        Checkpoint {
            timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 4 + 1,
            balance: amount,
        },
    ]);
    let vesting_schedule = Schedule(vec![
        Checkpoint {
            timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC - 1,
            balance: 0,
        },
        Checkpoint {
            timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC,
            balance: amount / 4,
        },
        Checkpoint {
            timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 4,
            balance: amount,
        },
    ]);
    (lockup_schedule, vesting_schedule)
}

#[test]
fn test_lockup_terminate_custom_vesting_hash() {
    let e = Env::init(None);
    let users = Users::init(&e);
    let amount = d(60000, TOKEN_DECIMALS);
    e.set_time_sec(GENESIS_TIMESTAMP_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());

    let (lockup_schedule, vesting_schedule) = lockup_vesting_schedule(amount);
    let vesting_hash = e.hash_schedule(&vesting_schedule);
    let lockup = Lockup {
        account_id: users.alice.valid_account_id(),
        schedule: lockup_schedule,
        claimed_balance: 0,
        termination_config: Some(TerminationConfig {
            terminator_id: users.eve.valid_account_id(),
            vesting_schedule: Some(HashOrSchedule::Hash(vesting_hash)),
        }),
    };

    let balance: WrappedBalance = e.add_lockup(&e.owner, amount, &lockup).unwrap_json();
    assert_eq!(balance.0, amount);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups.len(), 1);
    let lockup_index = lockups[0].0;

    // 1Y, 1 / 4 vested, 0 unlocked
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // TERMINATE
    ft_storage_deposit(&users.eve, TOKEN_ID, &users.eve.account_id);
    let res: WrappedBalance = e
        .terminate_with_schedule(&users.eve, lockup_index, vesting_schedule)
        .unwrap_json();
    assert_eq!(res.0, amount * 3 / 4);
    let terminator_balance = e.ft_balance_of(&users.eve);
    assert_eq!(terminator_balance, amount * 3 / 4);

    // Checking lockup
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount / 4);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // Rewind to 2Y + Y * 2 / 3, 1/4 of original unlock, full vested unlock
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 2 + ONE_YEAR_SEC * 2 / 3);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount / 4);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, amount / 4);

    // claiming
    ft_storage_deposit(&users.alice, TOKEN_ID, &users.alice.account_id);
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, amount / 4);

    // Checking lockups
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());

    // User lockups are empty
    let lockup = e.get_lockup(lockup_index);
    assert_eq!(lockup.total_balance, amount / 4);
    assert_eq!(lockup.claimed_balance, amount / 4);
    assert_eq!(lockup.unclaimed_balance, 0);
}

#[test]
fn test_lockup_terminate_custom_vesting_invalid_hash() {
    let e = Env::init(None);
    let users = Users::init(&e);
    let amount = d(60000, TOKEN_DECIMALS);
    e.set_time_sec(GENESIS_TIMESTAMP_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());

    let (lockup_schedule, vesting_schedule) = lockup_vesting_schedule(amount);
    let vesting_hash = e.hash_schedule(&vesting_schedule);
    let lockup = Lockup {
        account_id: users.alice.valid_account_id(),
        schedule: lockup_schedule,
        claimed_balance: 0,
        termination_config: Some(TerminationConfig {
            terminator_id: users.eve.valid_account_id(),
            vesting_schedule: Some(HashOrSchedule::Hash(vesting_hash)),
        }),
    };

    let balance: WrappedBalance = e.add_lockup(&e.owner, amount, &lockup).unwrap_json();
    assert_eq!(balance.0, amount);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups.len(), 1);
    let lockup_index = lockups[0].0;

    // 1Y, 1 / 4 vested, 0 unlocked
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // TERMINATE
    let fake_schedule = Schedule(vec![
        Checkpoint {
            timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 2,
            balance: 0,
        },
        Checkpoint {
            timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 4,
            balance: amount,
        },
    ]);
    ft_storage_deposit(&users.eve, TOKEN_ID, &users.eve.account_id);
    let res = e.terminate_with_schedule(&users.eve, lockup_index, fake_schedule);
    assert!(!res.is_ok());
    assert!(format!("{:?}", res.status()).contains("The revealed schedule hash doesn't match"));
}

#[test]
fn test_lockup_terminate_custom_vesting_incompatible_vesting_schedule_by_hash() {
    let e = Env::init(None);
    let users = Users::init(&e);
    let amount = d(60000, TOKEN_DECIMALS);
    e.set_time_sec(GENESIS_TIMESTAMP_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());

    let (lockup_schedule, _vesting_schedule) = lockup_vesting_schedule(amount);
    let incompatible_vesting_schedule = Schedule(vec![
        Checkpoint {
            timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 4,
            balance: 0,
        },
        Checkpoint {
            timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 4 + 1,
            balance: amount,
        },
    ]);
    let incompatible_vesting_hash = e.hash_schedule(&incompatible_vesting_schedule);
    let lockup = Lockup {
        account_id: users.alice.valid_account_id(),
        schedule: lockup_schedule,
        claimed_balance: 0,
        termination_config: Some(TerminationConfig {
            terminator_id: users.eve.valid_account_id(),
            vesting_schedule: Some(HashOrSchedule::Hash(incompatible_vesting_hash)),
        }),
    };

    let balance: WrappedBalance = e.add_lockup(&e.owner, amount, &lockup).unwrap_json();
    assert_eq!(balance.0, amount);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups.len(), 1);
    let lockup_index = lockups[0].0;

    // 1Y, 1 / 4 vested, 0 unlocked
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // TERMINATE
    ft_storage_deposit(&users.eve, TOKEN_ID, &users.eve.account_id);
    let res = e.terminate_with_schedule(&users.eve, lockup_index, incompatible_vesting_schedule);
    assert!(!res.is_ok());
    assert!(format!("{:?}", res.status()).contains("The lockup schedule is ahead of"));
}

#[test]
fn test_validate_schedule() {
    let e = Env::init(None);
    let users = Users::init(&e);
    let amount = d(60000, TOKEN_DECIMALS);
    e.set_time_sec(GENESIS_TIMESTAMP_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());

    let (lockup_schedule, vesting_schedule) = lockup_vesting_schedule(amount);

    let res = e.validate_schedule(&lockup_schedule, amount.into(), Some(&vesting_schedule));
    assert!(res.is_ok());

    let incompatible_vesting_schedule = Schedule(vec![
        Checkpoint {
            timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 4,
            balance: 0,
        },
        Checkpoint {
            timestamp: GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 4 + 1,
            balance: amount,
        },
    ]);
    let res = e.validate_schedule(
        &lockup_schedule,
        amount.into(),
        Some(&incompatible_vesting_schedule),
    );
    assert!(!res.is_ok());
    assert!(format!("{:?}", res.unwrap_err()).contains("The lockup schedule is ahead of"));
}

#[test]
fn test_lockup_terminate_custom_vesting_terminate_before_cliff() {
    let e = Env::init(None);
    let users = Users::init(&e);
    let amount = d(60000, TOKEN_DECIMALS);
    e.set_time_sec(GENESIS_TIMESTAMP_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());

    let (lockup_schedule, vesting_schedule) = lockup_vesting_schedule(amount);
    let lockup = Lockup {
        account_id: users.alice.valid_account_id(),
        schedule: lockup_schedule,
        claimed_balance: 0,
        termination_config: Some(TerminationConfig {
            terminator_id: users.eve.valid_account_id(),
            vesting_schedule: Some(HashOrSchedule::Schedule(vesting_schedule)),
        }),
    };

    let balance: WrappedBalance = e.add_lockup(&e.owner, amount, &lockup).unwrap_json();
    assert_eq!(balance.0, amount);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups.len(), 1);
    let lockup_index = lockups[0].0;

    // 1Y - 1 before cliff termination
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC - 1);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // TERMINATE
    ft_storage_deposit(&users.eve, TOKEN_ID, &users.eve.account_id);
    let res: WrappedBalance = e.terminate(&users.eve, lockup_index).unwrap_json();
    assert_eq!(res.0, amount);

    let terminator_balance = e.ft_balance_of(&users.eve);
    assert_eq!(terminator_balance, amount);

    // Checking lockup

    // after ALL the schedules have finished

    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());

    let lockup = e.get_lockup(lockup_index);
    assert_eq!(lockup.total_balance, 0);
    assert_eq!(lockup.claimed_balance, 0);
    assert_eq!(lockup.unclaimed_balance, 0);

    // Trying to claim
    ft_storage_deposit(&users.alice, TOKEN_ID, &users.alice.account_id);
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, 0);

    let balance = e.ft_balance_of(&users.alice);
    assert_eq!(balance, 0);
}

#[test]
fn test_lockup_terminate_custom_vesting_before_release() {
    let e = Env::init(None);
    let users = Users::init(&e);
    let amount = d(60000, TOKEN_DECIMALS);
    e.set_time_sec(GENESIS_TIMESTAMP_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());

    let (lockup_schedule, vesting_schedule) = lockup_vesting_schedule(amount);
    let lockup = Lockup {
        account_id: users.alice.valid_account_id(),
        schedule: lockup_schedule,
        claimed_balance: 0,
        termination_config: Some(TerminationConfig {
            terminator_id: users.eve.valid_account_id(),
            vesting_schedule: Some(HashOrSchedule::Schedule(vesting_schedule)),
        }),
    };

    let balance: WrappedBalance = e.add_lockup(&e.owner, amount, &lockup).unwrap_json();
    assert_eq!(balance.0, amount);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups.len(), 1);
    let lockup_index = lockups[0].0;

    // 1Y, 1 / 4 vested, 0 unlocked
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // TERMINATE
    ft_storage_deposit(&users.eve, TOKEN_ID, &users.eve.account_id);
    let res: WrappedBalance = e.terminate(&users.eve, lockup_index).unwrap_json();
    assert_eq!(res.0, amount * 3 / 4);
    let terminator_balance = e.ft_balance_of(&users.eve);
    assert_eq!(terminator_balance, amount * 3 / 4);

    // Checking lockup
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount / 4);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, 0);

    // Trying to claim
    ft_storage_deposit(&users.alice, TOKEN_ID, &users.alice.account_id);
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, 0);

    // Rewind to 2Y + Y/3, 1/8 of original should be unlocked
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 2 + ONE_YEAR_SEC / 3);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount / 4);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, amount / 8);

    // claiming
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, amount / 8);

    // Rewind to 2Y + Y * 2 / 3, 1/4 of original unlock, full vested unlock
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 2 + ONE_YEAR_SEC * 2 / 3);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount / 4);
    assert_eq!(lockups[0].1.claimed_balance, amount / 8);
    assert_eq!(lockups[0].1.unclaimed_balance, amount / 8);

    // claiming
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, amount / 8);

    // Checking lockups
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());

    // User lockups are empty
    let lockup = e.get_lockup(lockup_index);
    assert_eq!(lockup.total_balance, amount / 4);
    assert_eq!(lockup.claimed_balance, amount / 4);
    assert_eq!(lockup.unclaimed_balance, 0);
}

#[test]
fn test_lockup_terminate_custom_vesting_during_release() {
    let e = Env::init(None);
    let users = Users::init(&e);
    let amount = d(60000, TOKEN_DECIMALS);
    e.set_time_sec(GENESIS_TIMESTAMP_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());

    let (lockup_schedule, vesting_schedule) = lockup_vesting_schedule(amount);
    let lockup = Lockup {
        account_id: users.alice.valid_account_id(),
        schedule: lockup_schedule,
        claimed_balance: 0,
        termination_config: Some(TerminationConfig {
            terminator_id: users.eve.valid_account_id(),
            vesting_schedule: Some(HashOrSchedule::Schedule(vesting_schedule)),
        }),
    };

    let balance: WrappedBalance = e.add_lockup(&e.owner, amount, &lockup).unwrap_json();
    assert_eq!(balance.0, amount);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups.len(), 1);
    let lockup_index = lockups[0].0;

    // 2Y + Y / 3, 1/8 unlocked
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 2 + ONE_YEAR_SEC / 3);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, amount / 8);

    // Trying to claim
    ft_storage_deposit(&users.alice, TOKEN_ID, &users.alice.account_id);
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, amount / 8);

    // TERMINATE, 2Y + Y / 2, 5/8 unlocked
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 2 + ONE_YEAR_SEC / 2);
    ft_storage_deposit(&users.eve, TOKEN_ID, &users.eve.account_id);
    let res: WrappedBalance = e.terminate(&users.eve, lockup_index).unwrap_json();
    assert_eq!(res.0, amount * 3 / 8);
    let terminator_balance = e.ft_balance_of(&users.eve);
    assert_eq!(terminator_balance, amount * 3 / 8);

    // Checking lockup
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount * 5 / 8);
    assert_eq!(lockups[0].1.claimed_balance, amount / 8);
    assert_eq!(lockups[0].1.unclaimed_balance, amount / 16);

    // Rewind to 2Y + Y*2/3, 1/4 of original should be unlocked
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 2 + ONE_YEAR_SEC * 2 / 3);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount * 5 / 8);
    assert_eq!(lockups[0].1.claimed_balance, amount / 8);
    assert_eq!(lockups[0].1.unclaimed_balance, amount / 8);

    // claiming
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, amount / 8);

    // Rewind to 3Y + Y * 2 / 3, 5/8 of original unlock, full vested unlock
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 3 + ONE_YEAR_SEC * 2 / 3);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount * 5 / 8);
    assert_eq!(lockups[0].1.claimed_balance, amount * 2 / 8);
    assert_eq!(lockups[0].1.unclaimed_balance, amount * 3 / 8);

    // claiming
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, amount * 3 / 8);

    // Checking lockups
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());

    // User lockups are empty
    let lockup = e.get_lockup(lockup_index);
    assert_eq!(lockup.total_balance, amount * 5 / 8);
    assert_eq!(lockup.claimed_balance, amount * 5 / 8);
    assert_eq!(lockup.unclaimed_balance, 0);
}

#[test]
fn test_lockup_terminate_custom_vesting_during_lockup_cliff() {
    let e = Env::init(None);
    let users = Users::init(&e);
    let amount = d(60000, TOKEN_DECIMALS);
    e.set_time_sec(GENESIS_TIMESTAMP_SEC);
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());

    let (lockup_schedule, vesting_schedule) = lockup_vesting_schedule(amount);
    let lockup = Lockup {
        account_id: users.alice.valid_account_id(),
        schedule: lockup_schedule,
        claimed_balance: 0,
        termination_config: Some(TerminationConfig {
            terminator_id: users.eve.valid_account_id(),
            vesting_schedule: Some(HashOrSchedule::Schedule(vesting_schedule)),
        }),
    };

    let balance: WrappedBalance = e.add_lockup(&e.owner, amount, &lockup).unwrap_json();
    assert_eq!(balance.0, amount);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups.len(), 1);
    let lockup_index = lockups[0].0;

    // 2Y + Y * 2 / 3, 1/8 unlocked
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 2 + ONE_YEAR_SEC * 2 / 3);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount);
    assert_eq!(lockups[0].1.claimed_balance, 0);
    assert_eq!(lockups[0].1.unclaimed_balance, amount / 4);

    // Trying to claim
    ft_storage_deposit(&users.alice, TOKEN_ID, &users.alice.account_id);
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, amount / 4);

    // TERMINATE, 3Y + Y / 3, 5/6 unlocked
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 3 + ONE_YEAR_SEC / 3);
    ft_storage_deposit(&users.eve, TOKEN_ID, &users.eve.account_id);
    let res: WrappedBalance = e.terminate(&users.eve, lockup_index).unwrap_json();
    assert_eq!(res.0, amount / 6);
    let terminator_balance = e.ft_balance_of(&users.eve);
    assert_eq!(terminator_balance, amount / 6);

    // Checking lockup
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount * 5 / 6);
    assert_eq!(lockups[0].1.claimed_balance, amount / 4);
    assert_eq!(lockups[0].1.unclaimed_balance, amount / 4);

    // claiming
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, amount / 4);

    // Rewind to 4Y
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 4);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount * 5 / 6);
    assert_eq!(lockups[0].1.claimed_balance, amount * 1 / 2);
    assert_eq!(lockups[0].1.unclaimed_balance, amount * 1 / 4);

    // Rewind to 4Y + 1, full unlock including part of cliff
    e.set_time_sec(GENESIS_TIMESTAMP_SEC + ONE_YEAR_SEC * 4 + 1);
    let lockups = e.get_account_lockups(&users.alice);
    assert_eq!(lockups[0].1.total_balance, amount * 5 / 6);
    assert_eq!(lockups[0].1.claimed_balance, amount * 1 / 2);
    assert_eq!(lockups[0].1.unclaimed_balance, amount * 1 / 3);

    // claiming
    let res: WrappedBalance = e.claim(&users.alice).unwrap_json();
    assert_eq!(res.0, amount * 1 / 3);

    // Checking lockups
    let lockups = e.get_account_lockups(&users.alice);
    assert!(lockups.is_empty());

    // User lockups are empty
    let lockup = e.get_lockup(lockup_index);
    assert_eq!(lockup.total_balance, amount * 5 / 6);
    assert_eq!(lockup.claimed_balance, amount * 5 / 6);
    assert_eq!(lockup.unclaimed_balance, 0);
}

#[test]
fn test_deposit_whitelist_get() {
    let e = Env::init(None);
    let users = Users::init(&e);
    // let amount = d(60000, TOKEN_DECIMALS);
    // e.set_time_sec(GENESIS_TIMESTAMP_SEC);
    // let lockups = e.get_account_lockups(&users.alice);
    // assert!(lockups.is_empty());

    // deposit whitelist has owner by default
    let deposit_whitelist = e.get_deposit_whitelist();
    assert_eq!(deposit_whitelist, vec![e.owner.account_id.clone()]);

    // add to whitelist
    let result = e.add_to_deposit_whitelist(&e.owner, &users.eve.valid_account_id());
    assert!(result.is_ok());

    let deposit_whitelist = e.get_deposit_whitelist();
    assert_eq!(
        deposit_whitelist,
        vec![e.owner.account_id.clone(), users.eve.account_id.clone()]
    );

    // remove from whiltelist
    let result = e.remove_from_deposit_whitelist(&users.eve, &e.owner.valid_account_id());
    assert!(result.is_ok());

    let deposit_whitelist = e.get_deposit_whitelist();
    assert_eq!(
        deposit_whitelist,
        vec![users.eve.account_id.clone()]
    );
}
