# Contract Deployment Verification Scripts (#1066)

Offline, structural verification of deployment manifests — the record of where
each Stellar contract was deployed, its address, and the exact WASM bytecode
that backs it.

## Why

Deployment drift is silent and costly: a contract deployed with different
bytecode than the reviewed one, a typo'd address, or a stale `deployment.json`
all bypass feature tests. These scripts verify the *record of deployment* so
CI and release runs fail loudly whenever the manifest no longer matches the
artifacts actually built or the address format the network accepts.

## Tools

| Tool | Purpose |
|------|---------|
| `scripts/deploy-verify/index.ts` | Deep manifest verification (address format, uniqueness, bytecode SHA-256 vs local WASM builds, `--update-hashes`) |
| `scripts/verify-deployment.sh manifest` | Shell wrapper wiring the Node verifier into the existing deployment workflow |

## Usage (Node >= 22, no install needed)

```bash
# Structural verification only
node --experimental-strip-types scripts/deploy-verify/index.ts \
  --manifest scripts/deploy-verify/fixtures/deployment.example.json

# Authenticate bytecode: recorded hash must equal locally built artifact hash
node --experimental-strip-types scripts/deploy-verify/index.ts \
  --manifest environments/testnet/deployment.json \
  --wasm-dir stellar-lend/target/wasm32-unknown-unknown/release

# Recompute + write back hashes from local artifacts before committing
node --experimental-strip-types scripts/deploy-verify/index.ts \
  --manifest environments/testnet/deployment.json \
  --wasm-dir stellar-lend/target/wasm32-unknown-unknown/release --update-hashes

# Via the existing shell script
./scripts/verify-deployment.sh manifest environments/testnet/deployment.json
```

Exit codes: `0` verified, `1` verification failed, `2` usage error.

## Manifest schema

```jsonc
{
  "network": "testnet",
  "deployedAt": "2026-01-01T00:00:00.000Z",
  "contracts": [
    {
      "name": "lending",
      "address": "C…",                    // 56-char base32, must start with C
      "wasmArtifact": "lending.wasm",     // file name under target/wasm32…/release
      "wasmHash": "<sha256 of wasm>",     // optional when --wasm-dir is used
      "initialized": true                 // optional; reported if false
    }
  ]
}
```

## Tests

```bash
node --experimental-strip-types --test scripts/deploy-verify/verify.test.ts
```

## Covered acceptance criteria

- Validate Stellar contract address format and per-network uniqueness.
- Detect uninitialized contracts and duplicate names in a manifest.
- Authenticate deployed bytecode by comparing recorded WASM hashes against
  locally built artifacts (catching non-reproducible or drifted builds).
- `--update-hashes` to regenerate recorded hashes from local artifacts.
- Structural verification can run in CI without network access; live
  on-chain reachability (view functions) is a documented follow-up.