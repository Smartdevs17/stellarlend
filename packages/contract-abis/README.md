# @stellarlend/contract-abis

Generated ABI definitions and type-safe client bindings for StellarLend Soroban smart contracts.

## Getting the ABIs

Every published [GitHub release](https://github.com/Smartdevs17/stellarlend/releases) includes the
extracted contract ABI JSON files as release assets. Download the one(s) you need from the release
page, e.g.:

```bash
gh release download <tag> --repo Smartdevs17/stellarlend --pattern '*.json'
```

The same ABI JSON files are also published as part of this npm package under `abi/`:

```bash
npm install @stellarlend/contract-abis
```

```ts
import lendingAbi from "@stellarlend/contract-abis/abi/lending.json";
```

## Type-safe bindings

Generated TypeScript types and client bindings are exported from the package root:

```ts
import { LendingClient } from "@stellarlend/contract-abis";
```

## Regenerating locally

See [`scripts/abi-extractor/README.md`](../../scripts/abi-extractor/README.md) for the extraction
and type-generation tooling used to produce the contents of this package.
