# @stellarlend/math (issue #1759)

SDK-facing `bigint` port of `contracts/pool_math`. Every function is pure and
throws `MathError` for the same invalid inputs the Rust crate rejects. Values
use the same scales: basis points are 10,000 and interest indexes use 1e12.

```ts
import { applyBps, healthFactorBps, utilizationBps } from '@stellarlend/math';

applyBps(20_000n, 8_000n); // 16_000n
healthFactorBps(20_000n, 10_000n, 8_000n); // 16_000n
utilizationBps(4_000n, 20_000n); // 2_000n
```

Use `bigint` throughout; do not convert through `number` for on-chain amounts,
because stroop-sized values exceed `Number.MAX_SAFE_INTEGER`.

## Validation

```bash
npm run build -w @stellarlend/math
npm test -w @stellarlend/math
```
