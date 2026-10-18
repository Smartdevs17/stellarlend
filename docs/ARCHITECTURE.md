# Architecture Diagrams

Visual overview of StellarLend: how the pieces fit together off-chain and
on-chain, how a borrow transaction flows, and how the deployment topology looks.
The diagrams use [Mermaid](https://mermaid.js.org/) and render directly on
GitHub.

The path references below are the source of truth; when a module moves, update
the diagram in the same pull request.

## System context

```mermaid
flowchart TB
  subgraph Off-chain
    UI["Web clients<br/>frontend/ · landing/ · packages/ui-lending"]
    SDKC["packages/sdk<br/>(client façade + error model)"]
    API["api/<br/>HTTP API service"]
    OR["oracle/<br/>price oracle service"]
    TOOLS["bots/ · cli/ · scenarios/"]
    DB[("Database")]
  end
  subgraph On-chain ["On-chain (Stellar / Soroban)"]
    LEND["lending contract<br/>stellar-lend/contracts/lending"]
    HUB["oracle-hub · twap-oracle"]
    MARKET["amm · flash-loan · bridge"]
  end
  UI --> API
  SDKC --> RPC
  TOOLS --> RPC
  API --> RPC
  API --> DB
  OR --> RPC
  RPC(("Soroban RPC")) --> LEND
  RPC --> HUB
  RPC --> MARKET
```

Notes:

- The oracle service feeds `oracle-hub`, which the lending contract reads for
  valuation; `twap-oracle` provides manipulated-resistant averages. See
  [REALTIME_PRICE_FEED.md](REALTIME_PRICE_FEED.md).
- The SDK is transport-agnostic: servers, bots, and browsers all use the same
  client over a different adapter (see
  [CONTRACT_INTEGRATION.md](CONTRACT_INTEGRATION.md)).

## Contract module map

Contracts live under `stellar-lend/contracts/` (separate workspace) and grouped
by concern. The root `contracts/` directory additionally hosts governance,
proposal, and timelock contracts plus EVM/Solidity experiments.

```mermaid
flowchart TB
  subgraph Core lending
    L[lending]
    LC[lending-core]
    LI[lending-interest]
    LR[lending-risk]
    LT[lending-types]
  end
  subgraph Markets
    AMM[amm]
    FL[flash-loan]
    BR[bridge]
    SR[swap-router]
    DCA[dca-module]
  end
  subgraph Risk and liquidation
    RS[risk-scoring]
    LS[liquidation-strategy]
    LA[liquidation-auction]
    INS[insurance-marketplace]
  end
  subgraph Oracles
    OH[oracle-hub]
    TW[twap-oracle]
    CO[credit-oracle]
  end
  subgraph Admin and safety
    TM[treasury-multisig]
    PS[parameter-store]
    RL[rate-limit]
    SEC[security]
  end
  subgraph Tokens
    TK[token · token-adapter]
    DT[debt-token · principal-token]
    VT[yield-token · vault-share]
  end
  subgraph Shared libraries
    SM[safe-math · math · shared-math]
    SS[storage · storage-layer · shared-storage]
    SE[shared-events · errors · common]
  end
```

The clusters are organizational, not dependency claims; each contract's own
`Cargo.toml` and the [MODULAR_ARCHITECTURE.md](../stellar-lend/contracts/MODULAR_ARCHITECTURE.md)
note state what depends on what.

## Borrow lifecycle (sequence)

```mermaid
sequenceDiagram
  participant App as App (SDK client)
  participant RPC as Soroban RPC
  participant L as lending contract
  participant O as oracle-hub
  App->>RPC: simulate borrow(user, asset, amount, collateral)
  RPC->>L: simulate invocation
  L->>O: read price(s) for valuation
  O-->>L: price
  L-->>RPC: result + required auth entries
  RPC-->>App: simulation result
  App->>App: sign transaction (wallet or keypair)
  App->>RPC: submit signed transaction
  RPC->>L: invoke borrow
  L->>L: validate amount, 150% collateral floor, pause,<br/>utilization breaker, asset support
  L-->>RPC: BorrowEvent or Error(Contract, #N)
  RPC-->>App: confirmed result + events
```

The failure points map to contract error codes documented in
[TROUBLESHOOTING.md](TROUBLESHOOTING.md#contract-errors).

## Deployment topology

```mermaid
flowchart LR
  DEV[Developer machine] -->|npm run devnet| DVNET["docker/devnet/<br/>Soroban node · Horizon · Friendbot<br/>API · oracle · Postgres · Redis"]
  PR[Pull request] --> CI["GitHub Actions<br/>contract-tests · integration-tests<br/>e2e-journeys · gas benchmarks"]
  CI --> GATE{Merge}
  GATE --> TN["Soroban testnet deployment"]
  TN --> MN["Mainnet (per phase plan)"]
```

- `docker/devnet` is a full local network, including seeded accounts and
  liquidation scenarios ([docker/devnet/README.md](../docker/devnet/README.md)).
- CI runs the contract suites and journey tests; check names are listed in the
  workflows under `.github/workflows/`.

## Keeping these current

These diagrams are part of the documentation review surface: a PR that adds,
removes, or renames a module under `stellar-lend/contracts/` or changes the
deployment topology should update this file. Diagrams are reviewed like any
other documentation change.
