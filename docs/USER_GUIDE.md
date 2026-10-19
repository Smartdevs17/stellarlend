# StellarLend User Guide

## Getting Started with StellarLend

Welcome to StellarLend, the decentralized finance (DeFi) lending protocol on the Stellar blockchain. This guide will help you understand how to use StellarLend to lend and borrow assets.

## Prerequisites

- A Stellar wallet (e.g., Lobstr, Stellar Expert, Albedo)
- Supported assets to deposit as collateral
- Basic understanding of DeFi concepts

## Key Concepts

### Collateral

Assets you deposit into the protocol to borrow against. The protocol supports multiple asset types, allowing you to diversify your collateral.

**Supported Collateral:**
- USDC (USD Coin)
- USDT (Tether)
- BTC (Bitcoin via bridge)
- ETH (Ethereum via bridge)

### Borrowing Power

Your ability to borrow is determined by the value of your collateral and the loan-to-value (LTV) ratio.

```
Borrowing Power = Collateral Value × LTV Ratio
```

### Health Factor

Indicates the safety of your position. A higher health factor means lower liquidation risk.

```
Health Factor = Collateral Value / Borrowed Amount
```

**Health Factor Ranges:**
- > 3.0: Very safe
- 2.0 - 3.0: Safe
- 1.5 - 2.0: Moderate risk
- 1.0 - 1.5: High risk
- < 1.0: Liquidation risk

### Interest Rates

Interest rates are dynamic and adjust based on protocol utilization:

```
Interest Rate = Base Rate + (Utilization Rate × Slope)
```

Higher utilization means higher rates, encouraging borrowers to repay and lenders to provide more liquidity.

## Step-by-Step Guide

### 1. Deposit Collateral

#### Using the Web Interface

1. Connect your Stellar wallet
2. Navigate to "Deposit" section
3. Select the asset you want to deposit
4. Enter the amount
5. Confirm the transaction
6. Wait for blockchain confirmation

#### Using the API

```bash
curl -X POST https://api.stellarlend.io/api/v1/lending/deposit \
  -H "Authorization: Bearer YOUR_JWT_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "asset": "USDC",
    "amount": "1000.00"
  }'
```

### 2. Borrow Assets

Once you have collateral deposited:

1. Go to "Borrow" section
2. Select the asset you want to borrow
3. Enter the amount (must not exceed your borrowing power)
4. Review interest rate and due date
5. Confirm the transaction

**Important:** Your health factor will decrease as you borrow more. Monitor it to avoid liquidation.

### 3. Monitor Your Position

#### Via Dashboard

- View total collateral value
- Track borrowed amounts and accrued interest
- Monitor health factor in real-time
- See current interest rates

#### Via API

```bash
curl -X GET https://api.stellarlend.io/api/v1/lending/positions/YOUR_ADDRESS \
  -H "Authorization: Bearer YOUR_JWT_TOKEN"
```

### 4. Repay Your Loan

You can repay at any time:

1. Navigate to "Repay" section
2. Select the borrowed asset
3. Choose repayment amount (or repay in full)
4. Confirm the transaction
5. Interest is calculated at the time of repayment

### 5. Withdraw Collateral

After repaying borrowed assets:

1. Go to "Withdraw" section
2. Select the collateral asset
3. Enter the amount to withdraw
4. Confirm the transaction

**Note:** You can only withdraw collateral that is not required to maintain your health factor.

## Managing Risk

### Monitoring Health Factor

Check your health factor regularly:

```bash
curl -X GET https://api.stellarlend.io/api/v1/lending/health/YOUR_ADDRESS \
  -H "Authorization: Bearer YOUR_JWT_TOKEN"
```

### Avoiding Liquidation

- Keep your health factor > 1.5
- Monitor utilization rates
- Adjust collateral if rates increase
- Set up alerts for health factor changes

### Liquidation Process

If your health factor drops below 1.0:

1. Your position becomes eligible for liquidation
2. Liquidators can repay your debt
3. Collateral is auctioned to cover the debt
4. You lose collateral but retain any surplus

## Advanced Features

### Credit Lines

Delegate credit to trusted addresses:

```bash
curl -X POST https://api.stellarlend.io/api/credit/create \
  -H "X-User-Address: YOUR_ADDRESS" \
  -H "X-Stellar-Signature: SIGNATURE" \
  -H "X-Payload-Timestamp: TIMESTAMP" \
  -H "Content-Type: application/json" \
  -d '{
    "delegateAddress": "GXXXX...",
    "maxAmount": "10000",
    "interestRate": "0.06",
    "maturityDate": "2025-12-31T23:59:59Z"
  }'
```

### Multi-Step Transactions

Execute complex transaction workflows:

1. Swap assets
2. Deposit collateral
3. Borrow assets
4. All in a single atomic transaction

See [Transaction Guide](./TRANSACTION_GUIDE.md) for details.

## Fees

### Deposit/Withdrawal Fees

- Deposit: 0.01%
- Withdrawal: 0.01%

### Interest Fees

- Protocol fee: 10% of interest earned
- Referral fee: Configurable by referrer

### Liquidation Fees

- Liquidation incentive: 5% of liquidated collateral
- Protocol fee: 2% of liquidation amount

## Security Best Practices

1. **Never share your private key** with anyone
2. **Use hardware wallets** for large positions
3. **Enable multi-signature** for extra security
4. **Verify contract addresses** before interacting
5. **Test with small amounts** before large transactions
6. **Monitor your account regularly** for suspicious activity

## Troubleshooting

### Common Issues

**Q: Why can't I borrow more?**
A: You've reached your borrowing power limit. Deposit more collateral or repay existing loans.

**Q: My transaction failed, what happened?**
A: Check that you have sufficient balance, network is operational, and your health factor is above 1.0.

**Q: When does interest accrue?**
A: Interest accrues every block and is calculated on-chain. You can see accrued interest in your position details.

**Q: How do I reduce my health factor risk?**
A: Deposit more collateral or repay some of your borrowed assets.

## Support

- **Documentation:** https://docs.stellarlend.io
- **Discord:** https://discord.gg/stellarlend
- **Email:** support@stellarlend.io
- **GitHub:** https://github.com/stellar/stellarlend

## Additional Resources

- [Risk Management Guide](./RISK_MONITORING_DASHBOARD.md)
- [Oracle Configuration](./ORACLE_CONFIGURATION_GUIDE.md)
- [Governance Guide](./GOVERNANCE.md)
