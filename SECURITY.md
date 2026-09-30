# Security Policy

We take the security of StellarLend seriously. This document outlines our vulnerability disclosure policy.

## Reporting a Vulnerability

**The approved private submission channel is GitHub Security Advisories.** This is the preferred and primary method for reporting security vulnerabilities.

1. **GitHub Security Advisories (preferred and recommended).** Open
   `https://github.com/Smartdevs17/stellarlend/security/advisories/new` and
   submit a private draft report. Drafts are visible only to the maintainers you
   name until you publish them, and researchers are not credited publicly unless
   they choose to be.
   *This requires the repository to have private vulnerability reporting enabled
   under `Settings → Advanced security`. If the page 404s, the maintainers need
   to turn it on before this channel is usable.*
2. **Private contact channels.** You can also reach the security team directly
   via the channels listed in [SECURITY_CONTACT.md](SECURITY_CONTACT.md).

**Do not open a public GitHub issue to report a vulnerability.** A public
issue discloses the vulnerability to everyone immediately, before the team has
had a chance to assess or fix it, and it cannot be made private after the fact.

If you have already filed a public issue containing vulnerability details,
please open a private advisory as well and ask a maintainer to delete or redact
the public issue.

## What to Include in a Report

Please provide enough information to reproduce the issue, including:

- The affected repository, file and function (if known).
- Steps to reproduce.
- Impact of the vulnerability.
- Any proposed remediation, if you have one.

Reports can be submitted anonymously if you prefer.

## Scope

- Smart contracts within `contract/` and `packages/`
- Backend API services (`api/`) handling state and critical operations.

**Out of scope:**

- Third-party oracle services (unless specifically integrated within our codebase and misconfigured by us)
- Phishing or Social Engineering attacks
- Physical attacks against servers

## Response Times

We aim to respond to reports based on severity:

- **Critical**: < 24h
- **High**: < 72h
- **Medium/Low**: within 7 days