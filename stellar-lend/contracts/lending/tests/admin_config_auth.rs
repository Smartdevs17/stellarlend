mod common;

use test_utils::snapshotless_env;

#[test]
fn configuration_entry_points_require_the_stored_admin() {
    let env = snapshotless_env();
    env.mock_all_auths();
    let (client, _admin) = common::deploy(&env);
    env.set_auths(&[]);

    assert!(client
        .try_initialize_deposit_settings(&1_000_000_000, &1)
        .is_err());
    assert!(client.try_set_deposit_paused(&true).is_err());
    assert!(client.try_set_emergency_withdraw_limit(&1_000).is_err());
    assert!(client.try_initialize_withdraw_settings(&1).is_err());
    assert!(client.try_set_withdraw_paused(&true).is_err());
    assert!(client
        .try_initialize_borrow_settings(&1_000_000_000, &1)
        .is_err());
}
