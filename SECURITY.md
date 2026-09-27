# Security Policy & Bug Bounty Program

We take the security of StellarLend seriously. This document outlines our vulnerability disclosure policy and bug bounty program rules.

## Approved Private Submission Channel

**The approved private submission channel is GitHub Security Advisories.** This is the preferred and primary method for reporting security vulnerabilities.

1. **GitHub Security Advisories (preferred and recommended).** Open
   `https://github.com/Smartdevs17/stellarlend/security/advisories/new` and
   submit a private draft report. Drafts are visible only to the maintainers you
   name until you publish them, and researchers are not credited publicly unless
   they choose to be.
   *This requires the repository to have private vulnerability reporting enabled
   under `Settings → Advanced security`. If the page 404s, the maintainers need
   to turn it on before this channel is usable.*
2. **Bug Bounty Submission Form.** Our [Bug Bounty Submission Form](/bug-bounty)
   is also private and can be used as an alternative channel.

**Do not open a public GitHub issue to report a vulnerability.** A public
issue discloses the vulnerability to everyone immediately, before the team has
had a chance to assess or fix it, and it cannot be made private after the fact.

If you have already filed a public issue containing vulnerability details,
please open a private advisory as well and ask a maintainer to delete or redact
the public issue.

## Bug Bounty Program

Our Bug Bounty Program incentivizes external security researchers to find and responsibly disclose vulnerabilities.

### Is the Bounty Program Currently Active?

**Yes.** The USDC bug bounty program is currently accepting reports. We welcome submissions from qualified researchers.

### Eligibility

- **Source-code findings qualify.** Locally reproduced source-code findings within the documented scope are eligible for bounty, even when deployment exposure has not been independently verified by the researcher. The focus is on the correctness and security of the code itself.
- **In-Scope:**
  - Smart contracts within `contract/` and `packages/`
  - Backend API services (`api/`) handling state and critical operations.
- **Out-of-Scope:**
  - Third-party oracle services (unless specifically integrated within our codebase and misconfigured by us)
  - Phishing or Social Engineering attacks
  - Physical attacks against servers
- **Duplicate Reports:** If a vulnerability has already been reported by another researcher, you may still submit your own report but it will be considered a duplicate. The first valid report is eligible for payout. We encourage researchers to coordinate and avoid duplicate submissions where possible.

### Severity and Rewards

Rewards are based on the severity of the vulnerability and are paid in USDC.

| Severity | Description | Reward |
| --- | --- | --- |
| **Critical** | Vulnerabilities causing unauthorized access to protocol funds or significant state corruption. | $10,000 |
| **High** | Vulnerabilities allowing manipulation of protocol logic without direct fund theft, or disruption of key operations. | $5,000 |
| **Medium** | Issues that degrade user experience or cause temporary denial of service. | $2,000 |
| **Low** | Minor issues, edge cases, and cosmetic bugs with minimal impact on security. | $500 |

### Payout Details

- **Currency:** All bounty payouts are made in **USDC**.
- **Supported Network:** USDC payouts are processed on the **Stellar network** (Soroban-compatible USDC).
- **Mechanism:** Payouts are handled manually via stablecoin (USDC) transfers to the researcher's provided wallet address after the fix is deployed.
- **Timing:** Payouts occur after the fix has been deployed to the relevant network. Researchers will be asked to provide their Stellar wallet address for payment.

### SLAs

We aim to respond to reports based on severity:
- **Critical**: < 24h
- **High**: < 72h
- **Medium/Low**: within 7 days

### Hall of Fame

We publicly acknowledge the contributions of researchers who help us secure our protocol on our Hall of Fame page!
