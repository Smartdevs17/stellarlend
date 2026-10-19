use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn initialized_client<'a>(env: &'a Env) -> ReferralProgramClient<'a> {
    let contract_id = env.register(ReferralProgram, ());
    let client = ReferralProgramClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin, &1_000, &500, &10, &0, &5, &100, &20, &300);
    client
}

#[test]
fn referral_records_and_stats_are_scoped_to_addresses() {
    let env = Env::default();
    env.mock_all_auths();
    let client = initialized_client(&env);

    let referee_a = Address::generate(&env);
    let referee_b = Address::generate(&env);
    let referrer_a = Address::generate(&env);
    let referrer_b = Address::generate(&env);

    client.register_referral(&referee_a, &referrer_a);
    client.register_referral(&referee_b, &referrer_b);

    assert_eq!(client.get_referral(&referee_a).unwrap().referrer, referrer_a);
    assert_eq!(client.get_referral(&referee_b).unwrap().referrer, referrer_b);
    assert_eq!(client.get_referrer_stats(&referrer_a).total_referrals, 1);
    assert_eq!(client.get_referrer_stats(&referrer_b).total_referrals, 1);
}

#[test]
#[should_panic(expected = "HostError")]
fn accrue_fee_requires_referee_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let client = initialized_client(&env);

    let referee = Address::generate(&env);
    let referrer = Address::generate(&env);
    client.register_referral(&referee, &referrer);

    env.set_auths(&[]);
    client.accrue_fee(&referee, &100);
}

