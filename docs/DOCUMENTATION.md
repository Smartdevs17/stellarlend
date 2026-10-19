# StellarLend Documentation Index

Welcome to the comprehensive StellarLend documentation. This index guides you to all available resources organized by use case and audience.

## For Users

**Getting Started**
- [User Guide](./USER_GUIDE.md) - Complete guide to using StellarLend
  - How to deposit collateral
  - How to borrow assets
  - Position monitoring
  - Risk management
  - Security best practices

**Key Concepts**
- [Risk Monitoring Dashboard](./RISK_MONITORING_DASHBOARD.md) - Understand and manage your position risk
- [Governance Guide](./GOVERNANCE.md) - Participate in protocol governance
- [Protocol Health Score](./PROTOCOL_HEALTH_SCORE.md) - Understand protocol metrics

## For Developers

**Getting Started**
- [Developer Guide](./DEVELOPER_GUIDE.md) - Setup and development workflow
  - Project structure
  - Development environment
  - Testing guidelines
  - Deployment process

**API Documentation**
- [API Reference](./API.md) - Complete API documentation
  - Authentication methods
  - All endpoints
  - Request/response formats
  - Error handling
  - Rate limiting

**Code Examples**
- [JavaScript/TypeScript Examples](./examples/javascript.md)
  - Depositing collateral
  - Borrowing assets
  - Checking positions
  - Multi-step transactions
  - Error handling
  
- [Python Examples](./examples/python.md)
  - Python client setup
  - Common operations
  - Stellar signature authentication
  - Complete client implementation

**Smart Contracts**
- [Soroban Contracts Overview](../stellar-lend/README.md) - Smart contract architecture
- [Storage System](./storage.md) - Contract storage design
- [Event Schema](./event-schema.md) - Event logging specifications

## Architecture & Design

**Core Systems**
- [Event Indexing](./event-indexing.md) - Event indexing and querying
- [Upgrade Mechanism](./upgrade-mechanism.md) - Contract upgrade process
- [Oracle Configuration Guide](./ORACLE_CONFIGURATION_GUIDE.md) - Price feed setup
- [Oracle Stress Testing](./ORACLE_STRESS_TESTING.md) - Oracle reliability testing

**Advanced Features**
- [Flash Loans](./FLASH_LOAN.md) - Uncollateralized borrowing
- [AMM Integration](./AMM_INTEGRATION.md) - Automated market maker hooks
- [Cross-Chain Bridge](./BRIDGE.md) - Cross-chain asset transfers
- [Yield Aggregator](./YIELD_AGGREGATOR_ROUTING.md) - Yield optimization

**Risk Management**
- [Liquidation Strategy](./LIQUIDATION_STRATEGY.md) - Liquidation mechanics
- [Parameter Store](./PARAMETER_STORE.md) - Protocol parameters
- [Risk Engine](./RISK_ENGINE.md) - Risk assessment

## Security

**Security Documentation**
- [Security Policy](../SECURITY.md) - Security disclosure policy
- [Audit Logging](./audit-logging.md) - Audit trail system
- [Formal Verification](./FORMAL_VERIFICATION.md) - Mathematical proofs
- [Security Audit](./security-audit.md) - Audit findings and remediation

**Testing & Verification**
- [Fuzzing Pipeline](./fuzzing-pipeline.md) - Fuzz testing setup
- [Testing Guide](./INITIALIZATION_TESTS.md) - Test frameworks and examples
- [Cross-Contract Test Scenarios](./CROSS_CONTRACT_TEST_SCENARIOS.md) - Integration testing

## Operations

**Deployment & Administration**
- [Deployment Guide](./DEPLOYMENT.md) - Production deployment
- [Dev Tooling](./DEV_TOOLING.md) - Development tools setup
- [Emergency Procedures](./EMERGENCY_WITHDRAWAL.md) - Emergency operations
- [Admin Guide](./admin.md) - Administrative tasks

**Monitoring & Maintenance**
- [MEV Protection](./mev-protection.md) - MEV attack mitigation
- [Gas Optimization](./gas-optimization.md) - Gas efficiency guide
- [Gas Benchmarks](./gas-benchmarks.md) - Performance benchmarks
- [Database Indexing Optimization](./DATABASE_INDEXING_OPTIMIZATION.md) - Query performance

## Financial Features

**Credit & Lending**
- [Credit Delegation](./CREDIT_DELEGATION.md) - Credit line management
- [Borrow Tests](./BORROW_TESTS.md) - Borrowing test scenarios
- [Cross-Asset Rules](./CROSS_ASSET_RULES.md) - Multi-asset handling

**Staking & Rewards**
- [Referral Tracking and Rewards](./REFERRAL_TRACKING_AND_REWARDS.md) - Referral system
- [Fee Tier Loyalty System](./FEE_TIER_LOYALTY_SYSTEM.md) - Fee tiers and discounts
- [Yield Optimization](./YIELD_AGGREGATOR_ROUTING.md) - Yield strategies

**Advanced Analytics**
- [Transaction Simulation Cache](./TRANSACTION_SIMULATION_CACHE.md) - Simulation caching
- [Position Health Simulation](./POSITION_HEALTH_SIMULATION.md) - Position forecasting
- [Pool Performance Tracking](./POOL_PERFORMANCE_TRACKING.md) - Pool analytics

## Reference

**Technical Reference**
- [Math Library](./MATH_LIBRARY.md) - Mathematical functions
- [Arithmetic Safety](./ARITHMETIC_SAFETY.md) - Safe math operations
- [Gas Estimation System](./GAS_ESTIMATION_SYSTEM.md) - Gas prediction
- [TWAP Oracle Security](./twap-oracle-security.md) - Time-weighted price feeds

**Data & Schemas**
- [Event Schema](./event-schema.md) - Event definitions
- [Zero Amount Semantics](./ZERO_AMOUNT_SEMANTICS.md) - Handling zero amounts
- [Real-time Price Feed](./REALTIME_PRICE_FEED.md) - Price feed mechanics

**Governance & Recovery**
- [Multisig Governance](./multisig.md) - Multi-signature operations
- [Social Recovery](./recovery.md) - Account recovery mechanisms
- [Migration Guide](./migration-guide.md) - Data migration procedures

## Common Tasks

### I want to...

**...deposit and borrow**
→ Start with [User Guide](./USER_GUIDE.md)

**...develop using the API**
→ Check [Developer Guide](./DEVELOPER_GUIDE.md) and [API Reference](./API.md)

**...integrate StellarLend into my app**
→ See [JavaScript](./examples/javascript.md) or [Python](./examples/python.md) examples

**...set up a validator/node**
→ Read [Deployment Guide](./DEPLOYMENT.md)

**...understand contract architecture**
→ Review [Soroban Overview](../stellar-lend/README.md) and [Storage](./storage.md)

**...participate in governance**
→ Follow [Governance Guide](./GOVERNANCE.md)

**...report a security issue**
→ See [Security Policy](../SECURITY.md)

**...optimize performance**
→ Check [Gas Optimization](./gas-optimization.md) and [Database Indexing](./DATABASE_INDEXING_OPTIMIZATION.md)

## Community & Support

- **GitHub**: https://github.com/stellar/stellarlend
- **Discord**: https://discord.gg/stellarlend
- **Email Support**: support@stellarlend.io
- **Bug Bounty**: https://stellarlend.io/security

## Document Updates

Last updated: 2025-09-24

Documentation is maintained by the StellarLend community. To contribute updates or corrections, please submit a pull request on GitHub.

## Changelog

- **2025-09-24**: Added comprehensive API documentation, user guide, developer guide, and code examples
- See [CHANGELOG.md](../CHANGELOG.md) for full version history
