# Liquidation Protocol

## Dynamic Bonus System

The liquidation bonus adapts to market volatility using a **volatility score** (0–2) derived from:
- **15-minute EMA** of price deviations from 7-day moving average
- **Thresholds**: 5% (stable), 15% (moderate), 30% (high)

### Bonus Calculation
```
bonus = baseBonus × (1 + volatilityFactor × volatilityScore)
```
- **Base Bonus**: 1.2x (20%)
- **Max Scaling**: 0.8x (80% of base)
- **Extreme Cap**: 2.0x (200%)

### Volatility Impact
| Volatility Score | Bonus Range       | Market State  |
|------------------|-------------------|---------------|
| 0.0              | 1.2x              | Stable        |
| 0.15             | 1.2x–1.68x        | Moderate      |
| 0.30+            | 1.68x–2.0x        | High Stress   |

### Configuration
Edit `src/config.ts` to adjust:
- `VOLATILITY_THRESHOLDS`
- `BASE_BONUS`
- `MAX_VOLATILITY_FACTOR`

### Testing
Run with:
```bash
npm test liquidation/bonus
```