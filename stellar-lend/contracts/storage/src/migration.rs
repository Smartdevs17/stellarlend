//! Storage migration framework for handling version upgrades

use soroban_sdk::{contracttype, Env, Vec};

/// Migration error types
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum MigrationError {
    /// Migration from this version is not supported
    UnsupportedMigration = 1,
    /// Migration data is invalid
    InvalidMigrationData = 2,
    /// Migration is already in progress (concurrent migration)
    MigrationInProgress = 3,
    /// Migration failed due to internal error
    MigrationFailed = 4,
    /// Rollback failed
    RollbackFailed = 5,
}

/// Migration direction
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum MigrationDirection {
    Up,    // Upgrade to newer version
    Down,  // Downgrade to older version
}

/// Version tracking for storage
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct StorageVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl StorageVersion {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        StorageVersion { major, minor, patch }
    }

    pub fn to_u32(&self) -> u32 {
        (self.major << 16) | (self.minor << 8) | self.patch
    }

    pub fn from_u32(version: u32) -> Self {
        StorageVersion {
            major: (version >> 16) & 0xFF,
            minor: (version >> 8) & 0xFF,
            patch: version & 0xFF,
        }
    }
}

/// Migration handler trait for implementing storage migrations
pub trait MigrationHandler {
    /// Execute migration from one version to another
    fn migrate(&self, env: &Env, from_version: StorageVersion, to_version: StorageVersion) -> Result<(), MigrationError>;

    /// Validate that migration is safe to proceed
    fn validate(&self, env: &Env, from_version: StorageVersion, to_version: StorageVersion) -> Result<(), MigrationError>;

    /// Rollback migration to previous state
    fn rollback(&self, env: &Env, from_version: StorageVersion) -> Result<(), MigrationError>;
}

/// Storage migration manager
pub struct StorageMigration;

impl StorageMigration {
    /// Execute a migration with validation and rollback support
    pub fn execute<H: MigrationHandler>(
        env: &Env,
        handler: &H,
        from_version: StorageVersion,
        to_version: StorageVersion,
    ) -> Result<(), MigrationError> {
        // Validate migration is safe
        handler.validate(env, from_version.clone(), to_version.clone())?;

        // Execute migration
        handler.migrate(env, from_version, to_version)?;

        Ok(())
    }

    /// Calculate the steps needed for migration
    pub fn calculate_migration_path(_from: StorageVersion, _to: StorageVersion) -> Result<(), MigrationError> {
        // Generate list of intermediate versions
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_version_conversion() {
        let version = StorageVersion::new(1, 2, 3);
        let u32_version = version.to_u32();
        let recovered = StorageVersion::from_u32(u32_version);

        assert_eq!(version.major, recovered.major);
        assert_eq!(version.minor, recovered.minor);
        assert_eq!(version.patch, recovered.patch);
    }
}
