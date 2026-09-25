# Security Policy & Bug Bounty Program

We take the security of StellarLend seriously. This document outlines our vulnerability disclosure policy and bug bounty program rules.

## Bug Bounty Program

Our Bug Bounty Program incentivizes external security researchers to find and responsibly disclose vulnerabilities. 

### Severity and Rewards
Rewards are based on the severity of the vulnerability and are paid in USDC.

| Severity | Description | Reward |
| --- | --- | --- |
| **Critical** | Vulnerabilities causing unauthorized access to protocol funds or significant state corruption. | $10,000 |
| **High** | Vulnerabilities allowing manipulation of protocol logic without direct fund theft, or disruption of key operations. | $5,000 |
| **Medium** | Issues that degrade user experience or cause temporary denial of service. | $2,000 |
| **Low** | Minor issues, edge cases, and cosmetic bugs with minimal impact on security. | $500 |

### Scope
**In-Scope:**
- Smart contracts within `contract/` and `packages/`
- Backend API services (`api/`) handling state and critical operations.

**Out-of-Scope:**
- Third-party oracle services (unless specifically integrated within our codebase and misconfigured by us)
- Phishing or Social Engineering attacks
- Physical attacks against servers

### Responsible Disclosure Policy

**Please do not open a public GitHub issue to report a vulnerability.** A public
issue discloses the vulnerability to everyone immediately, before the team has
had a chance to assess or fix it, and it cannot be made private after the fact.

Use one of the private channels below instead:

1. **GitHub Security Advisories (preferred).** Open
   `https://github.com/Smartdevs17/stellarlend/security/advisories/new` and
   submit a private draft report. Drafts are visible only to the maintainers you
   name until you publish them, and researchers are not credited publicly unless
   they choose to be.
   *This requires the repository to have private vulnerability reporting enabled
   under `Settings → Advanced security`. If the page 404s, the maintainers need
   to turn it on before this channel is usable.*
2. **Bug Bounty Submission Form.** Our [Bug Bounty Submission Form](/bug-bounty)
   is also private.

Notes for reporters:
- The backend API (`api/`) is explicitly **in scope**. Reports against it are
  welcome and paid out under the same schedule as contract findings.
- Do not exploit the vulnerability further than necessary to prove its existence.
- Provide us a reasonable amount of time to resolve the issue before disclosing
  it to the public or a third party.
- If you have already filed a public issue containing vulnerability details,
  please open a private advisory as well and ask a maintainer to delete or redact
  the public issue.

### SLAs
We aim to respond to reports based on severity:
- **Critical**: < 24h
- **High**: < 72h
- **Medium/Low**: within 7 days

### Payout Mechanism
Payouts are handled manually via stablecoin (USDC) transfers to the researcher's provided wallet address after the fix is deployed.

### Hall of Fame
We publicly acknowledge the contributions of researchers who help us secure our protocol on our Hall of Fame page!
