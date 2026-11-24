//! Slashing path for misbehaving keeper submissions (issue #1780).
//!
//! A submission is a keeper's signed claim about off-chain work. Only the
//! configured admin or slasher may slash one, and only once. Slashing is a
//! single contract invocation: stake is debited, the submission is marked,
//! accounting is updated and an event is published. A rejected slash leaves
//! every record untouched.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    BytesN, Env, Map, String, Symbol,
};

const CONFIG: Symbol = symbol_short!("CFG");
const NEXT_SUBMISSION_ID: Symbol = symbol_short!("NSUBID");
const SUBMISSIONS: Symbol = symbol_short!("SUBS");
const STAKES: Symbol = symbol_short!("STAKES");
const STATS: Symbol = symbol_short!("STATS");
const BPS_DENOMINATOR: u32 = 10_000;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlashingError {
    NotAuthorized = 1,
    NotInitialized = 2,
    AlreadyInitialized = 3,
    InvalidSlashBps = 4,
    SubmissionNotFound = 5,
    AlreadySlashed = 6,
    NoStake = 7,
    InvalidAmount = 8,
    InvalidReason = 9,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlashingConfig {
    pub admin: Address,
    pub slasher: Address,
    pub treasury: Address,
    pub slash_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeeperSubmission {
    pub id: u64,
    pub keeper: Address,
    pub payload_hash: BytesN<32>,
    pub submitted_at: u64,
    pub slashed: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlashingStats {
    pub total_slashed: i128,
    pub slash_count: u64,
}

#[contract]
pub struct KeeperSlashingContract;

#[contractimpl]
impl KeeperSlashingContract {
    /// Bind the privileged callers and set the slash rate.
    pub fn initialize(
        env: Env,
        admin: Address,
        slasher: Address,
        treasury: Address,
        slash_bps: u32,
    ) {
        if env.storage().instance().has(&CONFIG) {
            panic_with_error!(&env, SlashingError::AlreadyInitialized);
        }
        Self::validate_bps(&env, slash_bps);
        env.storage().instance().set(
            &CONFIG,
            &SlashingConfig {
                admin,
                slasher,
                treasury,
                slash_bps,
            },
        );
        env.storage().instance().set(&NEXT_SUBMISSION_ID, &1u64);
        env.storage()
            .instance()
            .set(&SUBMISSIONS, &Map::<u64, KeeperSubmission>::new(&env));
        env.storage()
            .instance()
            .set(&STAKES, &Map::<Address, i128>::new(&env));
        env.storage().instance().set(
            &STATS,
            &SlashingStats {
                total_slashed: 0,
                slash_count: 0,
            },
        );
    }

    /// Update the privileged callers or slash rate (admin only).
    pub fn configure(
        env: Env,
        admin: Address,
        slasher: Address,
        treasury: Address,
        slash_bps: u32,
    ) {
        let config = Self::get_config_internal(&env);
        admin.require_auth();
        if admin != config.admin {
            panic_with_error!(&env, SlashingError::NotAuthorized);
        }
        Self::validate_bps(&env, slash_bps);
        env.storage().instance().set(
            &CONFIG,
            &SlashingConfig {
                admin,
                slasher,
                treasury,
                slash_bps,
            },
        );
    }

    /// Lock stake that can be forfeited by a slash.
    pub fn stake(env: Env, keeper: Address, amount: i128) {
        keeper.require_auth();
        if amount <= 0 {
            panic_with_error!(&env, SlashingError::InvalidAmount);
        }
        let mut stakes: Map<Address, i128> = env.storage().instance().get(&STAKES).unwrap();
        let current = stakes.get(keeper.clone()).unwrap_or(0);
        stakes.set(
            keeper.clone(),
            current
                .checked_add(amount)
                .unwrap_or_else(|| panic_with_error!(&env, SlashingError::InvalidAmount)),
        );
        env.storage().instance().set(&STAKES, &stakes);
    }

    /// Register a submission. The keeper authenticates its own payload hash.
    pub fn submit(env: Env, keeper: Address, payload_hash: BytesN<32>) -> u64 {
        keeper.require_auth();
        let id: u64 = env.storage().instance().get(&NEXT_SUBMISSION_ID).unwrap();
        env.storage().instance().set(&NEXT_SUBMISSION_ID, &(id + 1));
        let submission = KeeperSubmission {
            id,
            keeper: keeper.clone(),
            payload_hash: payload_hash.clone(),
            submitted_at: env.ledger().timestamp(),
            slashed: false,
        };
        let submitted_at = submission.submitted_at;
        let mut submissions: Map<u64, KeeperSubmission> =
            env.storage().instance().get(&SUBMISSIONS).unwrap();
        submissions.set(id, submission);
        env.storage().instance().set(&SUBMISSIONS, &submissions);
        env.events().publish(
            (symbol_short!("ksub"), id),
            (keeper, payload_hash, submitted_at),
        );
        id
    }

    /// Slash a misbehaving keeper exactly once.
    ///
    /// Caller must be the configured slasher or admin. All state writes happen
    /// after every check, so a rejected slash is a no-op.
    pub fn slash(env: Env, caller: Address, submission_id: u64, reason: String) -> i128 {
        caller.require_auth();
        let config = Self::get_config_internal(&env);
        if caller != config.slasher && caller != config.admin {
            panic_with_error!(&env, SlashingError::NotAuthorized);
        }
        if reason.len() == 0 {
            panic_with_error!(&env, SlashingError::InvalidReason);
        }

        let mut submissions: Map<u64, KeeperSubmission> =
            env.storage().instance().get(&SUBMISSIONS).unwrap();
        let mut submission = submissions
            .get(submission_id)
            .unwrap_or_else(|| panic_with_error!(&env, SlashingError::SubmissionNotFound));
        if submission.slashed {
            panic_with_error!(&env, SlashingError::AlreadySlashed);
        }

        let mut stakes: Map<Address, i128> = env.storage().instance().get(&STAKES).unwrap();
        let stake = stakes.get(submission.keeper.clone()).unwrap_or(0);
        if stake <= 0 {
            panic_with_error!(&env, SlashingError::NoStake);
        }

        let amount = stake
            .checked_mul(i128::from(config.slash_bps))
            .unwrap_or_else(|| panic_with_error!(&env, SlashingError::InvalidAmount))
            / i128::from(BPS_DENOMINATOR);
        let amount = if amount > 0 { amount } else { 1 };
        let remaining = stake - amount;

        // All checks passed: commit the slash atomically.
        stakes.set(submission.keeper.clone(), remaining);
        env.storage().instance().set(&STAKES, &stakes);
        submission.slashed = true;
        submissions.set(submission_id, submission.clone());
        env.storage().instance().set(&SUBMISSIONS, &submissions);

        let mut stats: SlashingStats = env.storage().instance().get(&STATS).unwrap();
        stats.total_slashed = stats
            .total_slashed
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, SlashingError::InvalidAmount));
        stats.slash_count = stats.slash_count + 1;
        env.storage().instance().set(&STATS, &stats);

        env.events().publish(
            (symbol_short!("ksl"), submission_id),
            (
                submission.keeper,
                amount,
                remaining,
                reason,
                config.treasury,
            ),
        );
        amount
    }

    pub fn get_config(env: Env) -> SlashingConfig {
        Self::get_config_internal(&env)
    }

    pub fn get_submission(env: Env, submission_id: u64) -> KeeperSubmission {
        let submissions: Map<u64, KeeperSubmission> =
            env.storage().instance().get(&SUBMISSIONS).unwrap();
        submissions
            .get(submission_id)
            .unwrap_or_else(|| panic_with_error!(&env, SlashingError::SubmissionNotFound))
    }

    pub fn get_stake(env: Env, keeper: Address) -> i128 {
        let stakes: Map<Address, i128> = env.storage().instance().get(&STAKES).unwrap();
        stakes.get(keeper).unwrap_or(0)
    }

    pub fn get_stats(env: Env) -> SlashingStats {
        env.storage().instance().get(&STATS).unwrap()
    }

    fn get_config_internal(env: &Env) -> SlashingConfig {
        env.storage()
            .instance()
            .get(&CONFIG)
            .unwrap_or_else(|| panic_with_error!(env, SlashingError::NotInitialized))
    }

    fn validate_bps(env: &Env, slash_bps: u32) {
        if slash_bps == 0 || slash_bps > BPS_DENOMINATOR {
            panic_with_error!(env, SlashingError::InvalidSlashBps);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, BytesN, Error};

    fn setup(
        slash_bps: u32,
    ) -> (
        Env,
        Address,
        KeeperSlashingContractClient<'static>,
        Address,
        Address,
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(KeeperSlashingContract, ());
        let client = KeeperSlashingContractClient::new(&env, &id);
        let admin = Address::generate(&env);
        let slasher = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.initialize(&admin, &slasher, &treasury, &slash_bps);
        (env, id, client, admin, slasher, treasury)
    }

    fn hash(env: &Env, byte: u8) -> BytesN<32> {
        BytesN::from_array(env, &[byte; 32])
    }

    #[test]
    fn slashing_a_submission_deducts_stake_and_emits_an_event() {
        let (env, _id, client, _admin, slasher, _treasury) = setup(2_500);
        let keeper = Address::generate(&env);
        client.stake(&keeper, &10_000);
        let submission_id = client.submit(&keeper, &hash(&env, 1));

        let amount = client.slash(
            &slasher,
            &submission_id,
            &String::from_str(&env, "double-sign"),
        );

        assert_eq!(amount, 2_500);
        assert_eq!(client.get_stake(&keeper), 7_500);
        assert!(client.get_submission(&submission_id).slashed);
        assert_eq!(client.get_stats().total_slashed, 2_500);
        assert_eq!(client.get_stats().slash_count, 1);
    }

    #[test]
    fn only_the_slasher_or_admin_can_slash() {
        let (env, _id, client, admin, slasher, _treasury) = setup(1_000);
        let keeper = Address::generate(&env);
        let intruder = Address::generate(&env);
        client.stake(&keeper, &1_000);
        let submission_id = client.submit(&keeper, &hash(&env, 2));

        assert_eq!(
            client.try_slash(&intruder, &submission_id, &String::from_str(&env, "bad")),
            Err(Ok(Error::from_contract_error(1)))
        );
        assert_eq!(
            client.slash(&admin, &submission_id, &String::from_str(&env, "bad")),
            100
        );
        assert_eq!(
            client.try_slash(&slasher, &submission_id, &String::from_str(&env, "bad")),
            Err(Ok(Error::from_contract_error(6)))
        );
    }

    #[test]
    fn rejected_slashes_leave_state_untouched() {
        let (env, _id, client, _admin, slasher, _treasury) = setup(1_000);
        let keeper = Address::generate(&env);
        client.stake(&keeper, &1_000);
        let submission_id = client.submit(&keeper, &hash(&env, 3));

        assert_eq!(
            client.try_slash(&slasher, &99, &String::from_str(&env, "bad")),
            Err(Ok(Error::from_contract_error(5)))
        );
        assert_eq!(
            client.try_slash(&slasher, &submission_id, &String::from_str(&env, "")),
            Err(Ok(Error::from_contract_error(9)))
        );
        assert_eq!(client.get_stake(&keeper), 1_000);
        assert!(!client.get_submission(&submission_id).slashed);
        assert_eq!(client.get_stats().slash_count, 0);
    }

    #[test]
    fn bounds_and_tiny_stakes_are_handled() {
        let (env, _id, client, _admin, slasher, _treasury) = setup(10_000);
        let keeper = Address::generate(&env);
        client.stake(&keeper, &1);
        let submission_id = client.submit(&keeper, &hash(&env, 4));
        assert_eq!(
            client.slash(&slasher, &submission_id, &String::from_str(&env, "bad")),
            1
        );
        assert_eq!(client.get_stake(&keeper), 0);

        let (_env2, _id2, client2, admin2, slasher2, treasury2) = setup(100);
        assert_eq!(
            client2.try_configure(&admin2, &slasher2, &treasury2, &0),
            Err(Ok(Error::from_contract_error(4)))
        );
        assert_eq!(
            client2.try_configure(&admin2, &slasher2, &treasury2, &10_001),
            Err(Ok(Error::from_contract_error(4)))
        );
    }

    #[test]
    fn zero_stake_cannot_be_slashed() {
        let (env, _id, client, _admin, slasher, _treasury) = setup(500);
        let keeper = Address::generate(&env);
        let submission_id = client.submit(&keeper, &hash(&env, 5));
        assert_eq!(
            client.try_slash(&slasher, &submission_id, &String::from_str(&env, "bad")),
            Err(Ok(Error::from_contract_error(7)))
        );
        assert_eq!(client.get_stats().total_slashed, 0);
    }
}
