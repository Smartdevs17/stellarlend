use soroban_sdk::{
    testutils::{Address as _, EnvTestConfig, Ledger as _},
    Address, Env, String as SorobanString,
};

pub struct TestEnv {
    pub env: Env,
    pub admin: Address,
    pub users: Vec<Address>,
}

impl TestEnv {
    pub fn new() -> Self {
        Self::from_env(Env::default())
    }

    /// Like [`TestEnv::new`], but the env does not write a test snapshot on
    /// drop. Use it for suites that create many environments (property tests,
    /// benchmarks, seeded scenarios).
    pub fn snapshotless() -> Self {
        Self::from_env(snapshotless_env())
    }

    /// Wrap an existing env: mocks all auths and generates an admin.
    pub fn from_env(env: Env) -> Self {
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let users = Vec::new();

        Self { env, admin, users }
    }

    pub fn with_timestamp(self, timestamp: u64) -> Self {
        self.env.ledger().with_mut(|li| li.timestamp = timestamp);
        self
    }

    pub fn with_ledger_sequence(self, sequence: u32) -> Self {
        self.env
            .ledger()
            .with_mut(|li| li.sequence_number = sequence);
        self
    }

    /// Lift the default CPU/memory limits, e.g. for long seeded sequences.
    /// Gas measurements through [`crate::bench::GasMeter`] still reset the
    /// budget per step.
    pub fn with_unlimited_budget(self) -> Self {
        self.env.cost_estimate().budget().reset_unlimited();
        self
    }

    pub fn generate_user(&mut self) -> Address {
        let user = Address::generate(&self.env);
        self.users.push(user.clone());
        user
    }

    pub fn generate_users(&mut self, count: usize) -> Vec<Address> {
        let mut generated = Vec::new();
        for _ in 0..count {
            generated.push(self.generate_user());
        }
        generated
    }

    pub fn advance_time(&mut self, seconds: u64) {
        advance_time(&self.env, seconds);
    }

    pub fn advance_ledger(&mut self, blocks: u32) {
        let current = self.env.ledger().sequence();
        self.env
            .ledger()
            .with_mut(|li| li.sequence_number = current + blocks);
    }
}

impl Default for TestEnv {
    fn default() -> Self {
        Self::new()
    }
}

/// An env that skips writing `test_snapshots/` on drop. Auths are not mocked,
/// so authorization tests can use it as-is.
pub fn snapshotless_env() -> Env {
    Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    })
}

/// Move the ledger clock forward by `seconds`.
pub fn advance_time(env: &Env, seconds: u64) {
    let current = env.ledger().timestamp();
    env.ledger().with_mut(|li| li.timestamp = current + seconds);
}

pub fn create_string(env: &Env, value: &str) -> SorobanString {
    SorobanString::from_str(env, value)
}

pub fn setup_test_env() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    (env, admin)
}

pub fn setup_test_env_with_users(user_count: usize) -> (Env, Address, Vec<Address>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let mut users = Vec::new();
    for _ in 0..user_count {
        users.push(Address::generate(&env));
    }
    (env, admin, users)
}
