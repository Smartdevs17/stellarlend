use soroban_sdk::{
    contract, contractimpl, contracttype, token, xdr::ToXdr, Address, Bytes, BytesN, Env, Vec,
};

use auction_types::{Auction, AuctionConfig, AuctionState, BidCommitment, BidResult};

const BPS_DIVISOR: i128 = 10000;

#[contracttype]
enum DataKey {
    Admin,
    Governance,
    AuctionCount,
    Auction(u64),
    Commitments(u64),
    BidsReceived(u64),
    NonceUsed(Address),
}

fn compute_current_price(config: &AuctionConfig, auction: &Auction, current_time: u64) -> i128 {
    if current_time >= auction.end_time {
        return auction.floor_price;
    }

    let elapsed = current_time - auction.start_time;
    let total_duration = auction.config.auction_duration;

    if total_duration == 0 {
        return auction.floor_price;
    }

    let elapsed_bps = (elapsed as i128) * BPS_DIVISOR / (total_duration as i128);

    match config.price_decay_function {
        auction_types::PriceDecayFunction::Linear => {
            auction.starting_price
                - ((auction.starting_price - auction.floor_price) * elapsed_bps / BPS_DIVISOR)
        }
        auction_types::PriceDecayFunction::Exponential => {
            let remaining_ratio = (BPS_DIVISOR - elapsed_bps) as u128;
            let price_range = (auction.starting_price - auction.floor_price) as u128;
            let decayed = (price_range * remaining_ratio * remaining_ratio
                / (BPS_DIVISOR as u128)
                / (BPS_DIVISOR as u128)) as i128;
            auction.floor_price + decayed
        }
    }
}

fn hash_bid(
    bidder: &Address,
    auction_id: u64,
    collateral_amount: i128,
    max_price: i128,
    nonce: &BytesN<32>,
    env: &Env,
) -> BytesN<32> {
    let mut data = Bytes::new(env);
    data.append(&bidder.to_xdr(env));
    data.append(&auction_id.to_xdr(env));
    data.append(&collateral_amount.to_xdr(env));
    data.append(&max_price.to_xdr(env));
    data.append(&nonce.to_xdr(env));
    env.crypto().sha256(&data).into()
}

/// Atomically settle a bid's asset movements before any auction accounting is
/// committed:
///   1. collect the exact `debt_repaid` of `debt_asset` from the bidder and
///      route the proceeds to the lending pool (debt settlement path);
///   2. transfer the filled `collateral_asset` from the auction contract to
///      the bidder.
///
/// A failed payment (zero balance, missing allowance, frozen token) panics and
/// reverts the whole call, so auction state can never be mutated without the
/// corresponding assets moving.
fn settle_bid_payment(
    env: &Env,
    auction: &Auction,
    bidder: &Address,
    debt_repaid: i128,
    collateral_filled: i128,
) {
    let debt_token = token::Client::new(env, &auction.debt_asset);
    debt_token.transfer_from(
        &env.current_contract_address(),
        bidder,
        &auction.pool,
        &debt_repaid,
    );

    let collateral_token = token::Client::new(env, &auction.collateral_asset);
    collateral_token.transfer(&env.current_contract_address(), bidder, &collateral_filled);
}

#[contract]
pub struct LiquidationAuctionContract;

#[contractimpl]
impl LiquidationAuctionContract {
    pub fn initialize(env: Env, admin: Address, governance: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Governance, &governance);
        env.storage().instance().set(&DataKey::AuctionCount, &0u64);
    }

    pub fn start_auction(
        env: Env,
        user: Address,
        pool: Address,
        collateral_asset: Address,
        debt_asset: Address,
        collateral_amount: i128,
        debt_amount: i128,
        config: AuctionConfig,
    ) -> u64 {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        if config.starting_premium_bps <= config.floor_discount_bps {
            panic!("Invalid config: starting premium must be greater than floor discount");
        }
        if config.auction_duration == 0 {
            panic!("Invalid config: auction duration must be greater than zero");
        }
        if collateral_amount <= 0 || debt_amount <= 0 {
            panic!("Invalid config: collateral and debt must be positive");
        }

        let now = env.ledger().timestamp();
        let starting_price =
            debt_amount * (BPS_DIVISOR + config.starting_premium_bps) / BPS_DIVISOR;
        let floor_price = debt_amount * (BPS_DIVISOR + config.floor_discount_bps) / BPS_DIVISOR;

        let auction = Auction {
            id: 0,
            pool: pool.clone(),
            user,
            collateral_asset,
            debt_asset,
            collateral_amount,
            debt_amount,
            start_time: now,
            end_time: now + config.auction_duration,
            starting_price,
            floor_price,
            current_price: starting_price,
            remaining_collateral: collateral_amount,
            filled_amount: 0,
            state: AuctionState::Active,
            config: config.clone(),
            created_at: now,
        };

        let count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::AuctionCount)
            .unwrap_or(0);
        let auction_id = count + 1;

        let mut auction_with_id = auction;
        auction_with_id.id = auction_id;

        env.storage()
            .instance()
            .set(&DataKey::Auction(auction_id), &auction_with_id);
        env.storage()
            .instance()
            .set(&DataKey::AuctionCount, &auction_id);
        env.storage().instance().set(
            &DataKey::BidsReceived(auction_id),
            &Vec::<Address>::new(&env),
        );

        env.events().publish(
            ("auction_started", &pool),
            (
                auction_id,
                auction_with_id.collateral_amount,
                starting_price,
            ),
        );

        auction_id
    }

    pub fn commit_bid(env: Env, bidder: Address, auction_id: u64, commitment_hash: BytesN<32>) {
        bidder.require_auth();

        let auction: Auction = env
            .storage()
            .instance()
            .get(&DataKey::Auction(auction_id))
            .expect("Auction not found");

        if auction.state != AuctionState::Active && auction.state != AuctionState::PartiallyFilled {
            panic!("Auction not in active state");
        }

        let now = env.ledger().timestamp();
        let commit_end = auction.start_time + auction.config.commit_phase_duration;

        if now > commit_end {
            panic!("Commit phase has ended");
        }

        let commitment = BidCommitment {
            bidder: bidder.clone(),
            commitment_hash,
            auction_id,
            timestamp: now,
        };

        env.storage().persistent().set(
            &(DataKey::Commitments(auction_id), bidder.clone()),
            &commitment,
        );

        let mut bids: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::BidsReceived(auction_id))
            .unwrap_or(Vec::new(&env));
        if !bids.contains(&bidder) {
            bids.push_back(bidder.clone());
            env.storage()
                .instance()
                .set(&DataKey::BidsReceived(auction_id), &bids);
        }

        env.events()
            .publish(("bid_committed", &auction.user), (auction_id, &bidder));
    }

    pub fn reveal_bid(
        env: Env,
        bidder: Address,
        auction_id: u64,
        collateral_amount: i128,
        max_price: i128,
        nonce: BytesN<32>,
    ) {
        bidder.require_auth();

        let mut auction: Auction = env
            .storage()
            .instance()
            .get(&DataKey::Auction(auction_id))
            .expect("Auction not found");

        if auction.state != AuctionState::Active && auction.state != AuctionState::PartiallyFilled {
            panic!("Auction not in active state");
        }

        let now = env.ledger().timestamp();
        let commit_end = auction.start_time + auction.config.commit_phase_duration;
        let reveal_end = commit_end + auction.config.reveal_phase_duration;

        if now <= commit_end || now > reveal_end {
            panic!("Not in reveal phase");
        }

        let expected_hash = hash_bid(
            &bidder,
            auction_id,
            collateral_amount,
            max_price,
            &nonce,
            &env,
        );

        let commitment: BidCommitment = env
            .storage()
            .persistent()
            .get(&(DataKey::Commitments(auction_id), bidder.clone()))
            .expect("No commitment found");

        if commitment.commitment_hash != expected_hash {
            panic!("Invalid reveal: hash mismatch");
        }

        if collateral_amount <= 0 || collateral_amount > auction.remaining_collateral {
            panic!("Invalid bid: collateral amount out of range");
        }

        if collateral_amount < auction.config.min_bid_size {
            panic!("Invalid bid: below minimum bid size");
        }

        let current_price = compute_current_price(&auction.config, &auction, now);

        if max_price < current_price {
            panic!("Bid too low: max price below current auction price");
        }

        let fill_amount = if collateral_amount > auction.remaining_collateral {
            auction.remaining_collateral
        } else {
            collateral_amount
        };

        let debt_repaid = fill_amount * current_price / BPS_DIVISOR;

        settle_bid_payment(&env, &auction, &bidder, debt_repaid, fill_amount);

        auction.filled_amount += fill_amount;
        auction.remaining_collateral -= fill_amount;
        auction.current_price = current_price;

        if auction.remaining_collateral == 0 {
            auction.state = AuctionState::FullyFilled;
        } else if auction.filled_amount > 0 {
            auction.state = AuctionState::PartiallyFilled;
        }

        env.storage()
            .instance()
            .set(&DataKey::Auction(auction_id), &auction);

        env.storage()
            .persistent()
            .remove(&(DataKey::Commitments(auction_id), bidder.clone()));

        let result = BidResult {
            auction_id,
            bidder: bidder.clone(),
            collateral_filled: fill_amount,
            price_paid: current_price,
            debt_repaid,
        };

        env.events()
            .publish(("bid_revealed", &auction.user), &result);
    }

    pub fn place_bid(
        env: Env,
        bidder: Address,
        auction_id: u64,
        collateral_amount: i128,
    ) -> BidResult {
        bidder.require_auth();

        let mut auction: Auction = env
            .storage()
            .instance()
            .get(&DataKey::Auction(auction_id))
            .expect("Auction not found");

        if auction.state != AuctionState::Active && auction.state != AuctionState::PartiallyFilled {
            panic!("Auction not in active state");
        }

        let now = env.ledger().timestamp();
        if now > auction.end_time {
            auction.state = AuctionState::Settled;
            env.storage()
                .instance()
                .set(&DataKey::Auction(auction_id), &auction);
            panic!("Auction has expired");
        }

        if collateral_amount <= 0 || collateral_amount > auction.remaining_collateral {
            panic!("Invalid bid amount");
        }

        if collateral_amount < auction.config.min_bid_size {
            panic!("Invalid bid: below minimum bid size");
        }

        let current_price = compute_current_price(&auction.config, &auction, now);

        let fill_amount = if collateral_amount > auction.remaining_collateral {
            auction.remaining_collateral
        } else {
            collateral_amount
        };

        let debt_repaid = fill_amount * current_price / BPS_DIVISOR;

        settle_bid_payment(&env, &auction, &bidder, debt_repaid, fill_amount);

        auction.filled_amount += fill_amount;
        auction.remaining_collateral -= fill_amount;
        auction.current_price = current_price;

        if auction.remaining_collateral == 0 {
            auction.state = AuctionState::FullyFilled;
        } else if auction.filled_amount > 0 {
            auction.state = AuctionState::PartiallyFilled;
        }

        env.storage()
            .instance()
            .set(&DataKey::Auction(auction_id), &auction);

        let result = BidResult {
            auction_id,
            bidder: bidder.clone(),
            collateral_filled: fill_amount,
            price_paid: current_price,
            debt_repaid,
        };

        env.events().publish(("bid_placed", &auction.user), &result);

        result
    }

    pub fn settle_auction(env: Env, auction_id: u64) -> Auction {
        let mut auction: Auction = env
            .storage()
            .instance()
            .get(&DataKey::Auction(auction_id))
            .expect("Auction not found");

        let now = env.ledger().timestamp();

        if now < auction.end_time {
            panic!("Auction has not expired yet");
        }

        if auction.state == AuctionState::FullyFilled {
            auction.state = AuctionState::Settled;
        } else if auction.state == AuctionState::PartiallyFilled {
            auction.state = AuctionState::Settled;
        } else if auction.filled_amount == 0 {
            if auction.state == AuctionState::Active {
                auction.current_price = auction.floor_price;
            }
            auction.state = AuctionState::Settled;
        }

        env.storage()
            .instance()
            .set(&DataKey::Auction(auction_id), &auction);

        env.events().publish(
            ("auction_settled", &auction.user),
            (auction_id, auction.remaining_collateral),
        );

        auction
    }

    pub fn cancel_auction(env: Env, auction_id: u64, position_health_restored: bool) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let mut auction: Auction = env
            .storage()
            .instance()
            .get(&DataKey::Auction(auction_id))
            .expect("Auction not found");

        if auction.state != AuctionState::Active && auction.state != AuctionState::PartiallyFilled {
            panic!("Auction cannot be cancelled in current state");
        }

        if auction.filled_amount > 0 {
            panic!("Cannot cancel auction with filled bids; settle first");
        }

        auction.state = AuctionState::Cancelled;
        env.storage()
            .instance()
            .set(&DataKey::Auction(auction_id), &auction);

        let reason = if position_health_restored {
            "position_health_restored"
        } else {
            "cancelled_by_admin"
        };

        env.events()
            .publish(("auction_cancelled", &auction.user), (auction_id, reason));
    }

    pub fn get_auction(env: Env, auction_id: u64) -> Auction {
        let mut auction: Auction = env
            .storage()
            .instance()
            .get(&DataKey::Auction(auction_id))
            .expect("Auction not found");

        let now = env.ledger().timestamp();
        if auction.state == AuctionState::Active || auction.state == AuctionState::PartiallyFilled {
            auction.current_price = compute_current_price(&auction.config, &auction, now);
        }

        auction
    }

    pub fn get_auction_price(env: Env, auction_id: u64) -> i128 {
        let auction: Auction = env
            .storage()
            .instance()
            .get(&DataKey::Auction(auction_id))
            .expect("Auction not found");

        if auction.state == AuctionState::Active || auction.state == AuctionState::PartiallyFilled {
            let now = env.ledger().timestamp();
            compute_current_price(&auction.config, &auction, now)
        } else {
            auction.current_price
        }
    }

    pub fn get_bidder_count(env: Env, auction_id: u64) -> u32 {
        let bids: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::BidsReceived(auction_id))
            .unwrap_or(Vec::new(&env));
        bids.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};

    #[test]
    fn test_initialize() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let gov = Address::generate(&env);

        LiquidationAuctionContract::initialize(env.clone(), admin.clone(), gov);
        let stored: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        assert_eq!(stored, admin);
    }

    #[test]
    #[should_panic]
    fn test_initialize_requires_auth() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let gov = Address::generate(&env);

        LiquidationAuctionContract::initialize(env.clone(), admin, gov);
    }

    #[test]
    fn test_compute_current_price_linear() {
        let config = AuctionConfig {
            starting_premium_bps: 1000,
            floor_discount_bps: 500,
            auction_duration: 3600,
            price_decay_function: auction_types::PriceDecayFunction::Linear,
            min_bid_size: 100,
            commit_phase_duration: 600,
            reveal_phase_duration: 600,
        };
        let auction = Auction {
            id: 1,
            pool: Address::generate(&Env::default()),
            user: Address::generate(&Env::default()),
            collateral_asset: Address::generate(&Env::default()),
            debt_asset: Address::generate(&Env::default()),
            collateral_amount: 1000,
            debt_amount: 1000,
            start_time: 0,
            end_time: 3600,
            starting_price: 110000,
            floor_price: 105000,
            current_price: 110000,
            remaining_collateral: 1000,
            filled_amount: 0,
            state: AuctionState::Active,
            config: config.clone(),
            created_at: 0,
        };

        let price_start = compute_current_price(&config, &auction, 0);
        assert!(price_start >= auction.floor_price);

        let price_end = compute_current_price(&config, &auction, 3600);
        assert_eq!(price_end, auction.floor_price);
    }

    #[test]
    fn test_compute_current_price_exponential() {
        let config = AuctionConfig {
            starting_premium_bps: 1000,
            floor_discount_bps: 500,
            auction_duration: 3600,
            price_decay_function: auction_types::PriceDecayFunction::Exponential,
            min_bid_size: 100,
            commit_phase_duration: 600,
            reveal_phase_duration: 600,
        };
        let auction = Auction {
            id: 1,
            pool: Address::generate(&Env::default()),
            user: Address::generate(&Env::default()),
            collateral_asset: Address::generate(&Env::default()),
            debt_asset: Address::generate(&Env::default()),
            collateral_amount: 1000,
            debt_amount: 1000,
            start_time: 0,
            end_time: 3600,
            starting_price: 110000,
            floor_price: 105000,
            current_price: 110000,
            remaining_collateral: 1000,
            filled_amount: 0,
            state: AuctionState::Active,
            config: config.clone(),
            created_at: 0,
        };

        let price_end = compute_current_price(&config, &auction, 3600);
        assert_eq!(price_end, auction.floor_price);
    }

    fn test_config() -> AuctionConfig {
        AuctionConfig {
            starting_premium_bps: 1000,
            floor_discount_bps: 500,
            auction_duration: 3600,
            price_decay_function: auction_types::PriceDecayFunction::Linear,
            min_bid_size: 100,
            commit_phase_duration: 600,
            reveal_phase_duration: 600,
        }
    }

    struct AuctionTestSetup {
        env: Env,
        contract: Address,
        pool: Address,
        bidder: Address,
        collateral_asset: Address,
        debt_asset: Address,
        collateral_amount: i128,
        debt_amount: i128,
    }

    impl AuctionTestSetup {
        fn new() -> Self {
            let env = Env::default();
            env.mock_all_auths();

            let admin = Address::generate(&env);
            let gov = Address::generate(&env);
            let pool = Address::generate(&env);
            let bidder = Address::generate(&env);
            let collateral_token_admin = Address::generate(&env);
            let debt_token_admin = Address::generate(&env);

            let collateral_asset = env
                .register_stellar_asset_contract_v2(collateral_token_admin)
                .address();
            let debt_asset = env
                .register_stellar_asset_contract_v2(debt_token_admin)
                .address();
            let contract = env.register_contract(None, LiquidationAuctionContract);

            LiquidationAuctionContractClient::new(&env, &contract).initialize(&admin, &gov);

            AuctionTestSetup {
                env,
                contract,
                pool,
                bidder,
                collateral_asset,
                debt_asset,
                collateral_amount: 1000,
                debt_amount: 1000,
            }
        }

        fn client(&self) -> LiquidationAuctionContractClient<'_> {
            LiquidationAuctionContractClient::new(&self.env, &self.contract)
        }

        fn fund_collateral(&self, amount: i128) {
            token::StellarAssetClient::new(&self.env, &self.collateral_asset)
                .mint(&self.contract, &amount);
        }

        fn fund_bidder_debt(&self, amount: i128) {
            token::StellarAssetClient::new(&self.env, &self.debt_asset).mint(&self.bidder, &amount);
        }

        fn approve_bidder_debt(&self, amount: i128) {
            token::Client::new(&self.env, &self.debt_asset).approve(
                &self.bidder,
                &self.contract,
                &amount,
                &9999,
            );
        }

        fn start(&self) -> u64 {
            self.client().start_auction(
                &self.pool,
                &self.pool,
                &self.collateral_asset,
                &self.debt_asset,
                &self.collateral_amount,
                &self.debt_amount,
                &test_config(),
            )
        }

        fn stored_auction(&self, auction_id: u64) -> Auction {
            self.env.as_contract(&self.contract, || {
                self.env
                    .storage()
                    .instance()
                    .get(&DataKey::Auction(auction_id))
                    .unwrap()
            })
        }
    }

    fn expected_debt_repaid(env: &Env, auction: &Auction, collateral: i128) -> i128 {
        let now = env.ledger().timestamp();
        let current_price = compute_current_price(&auction.config, auction, now);
        collateral * current_price / BPS_DIVISOR
    }

    #[test]
    fn test_place_bid_settles_tokens() {
        let setup = AuctionTestSetup::new();
        setup.fund_collateral(1_000_000);
        setup.fund_bidder_debt(1_000_000);
        setup.approve_bidder_debt(1_000_000);

        let debt_token = token::Client::new(&setup.env, &setup.debt_asset);
        let collateral_token = token::Client::new(&setup.env, &setup.collateral_asset);

        let auction_id = setup.start();
        let debt_repaid = expected_debt_repaid(
            &setup.env,
            &setup.stored_auction(auction_id),
            setup.collateral_amount,
        );

        let result = setup
            .client()
            .place_bid(&setup.bidder, &auction_id, &setup.collateral_amount);

        assert_eq!(result.collateral_filled, setup.collateral_amount);
        assert_eq!(result.debt_repaid, debt_repaid);

        // Debt assets moved from the bidder to the lending pool.
        assert_eq!(debt_token.balance(&setup.bidder), 1_000_000 - debt_repaid);
        assert_eq!(debt_token.balance(&setup.pool), debt_repaid);

        // Collateral moved from the auction contract to the bidder.
        assert_eq!(
            collateral_token.balance(&setup.bidder),
            setup.collateral_amount
        );
        assert_eq!(
            collateral_token.balance(&setup.contract),
            1_000_000 - setup.collateral_amount
        );

        let stored = setup.stored_auction(auction_id);
        assert_eq!(stored.remaining_collateral, 0);
        assert_eq!(stored.state, AuctionState::FullyFilled);
    }

    #[test]
    fn test_reveal_bid_settles_tokens() {
        let setup = AuctionTestSetup::new();
        setup.fund_collateral(1_000_000);
        setup.fund_bidder_debt(1_000_000);
        setup.approve_bidder_debt(1_000_000);

        let debt_token = token::Client::new(&setup.env, &setup.debt_asset);
        let collateral_token = token::Client::new(&setup.env, &setup.collateral_asset);

        let auction_id = setup.start();

        let nonce = BytesN::from_array(&setup.env, &[7u8; 32]);
        let max_price = i128::MAX - 1;
        setup
            .client()
            .commit_bid(&setup.bidder, &auction_id, &nonce);
        setup
            .env
            .ledger()
            .set_timestamp(setup.env.ledger().timestamp() + 700);
        setup.client().reveal_bid(
            &setup.bidder,
            &auction_id,
            &setup.collateral_amount,
            &max_price,
            &nonce,
        );

        let debt_repaid = expected_debt_repaid(
            &setup.env,
            &setup.stored_auction(auction_id),
            setup.collateral_amount,
        );
        assert_eq!(debt_token.balance(&setup.bidder), 1_000_000 - debt_repaid);
        assert_eq!(debt_token.balance(&setup.pool), debt_repaid);
        assert_eq!(
            collateral_token.balance(&setup.bidder),
            setup.collateral_amount
        );
        assert_eq!(
            collateral_token.balance(&setup.contract),
            1_000_000 - setup.collateral_amount
        );

        let stored = setup.stored_auction(auction_id);
        assert_eq!(stored.remaining_collateral, 0);
        assert_eq!(stored.state, AuctionState::FullyFilled);
    }

    #[test]
    fn test_place_bid_zero_balance_bidder_fails_atomically() {
        let setup = AuctionTestSetup::new();
        setup.approve_bidder_debt(1_000_000);

        let debt_token = token::Client::new(&setup.env, &setup.debt_asset);
        assert_eq!(debt_token.balance(&setup.bidder), 0);

        let auction_id = setup.start();

        let result =
            setup
                .client()
                .try_place_bid(&setup.bidder, &auction_id, &setup.collateral_amount);
        assert!(
            result.is_err(),
            "zero-balance bidder must not win the auction"
        );

        let stored = setup.stored_auction(auction_id);
        assert_eq!(stored.remaining_collateral, setup.collateral_amount);
        assert_eq!(stored.filled_amount, 0);
        assert_eq!(stored.state, AuctionState::Active);
    }

    #[test]
    fn test_place_bid_insufficient_allowance_fails_atomically() {
        let setup = AuctionTestSetup::new();
        setup.fund_bidder_debt(1_000_000);

        let auction_id = setup.start();

        let result =
            setup
                .client()
                .try_place_bid(&setup.bidder, &auction_id, &setup.collateral_amount);
        assert!(
            result.is_err(),
            "unapproved bidder must not win the auction"
        );

        let stored = setup.stored_auction(auction_id);
        assert_eq!(stored.remaining_collateral, setup.collateral_amount);
        assert_eq!(stored.filled_amount, 0);
        assert_eq!(stored.state, AuctionState::Active);
    }
}
