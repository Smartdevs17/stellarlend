# Contributing to StellarLend

Thanks for your interest in StellarLend — a lending and borrowing protocol for
Stellar/Soroban. This guide covers the setup, the repository layout, the
commands you will need, and the conventions every contribution follows.

By participating you agree to keep discussions respectful and constructive. For
security issues, do **not** open a public issue — see [SECURITY.md](SECURITY.md).

## Where to start

- Read the documentation index: [`docs/README.md`](docs/README.md).
- New contributors: pick an issue labelled `documentation` or `devex` — they are
  scoped to be self-contained, and the same labels mark tasks in the
  contract, API, and oracle workspaces.
- The developer portal walkthrough lives at
  [`docs/developer-portal.md`](docs/developer-portal.md).

## Prerequisites

| Tool | Version | Needed for |
| --- | --- | --- |
| Node.js | `>= 20` (see root `package.json` `engines`) | API, oracle, SDK, UI, tests |
| Rust (stable) + `wasm32-unknown-unknown` | see the workspace `Cargo.toml` | Soroban contracts |
| `stellar-cli` | current release | building and deploying contracts |
| Docker + Compose | current release | local devnet stack (optional) |

## Setup

```bash
git clone https://github.com/<your-fork>/stellarlend.git
cd stellarlend
npm install                      # installs every npm workspace (api, oracle, packages/*)
rustup target add wasm32-unknown-unknown
```

Docker users can skip the local toolchain: see the "Docker Quickstart" section of
[README.md](README.md), and `docker/devnet/README.md` for the full local devnet.

## Repository layout

| Path | What it is |
| --- | --- |
| `api/` | HTTP API service (npm workspace) |
| `oracle/` | Price oracle service (npm workspace) |
| `packages/sdk/` | TypeScript SDK consumed by integrators |
| `packages/ui-lending/` | Lending UI package |
| `contracts/` | Soroban contracts (root Cargo workspace) plus EVM/experimental Solidity |
| `stellar-lend/` | Separate workspace: contracts, client, fuzz, benchmarks, formal verification, indexing prototype |
| `docker/devnet/` | Local Soroban devnet compose stack |
| `scripts/` | Devnet, testing, and reporting scripts |
| `docs/` | Documentation — index at [`docs/README.md`](docs/README.md) |

## Common commands

| Task | Command |
| --- | --- |
| Install JS workspaces | `npm install` |
| Type-check everything | `npm run typecheck` |
| Run root test suite | `npm test` |
| Run every workspace's tests | `npm run test:all` |
| Build all JS workspaces | `npm run build:all` |
| Lint API + oracle | `npm run lint:all` |
| Contract test suite (all suites / gas / coverage) | `scripts/testing/run-contract-tests.sh [bench\|coverage]` |
| Reproduce CI locally | `./local-ci.sh` |
| Boot the local devnet | `npm run devnet` |
| Reset devnet data | `npm run devnet:reset` |
| Devnet smoke tests | `npm run test:devnet` |

If a command needs configuration, copy the relevant `.env.example` (for example
`api/.env`, `oracle/.env`, `docker/devnet/.env`) first.

## Asset onboarding checklist

Onboarding a new asset is a multi-step process that touches contracts, the
oracle, the API, and documentation. Work through this checklist and reference it
in your pull request so reviewers can confirm every step is covered.

- [ ] **Proposal** — open an issue describing the asset, its Stellar issuer and
  asset code, decimals, and the risk parameters you propose.
- [ ] **Contract configuration** — add the asset to the relevant Soroban
  contract configuration and cover it with a positive and a negative test.
- [ ] **Oracle feed** — register the price feed in `oracle/`, document the
  source and update cadence, and add a test for a stale or missing price.
- [ ] **API exposure** — surface the asset through `api/` (market metadata,
  decimals, and risk parameters) and update the API tests.
- [ ] **SDK and UI** — update `packages/sdk/` and `packages/ui-lending/` so
  integrators and the UI can list and interact with the asset.
- [ ] **Risk review** — record the collateral factor, liquidation threshold,
  and supply/borrow caps, and get sign-off from a maintainer.
- [ ] **Documentation** — add the asset to the supported-assets documentation
  and link it from `docs/README.md`.
- [ ] **Verification** — run `npm run test:all` and `./local-ci.sh`, and note
  the results in the pull request.

## Making changes

1. **Fork and branch.** Branch from the latest `main`:
   `fix/<issue>-<slug>`, `feat/<slug>`, or `docs/<slug>`.
2. **Keep it scoped.** One issue per pull request; split unrelated cleanups out.
3. **Write tests.** New behavior needs a positive and a negative test; bug fixes
   should include a regression test that fails without the fix.
4. **Commit conventionally.** The repo enforces
   [Conventional Commits](https://www.conventionalcommits.org/) via commitlint:
   allowed types are `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`,
   `build`, `ci`, `chore`, `revert`. Subjects are lowercase, at most 100
   characters, and do not end with a period. Example:

   ```
   fix(lending): reject zero-collateral borrows (#123)
   ```
5. **Update documentation.** Anything user-visible should be reflected in
   `docs/` and, when the change is notable, in `CHANGELOG.md` under
   `Unreleased`.

## Pull requests

Fill in the [pull request template](.github/pull_request_template.md) — "What
Changed", "Why", "Testing", and the related issue (`Closes #123`). Before you
request review:

- [ ] `npm run test:all` passes (or the subset your change touches).
- [ ] `./local-ci.sh` passes for contract changes.
- [ ] The PR links its issue, and the description explains how to verify it.
- [ ] New documentation is linked from `docs/README.md`.
- [ ] New assets follow the [asset onboarding checklist](#asset-onboarding-checklist).

A maintainer reviews within a few business days. Address review comments with
new commits rather than force-pushing mid-review; large, un-reviewable diffs
will be asked to split.

## Adding documentation

Place new documents in `docs/` using short, descriptive filenames (for example
`RISK_PARAMETERS.md`), start with a one-paragraph summary, and link the document
from `docs/README.md` so it is discoverable. Prefer relative links between docs
so they work on GitHub and in local checkouts.

When onboarding an asset, update the supported-assets documentation as part of
the [asset onboarding checklist](#asset-onboarding-checklist).

## Security

Report vulnerabilities privately following [SECURITY.md](SECURITY.md). Never
include secrets, private keys, or real user data in issues, pull requests, or
test fixtures.
