#![no_std]

use soroban_sdk::{contract, contractimpl, log, symbol_short, Address, Env, String, Symbol};

mod types;

#[cfg(test)]
mod test;

use types::{SNSAnalytics, SNSCache, SNSConfig, SNSError, SNSRecord};

#[contract]
pub struct SNSIntegration;

#[contractimpl]
impl SNSIntegration {
    pub fn initialize(
        env: Env,
        admin: Address,
        cache_ttl_seconds: u64,
        name_expiry_days: u64,
    ) -> Result<(), SNSError> {
        if env.storage().instance().has(&symbol_short!("config")) {
            return Err(SNSError::AlreadyInitialized);
        }
        admin.require_auth();

        let config = SNSConfig {
            admin,
            cache_ttl_seconds,
            name_expiry_days,
        };
        env.storage().instance().set(&symbol_short!("config"), &config);

        let analytics = SNSAnalytics {
            total_names_registered: 0,
            total_resolutions: 0,
            cache_hit_rate: 0,
            resolution_latency_ms: 0,
        };
        env.storage()
            .instance()
            .set(&symbol_short!("analytics"), &analytics);

        Ok(())
    }

    /// Register or update a SNS name.
    pub fn register_name(
        env: Env,
        name: String,
        address: Address,
    ) -> Result<(), SNSError> {
        address.require_auth();
        Self::require_initialized(&env)?;

        if name.len() == 0 {
            return Err(SNSError::InvalidName);
        }

        let config: SNSConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("config"))
            .ok_or(SNSError::NotInitialized)?;

        let expires_at = env.ledger().timestamp() + (config.name_expiry_days * 86400);

        let record = SNSRecord {
            name: name.clone(),
            address: address.clone(),
            registered_at: env.ledger().timestamp(),
            expires_at,
            owner: address.clone(),
        };

        let name_key = Self::name_key(&name);
        env.storage().persistent().set(&name_key, &record);

        // Invalidate cache for this name
        env.storage()
            .persistent()
            .remove(&Self::cache_key(&name));

        // Update analytics
        let mut analytics: SNSAnalytics = env
            .storage()
            .instance()
            .get(&symbol_short!("analytics"))
            .unwrap();
        analytics.total_names_registered += 1;
        env.storage()
            .instance()
            .set(&symbol_short!("analytics"), &analytics);

        log!(&env, "SNS name registered: {} -> {}", name, address);

        Ok(())
    }

    /// Resolve a SNS name to an address with caching.
    pub fn resolve_name(env: Env, name: String) -> Result<Address, SNSError> {
        Self::require_initialized(&env)?;

        // Check cache first
        if let Some(cached) = Self::get_from_cache(&env, &name) {
            // Update analytics with cache hit
            let mut analytics: SNSAnalytics = env
                .storage()
                .instance()
                .get(&symbol_short!("analytics"))
                .unwrap();
            analytics.total_resolutions += 1;
            // Cache hit rate calculation (simplified)
            if analytics.cache_hit_rate < 100 {
                analytics.cache_hit_rate += 1;
            }
            env.storage()
                .instance()
                .set(&symbol_short!("analytics"), &analytics);

            return Ok(cached.address);
        }

        // Fetch from persistent storage
        let name_key = Self::name_key(&name);
        let record: SNSRecord = env
            .storage()
            .persistent()
            .get(&name_key)
            .ok_or(SNSError::NameNotFound)?;

        if record.expires_at < env.ledger().timestamp() {
            return Err(SNSError::NameExpired);
        }

        // Cache the resolution
        let config: SNSConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("config"))
            .ok_or(SNSError::NotInitialized)?;

        let cache = SNSCache {
            name: name.clone(),
            address: record.address.clone(),
            cached_at: env.ledger().timestamp(),
            ttl: config.cache_ttl_seconds,
        };

        let cache_key = Self::cache_key(&name);
        env.storage().persistent().set(&cache_key, &cache);

        // Update analytics
        let mut analytics: SNSAnalytics = env
            .storage()
            .instance()
            .get(&symbol_short!("analytics"))
            .unwrap();
        analytics.total_resolutions += 1;
        env.storage()
            .instance()
            .set(&symbol_short!("analytics"), &analytics);

        log!(&env, "SNS name resolved: {} -> {}", name, record.address);

        Ok(record.address)
    }

    /// Bulk resolve multiple names.
    pub fn resolve_names_batch(
        env: Env,
        names: soroban_sdk::Vec<String>,
    ) -> soroban_sdk::Vec<Result<Address, SNSError>> {
        let mut results = soroban_sdk::Vec::new();

        for name in names.iter() {
            let result = Self::resolve_name(env.clone(), name);
            results.push_back(result);
        }

        results
    }

    /// Validate that a name resolves before transaction.
    pub fn validate_name(env: Env, name: String) -> Result<Address, SNSError> {
        Self::resolve_name(env, name)
    }

    /// Get SNS analytics: top resolved names, cache hit rate, latency.
    pub fn get_analytics(env: Env) -> Result<SNSAnalytics, SNSError> {
        Self::require_initialized(&env)?;

        env.storage()
            .instance()
            .get(&symbol_short!("analytics"))
            .ok_or(SNSError::NotInitialized)
    }

    /// Check if a name is expired.
    pub fn is_name_expired(env: Env, name: String) -> Result<bool, SNSError> {
        let name_key = Self::name_key(&name);
        let record: SNSRecord = env
            .storage()
            .persistent()
            .get(&name_key)
            .ok_or(SNSError::NameNotFound)?;

        Ok(record.expires_at < env.ledger().timestamp())
    }

    /// Renew a name registration.
    pub fn renew_name(env: Env, name: String) -> Result<(), SNSError> {
        let name_key = Self::name_key(&name);
        let mut record: SNSRecord = env
            .storage()
            .persistent()
            .get(&name_key)
            .ok_or(SNSError::NameNotFound)?;

        record.owner.require_auth();

        let config: SNSConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("config"))
            .ok_or(SNSError::NotInitialized)?;

        record.expires_at = env.ledger().timestamp() + (config.name_expiry_days * 86400);

        env.storage().persistent().set(&name_key, &record);

        log!(&env, "SNS name renewed: {}", name);

        Ok(())
    }

    /// Helper: Get SNS name key.
    fn name_key(name: &String) -> Symbol {
        symbol_short!("snsname")
    }

    /// Helper: Get cache key.
    fn cache_key(name: &String) -> Symbol {
        symbol_short!("snscache")
    }

    /// Helper: Get from cache if not expired.
    fn get_from_cache(env: &Env, name: &String) -> Option<SNSCache> {
        let cache_key = Self::cache_key(name);
        let cache: SNSCache = env.storage().persistent().get(&cache_key)?;

        if env.ledger().timestamp() < cache.cached_at + cache.ttl {
            return Some(cache);
        }

        env.storage().persistent().remove(&cache_key);
        None
    }

    /// Helper: Require initialized.
    fn require_initialized(env: &Env) -> Result<(), SNSError> {
        if !env.storage().instance().has(&symbol_short!("config")) {
            return Err(SNSError::NotInitialized);
        }
        Ok(())
    }
}
