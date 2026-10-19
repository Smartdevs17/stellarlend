# Governance Proposal Templates (#1451)

Quick-start templates for creating governance proposals.

## Templates Available

### 1. Parameter Update Proposal
For proposing changes to protocol parameters.

```bash
cp templates/parameter_update_proposal.toml my-proposal.toml
# Edit and customize my-proposal.toml
cargo run --bin governance-cli submit my-proposal.toml
```

**Use cases:**
- Adjust interest rates
- Update risk parameters
- Modify fee percentages

### 2. Feature Activation Proposal
For proposing activation of new features.

```bash
cp templates/feature_activation_proposal.toml my-feature.toml
# Edit and customize my-feature.toml
cargo run --bin governance-cli submit my-feature.toml
```

**Use cases:**
- Enable new lending pools
- Activate collateral types
- Deploy updated contracts

### 3. Treasury Allocation Proposal
For proposing use of treasury funds.

```bash
cp templates/treasury_allocation_proposal.toml my-allocation.toml
# Edit and customize my-allocation.toml
cargo run --bin governance-cli submit my-allocation.toml
```

**Use cases:**
- Fund development
- Marketing initiatives
- Security audits

## Validation

All templates are validated against the governance schema:

```bash
cargo run --bin governance-validator templates/parameter_update_proposal.toml
```

## Creating Custom Proposals

1. Copy a template matching your proposal type
2. Fill in the required fields
3. Add supporting documentation
4. Run validation: `governance-validator your-proposal.toml`
5. Submit: `governance-cli submit your-proposal.toml`

## Voting

Once submitted, proposals enter a voting phase:
- **Voting Period**: 7 days (configurable)
- **Quorum Required**: 40-50% (depends on proposal type)
- **Approval**: > 50% Yes votes

## Documentation

For detailed governance rules, see [GOVERNANCE.md](../GOVERNANCE.md)
