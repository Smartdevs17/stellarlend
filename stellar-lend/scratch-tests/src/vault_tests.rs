mod tests {
    use hello_world::errors::LendingError;
    use hello_world::vault::{
        accrue_yield, approve_shares, convert_to_assets, convert_to_shares, deposit,
        get_vault_config, get_vault_stats, initialize_vault, max_deposit, max_mint, max_redeem,
        max_withdraw, mint, preview_deposit, preview_mint, preview_redeem, preview_withdraw,
        redeem, set_vault_config, share_allowance, share_balance_of, total_assets, total_shares,
        transfer_shares, withdraw, ERC4626Vault, StablecoinVault, VaultConfig,
    };
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};

    use crate::harness::Ctx;

    fn new_contract(env: &Env) -> Address {
        env.register(Ctx, ())
    }

    fn make_admin(env: &Env, contract: &Address) -> Address {
        env.as_contract(contract, || {
            let admin = Address::generate(env);
            env.storage()
                .persistent()
                .set(&hello_world::admin::AdminDataKey::Admin, &admin);
            admin
        })
    }

    fn default_config() -> VaultConfig {
        VaultConfig {
            deposit_cap: 1_000_000,
            min_deposit: 10,
            deposit_paused: false,
            withdraw_paused: false,
        }
    }

    #[test]
    fn test_vault_initialize_and_get_config() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = new_contract(&env);
        let admin = make_admin(&env, &contract);
        let asset = Address::generate(&env);

        let config = default_config();

        // Initialize vault for default pool (None)
        env.as_contract(&contract, || {
            initialize_vault(&env, &admin, None, asset.clone(), config.clone()).unwrap();
        });

        // Verify config
        env.as_contract(&contract, || {
            let fetched = get_vault_config(&env, None).unwrap();
            assert_eq!(fetched, config);
            assert_eq!(total_assets(&env, None).unwrap(), 0);
            assert_eq!(total_shares(&env, None).unwrap(), 0);
        });

        // Re-initialization should fail with AlreadyInitialized
        env.as_contract(&contract, || {
            let res = initialize_vault(&env, &admin, None, asset.clone(), config);
            assert_eq!(res, Err(LendingError::AlreadyInitialized));
        });

        // Non-admin initialization should fail with Unauthorized
        let non_admin = Address::generate(&env);
        env.as_contract(&contract, || {
            let res = initialize_vault(
                &env,
                &non_admin,
                Some(Address::generate(&env)),
                asset,
                default_config(),
            );
            assert_eq!(res, Err(LendingError::Unauthorized));
        });
    }

    #[test]
    fn test_vault_config_updates() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = new_contract(&env);
        let admin = make_admin(&env, &contract);
        let asset = Address::generate(&env);

        env.as_contract(&contract, || {
            initialize_vault(&env, &admin, None, asset, default_config()).unwrap();
        });

        let updated_config = VaultConfig {
            deposit_cap: 2_000_000,
            min_deposit: 50,
            deposit_paused: true,
            withdraw_paused: true,
        };

        // Admin updates config
        env.as_contract(&contract, || {
            set_vault_config(&env, &admin, None, updated_config.clone()).unwrap();
            let fetched = get_vault_config(&env, None).unwrap();
            assert_eq!(fetched, updated_config);
        });

        // Non-admin fails
        let non_admin = Address::generate(&env);
        env.as_contract(&contract, || {
            let res = set_vault_config(&env, &non_admin, None, default_config());
            assert_eq!(res, Err(LendingError::Unauthorized));
        });

        // Updating uninitialized pool fails
        let uninit_pool = Some(Address::generate(&env));
        env.as_contract(&contract, || {
            let res = set_vault_config(&env, &admin, uninit_pool, default_config());
            assert_eq!(res, Err(LendingError::NotInitialized));
        });
    }

    #[test]
    fn test_deposit_and_convert_math() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = new_contract(&env);
        let admin = make_admin(&env, &contract);
        let asset = Address::generate(&env);
        let user = Address::generate(&env);

        env.as_contract(&contract, || {
            initialize_vault(&env, &admin, None, asset, default_config()).unwrap();
        });

        // First deposit: 100 assets -> 100 shares
        env.as_contract(&contract, || {
            let preview = preview_deposit(&env, None, 100).unwrap();
            assert_eq!(preview, 100);

            let shares = deposit(&env, &user, 100, &user, None).unwrap();
            assert_eq!(shares, 100);
            assert_eq!(total_assets(&env, None).unwrap(), 100);
            assert_eq!(total_shares(&env, None).unwrap(), 100);
            assert_eq!(share_balance_of(&env, None, &user), 100);

            // Check conversions
            assert_eq!(convert_to_shares(&env, None, 50).unwrap(), 50);
            assert_eq!(convert_to_assets(&env, None, 50).unwrap(), 50);
        });
    }

    #[test]
    fn test_accrue_yield_and_share_appreciation() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = new_contract(&env);
        let admin = make_admin(&env, &contract);
        let asset = Address::generate(&env);
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);

        env.as_contract(&contract, || {
            initialize_vault(&env, &admin, None, asset, default_config()).unwrap();
        });

        // User1 deposits 100 assets -> 100 shares
        env.as_contract(&contract, || {
            deposit(&env, &user1, 100, &user1, None).unwrap();
        });

        // Yield of 100 accrued into vault: total_assets = 200, total_shares = 100 (each share worth ~2 assets)
        env.as_contract(&contract, || {
            accrue_yield(&env, &admin, None, 100).unwrap();
            assert_eq!(total_assets(&env, None).unwrap(), 200);
            assert_eq!(total_shares(&env, None).unwrap(), 100);
        });

        // User2 deposits 200 assets: should get ~100 shares
        env.as_contract(&contract, || {
            let expected_shares = preview_deposit(&env, None, 200).unwrap();
            let shares = deposit(&env, &user2, 200, &user2, None).unwrap();
            assert_eq!(shares, expected_shares);
            assert_eq!(share_balance_of(&env, None, &user2), shares);

            // User1's 100 shares are now redeemable for ~200 assets
            let user1_assets = preview_redeem(&env, None, 100).unwrap();
            assert!(user1_assets >= 199 && user1_assets <= 201);
        });
    }

    #[test]
    fn test_mint_exact_shares() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = new_contract(&env);
        let admin = make_admin(&env, &contract);
        let asset = Address::generate(&env);
        let user = Address::generate(&env);

        env.as_contract(&contract, || {
            initialize_vault(&env, &admin, None, asset, default_config()).unwrap();
        });

        env.as_contract(&contract, || {
            let required_assets = preview_mint(&env, None, 50).unwrap();
            let assets = mint(&env, &user, 50, &user, None).unwrap();
            assert_eq!(assets, required_assets);
            assert_eq!(share_balance_of(&env, None, &user), 50);
            assert_eq!(total_shares(&env, None).unwrap(), 50);
        });
    }

    #[test]
    fn test_withdraw_and_redeem() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = new_contract(&env);
        let admin = make_admin(&env, &contract);
        let asset = Address::generate(&env);
        let user = Address::generate(&env);
        let receiver = Address::generate(&env);

        env.as_contract(&contract, || {
            initialize_vault(&env, &admin, None, asset, default_config()).unwrap();
        });

        // Deposit 1,000 assets -> 1,000 shares
        env.as_contract(&contract, || {
            deposit(&env, &user, 1_000, &user, None).unwrap();
        });

        // Withdraw 400 assets to receiver
        env.as_contract(&contract, || {
            let burned_shares = preview_withdraw(&env, None, 400).unwrap();
            let shares = withdraw(&env, &user, 400, &receiver, &user, None).unwrap();
            assert_eq!(shares, burned_shares);
            assert_eq!(share_balance_of(&env, None, &user), 1_000 - shares);
            assert_eq!(total_assets(&env, None).unwrap(), 600);
        });

        // Redeem remaining shares
        env.as_contract(&contract, || {
            let remaining = share_balance_of(&env, None, &user);
            let expected_assets = preview_redeem(&env, None, remaining).unwrap();
            let assets = redeem(&env, &user, remaining, &receiver, &user, None).unwrap();
            assert_eq!(assets, expected_assets);
            assert_eq!(share_balance_of(&env, None, &user), 0);
            assert_eq!(total_shares(&env, None).unwrap(), 0);
        });
    }

    #[test]
    fn test_third_party_allowance_withdraw_and_redeem() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = new_contract(&env);
        let admin = make_admin(&env, &contract);
        let asset = Address::generate(&env);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);
        let receiver = Address::generate(&env);

        env.as_contract(&contract, || {
            initialize_vault(&env, &admin, None, asset, default_config()).unwrap();
            deposit(&env, &owner, 1_000, &owner, None).unwrap();
        });

        // Approve spender for 500 shares
        env.as_contract(&contract, || {
            approve_shares(&env, &owner, &spender, None, 500).unwrap();
            assert_eq!(share_allowance(&env, None, &owner, &spender), 500);
        });

        // Spender withdraws 200 assets on behalf of owner
        env.as_contract(&contract, || {
            let shares = withdraw(&env, &spender, 200, &receiver, &owner, None).unwrap();
            assert_eq!(share_allowance(&env, None, &owner, &spender), 500 - shares);
        });

        // Spender redeems more than remaining allowance fails with Unauthorized
        env.as_contract(&contract, || {
            let res = redeem(&env, &spender, 400, &receiver, &owner, None);
            assert_eq!(res, Err(LendingError::Unauthorized));
        });
    }

    #[test]
    fn test_share_transfer() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = new_contract(&env);
        let admin = make_admin(&env, &contract);
        let asset = Address::generate(&env);
        let user_a = Address::generate(&env);
        let user_b = Address::generate(&env);

        env.as_contract(&contract, || {
            initialize_vault(&env, &admin, None, asset, default_config()).unwrap();
            deposit(&env, &user_a, 500, &user_a, None).unwrap();
        });

        // Transfer 200 shares from A to B
        env.as_contract(&contract, || {
            transfer_shares(&env, &user_a, &user_b, None, 200).unwrap();
            assert_eq!(share_balance_of(&env, None, &user_a), 300);
            assert_eq!(share_balance_of(&env, None, &user_b), 200);
        });

        // Over-transfer fails
        env.as_contract(&contract, || {
            let res = transfer_shares(&env, &user_a, &user_b, None, 400);
            assert_eq!(res, Err(LendingError::InsufficientBalance));
        });
    }

    #[test]
    fn test_failure_modes_paused() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = new_contract(&env);
        let admin = make_admin(&env, &contract);
        let asset = Address::generate(&env);
        let user = Address::generate(&env);

        let mut config = default_config();
        config.deposit_paused = true;
        config.withdraw_paused = true;

        env.as_contract(&contract, || {
            initialize_vault(&env, &admin, None, asset, config).unwrap();
        });

        env.as_contract(&contract, || {
            assert_eq!(max_deposit(&env, None, &user).unwrap(), 0);
            assert_eq!(max_mint(&env, None, &user).unwrap(), 0);
            assert_eq!(max_withdraw(&env, None, &user).unwrap(), 0);
            assert_eq!(max_redeem(&env, None, &user).unwrap(), 0);

            let dep_res = deposit(&env, &user, 100, &user, None);
            assert_eq!(dep_res, Err(LendingError::ProtocolPaused));

            let mint_res = mint(&env, &user, 100, &user, None);
            assert_eq!(mint_res, Err(LendingError::ProtocolPaused));

            let wd_res = withdraw(&env, &user, 100, &user, &user, None);
            assert_eq!(wd_res, Err(LendingError::ProtocolPaused));

            let red_res = redeem(&env, &user, 100, &user, &user, None);
            assert_eq!(red_res, Err(LendingError::ProtocolPaused));
        });
    }

    #[test]
    fn test_failure_modes_limits_and_invalid_amounts() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = new_contract(&env);
        let admin = make_admin(&env, &contract);
        let asset = Address::generate(&env);
        let user = Address::generate(&env);

        let config = VaultConfig {
            deposit_cap: 500,
            min_deposit: 50,
            deposit_paused: false,
            withdraw_paused: false,
        };

        env.as_contract(&contract, || {
            initialize_vault(&env, &admin, None, asset, config).unwrap();
        });

        env.as_contract(&contract, || {
            // Zero or negative amounts
            assert_eq!(
                deposit(&env, &user, 0, &user, None),
                Err(LendingError::InvalidAmount)
            );
            assert_eq!(
                mint(&env, &user, 0, &user, None),
                Err(LendingError::InvalidAmount)
            );
            assert_eq!(
                withdraw(&env, &user, 0, &user, &user, None),
                Err(LendingError::InvalidAmount)
            );
            assert_eq!(
                redeem(&env, &user, 0, &user, &user, None),
                Err(LendingError::InvalidAmount)
            );

            // Below minimum deposit
            assert_eq!(
                deposit(&env, &user, 40, &user, None),
                Err(LendingError::InvalidAmount)
            );

            // Exceeding deposit cap
            assert_eq!(
                deposit(&env, &user, 600, &user, None),
                Err(LendingError::LimitExceeded)
            );

            // Valid deposit
            deposit(&env, &user, 300, &user, None).unwrap();

            // Insufficient balance on withdraw
            assert_eq!(
                withdraw(&env, &user, 500, &user, &user, None),
                Err(LendingError::InsufficientLiquidity)
            );
        });
    }

    #[test]
    fn test_rounding_direction_erc4626() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = new_contract(&env);
        let admin = make_admin(&env, &contract);
        let asset = Address::generate(&env);
        let user = Address::generate(&env);

        env.as_contract(&contract, || {
            initialize_vault(&env, &admin, None, asset, default_config()).unwrap();
            // Initial deposit 100 assets -> 100 shares
            deposit(&env, &user, 100, &user, None).unwrap();
            // Accrue yield of 50 -> total_assets = 150, total_shares = 100
            accrue_yield(&env, &admin, None, 50).unwrap();
        });

        env.as_contract(&contract, || {
            // In an asymmetrical vault:
            // preview_deposit should round down
            let shares = preview_deposit(&env, None, 10).unwrap();
            // preview_mint should round up (in favor of vault)
            let assets_needed = preview_mint(&env, None, shares).unwrap();
            assert!(assets_needed >= 10);

            // preview_withdraw should round up (in favor of vault)
            let shares_to_burn = preview_withdraw(&env, None, 10).unwrap();
            // preview_redeem should round down
            let assets_got = preview_redeem(&env, None, shares_to_burn).unwrap();
            assert!(assets_got >= 10);
        });
    }

    #[test]
    fn test_multi_pool_isolation() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = new_contract(&env);
        let admin = make_admin(&env, &contract);
        let usdc_pool = Some(Address::generate(&env));
        let eurc_pool = Some(Address::generate(&env));
        let usdc_asset = Address::generate(&env);
        let eurc_asset = Address::generate(&env);
        let user = Address::generate(&env);

        env.as_contract(&contract, || {
            initialize_vault(
                &env,
                &admin,
                usdc_pool.clone(),
                usdc_asset,
                default_config(),
            )
            .unwrap();
            initialize_vault(
                &env,
                &admin,
                eurc_pool.clone(),
                eurc_asset,
                default_config(),
            )
            .unwrap();
        });

        env.as_contract(&contract, || {
            deposit(&env, &user, 500, &user, usdc_pool.clone()).unwrap();
            deposit(&env, &user, 200, &user, eurc_pool.clone()).unwrap();

            assert_eq!(total_assets(&env, usdc_pool.clone()).unwrap(), 500);
            assert_eq!(total_assets(&env, eurc_pool.clone()).unwrap(), 200);
            assert_eq!(share_balance_of(&env, usdc_pool.clone(), &user), 500);
            assert_eq!(share_balance_of(&env, eurc_pool.clone(), &user), 200);

            let usdc_stats = get_vault_stats(&env, usdc_pool).unwrap();
            let eurc_stats = get_vault_stats(&env, eurc_pool).unwrap();
            assert_eq!(usdc_stats.total_assets, 500);
            assert_eq!(eurc_stats.total_assets, 200);
        });
    }

    #[test]
    fn test_vault_trait_implementation() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = new_contract(&env);
        let admin = make_admin(&env, &contract);
        let asset = Address::generate(&env);
        let user = Address::generate(&env);

        env.as_contract(&contract, || {
            initialize_vault(&env, &admin, None, asset.clone(), default_config()).unwrap();
        });

        env.as_contract(&contract, || {
            let vault = StablecoinVault;
            assert_eq!(vault.asset(&env, None).unwrap(), asset);
            assert_eq!(vault.total_assets(&env, None).unwrap(), 0);
            assert_eq!(vault.total_shares(&env, None).unwrap(), 0);

            let shares = vault.deposit(&env, &user, 250, &user, None).unwrap();
            assert_eq!(shares, 250);
            assert_eq!(vault.total_assets(&env, None).unwrap(), 250);
            assert_eq!(vault.total_shares(&env, None).unwrap(), 250);

            let withdrawn_shares = vault.withdraw(&env, &user, 100, &user, &user, None).unwrap();
            assert_eq!(withdrawn_shares, 100);
            assert_eq!(vault.total_assets(&env, None).unwrap(), 150);
        });
    }
}
