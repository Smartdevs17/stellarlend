# StellarLend Integration Guide

This guide shows how to integrate StellarLend into your application, whether you're building a wallet, trading platform, portfolio manager, or any other service.

## Overview

StellarLend provides a REST API and smart contracts for lending and borrowing operations on the Stellar blockchain. Integration typically involves:

1. User authentication via JWT tokens
2. Querying user positions and available liquidity
3. Executing lending/borrowing transactions
4. Monitoring positions and risk metrics
5. Handling webhooks for real-time updates

## Quick Start

### Step 1: Register for API Access

```bash
# Create an account and get API credentials
curl -X POST https://api.stellarlend.io/api/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "email": "your-email@example.com",
    "password": "secure-password"
  }'

# Login to get JWT token
curl -X POST https://api.stellarlend.io/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "email": "your-email@example.com",
    "password": "secure-password"
  }'
```

### Step 2: Set Up Your Environment

```bash
# Install required packages
npm install axios stellar-sdk dotenv

# Create .env file
cat > .env << 'EOF'
STELLARLEND_API_URL=https://api.stellarlend.io/api
STELLARLEND_API_KEY=your-api-key
STELLAR_NETWORK=PUBLIC
STELLAR_SECRET_KEY=S...
EOF
```

### Step 3: Make Your First API Call

```javascript
const axios = require('axios');

const client = axios.create({
  baseURL: process.env.STELLARLEND_API_URL,
  headers: {
    'X-API-Key': process.env.STELLARLEND_API_KEY,
    'Content-Type': 'application/json',
  },
});

// Get user positions
const positions = await client.get('/v1/lending/positions/GXXXXXX');
console.log(positions.data);
```

## Integration Scenarios

### Scenario 1: Wallet Integration

**Goal:** Add lending/borrowing features to your wallet

```typescript
import StellarSdk from 'stellar-sdk';
import axios from 'axios';

class WalletIntegration {
  constructor(userAddress: string, apiKey: string) {
    this.userAddress = userAddress;
    this.client = axios.create({
      baseURL: 'https://api.stellarlend.io/api',
      headers: { 'X-API-Key': apiKey },
    });
  }

  async getAvailableCollateral() {
    const positions = await this.client.get(
      `/v1/lending/positions/${this.userAddress}`
    );
    return positions.data.positions[0]?.collateral || [];
  }

  async depositCollateral(asset: string, amount: string) {
    return await this.client.post('/v1/lending/deposit', {
      asset,
      amount,
    });
  }

  async borrowAssets(asset: string, maxAmount: string) {
    const positions = await this.getAvailableCollateral();
    const borrowingPower = this.calculateBorrowingPower(positions);
    
    if (parseFloat(maxAmount) > borrowingPower) {
      throw new Error('Insufficient borrowing power');
    }

    return await this.client.post('/v1/lending/borrow', {
      asset,
      amount: maxAmount,
    });
  }

  private calculateBorrowingPower(collateral: any[]): number {
    // Implement LTV calculation based on collateral composition
    return collateral.reduce((sum, item) => sum + item.value * item.ltv, 0);
  }
}

// Usage
const wallet = new WalletIntegration(userAddress, apiKey);
await wallet.depositCollateral('USDC', '1000');
const borrowResult = await wallet.borrowAssets('USDT', '500');
```

### Scenario 2: Trading Platform Integration

**Goal:** Enable margin trading with StellarLend

```typescript
class MarginTradingEngine {
  constructor(apiKey: string) {
    this.client = axios.create({
      baseURL: 'https://api.stellarlend.io/api',
      headers: { 'X-API-Key': apiKey },
    });
  }

  async executeMarginTrade(
    userAddress: string,
    fromAsset: string,
    toAsset: string,
    amount: string,
    leverage: number
  ) {
    // 1. Validate borrowing capacity
    const health = await this.checkHealthFactor(userAddress);
    if (health < 2.0) {
      throw new Error('Health factor too low for margin trading');
    }

    // 2. Create multi-step transaction
    const txId = await this.createMultiStepTx(userAddress, [
      {
        operation: 'borrow',
        asset: fromAsset,
        amount: (parseFloat(amount) * leverage).toString(),
      },
      {
        operation: 'swap',
        fromAsset,
        toAsset,
        amount: (parseFloat(amount) * leverage).toString(),
      },
      {
        operation: 'deposit',
        asset: toAsset,
        amount: (parseFloat(amount) * leverage).toString(),
      },
    ]);

    // 3. Sign and execute
    return await this.executeTransaction(userAddress, txId);
  }

  private async checkHealthFactor(userAddress: string): Promise<number> {
    const positions = await this.client.get(
      `/v1/lending/positions/${userAddress}`
    );
    return positions.data.positions[0]?.healthFactor || 0;
  }

  private async createMultiStepTx(userAddress: string, steps: any[]) {
    const result = await this.client.post('/transactions', {
      userAddress,
      steps,
      ttlSeconds: 600,
    });
    return result.data.transaction.txId;
  }

  private async executeTransaction(userAddress: string, txId: string) {
    // Implementation for signing and submitting to blockchain
  }
}
```

### Scenario 3: Portfolio Manager Integration

**Goal:** Track and optimize user positions

```typescript
class PortfolioManager {
  constructor(apiKey: string) {
    this.client = axios.create({
      baseURL: 'https://api.stellarlend.io/api',
      headers: { 'X-API-Key': apiKey },
    });
  }

  async analyzePortfolio(userAddress: string) {
    const positions = await this.client.get(
      `/v1/lending/positions/${userAddress}`
    );
    const pos = positions.data.positions[0];

    return {
      totalCollateral: this.sumValues(pos.collateral),
      totalBorrowed: this.sumValues(pos.borrowed),
      healthFactor: pos.healthFactor,
      interestAccrued: this.calculateAccruedInterest(pos.borrowed),
      liquidationRisk: this.assessLiquidationRisk(pos.healthFactor),
      recommendations: this.generateRecommendations(pos),
    };
  }

  generateRecommendations(position: any): string[] {
    const recommendations = [];
    
    if (position.healthFactor < 1.5) {
      recommendations.push('Increase collateral to reduce liquidation risk');
    }
    
    if (position.healthFactor > 3.0) {
      recommendations.push('Consider borrowing more to optimize capital efficiency');
    }

    return recommendations;
  }

  private sumValues(assets: any[]): number {
    return assets.reduce((sum, asset) => sum + parseFloat(asset.value), 0);
  }

  private calculateAccruedInterest(borrowed: any[]): number {
    return borrowed.reduce(
      (sum, item) => sum + parseFloat(item.interestAccrued || 0),
      0
    );
  }

  private assessLiquidationRisk(healthFactor: number): string {
    if (healthFactor < 1.0) return 'CRITICAL';
    if (healthFactor < 1.5) return 'HIGH';
    if (healthFactor < 2.0) return 'MODERATE';
    return 'LOW';
  }
}
```

### Scenario 4: DeFi Aggregator Integration

**Goal:** Combine StellarLend with other DeFi protocols

```typescript
class DeFiAggregator {
  constructor(apiKey: string) {
    this.stellarlend = axios.create({
      baseURL: 'https://api.stellarlend.io/api',
      headers: { 'X-API-Key': apiKey },
    });
  }

  async findBestYieldOpportunity(asset: string, amount: string) {
    // Get StellarLend rates
    const slRates = await this.getStellarLendRates(asset);

    // Get other protocol rates
    const otherRates = await this.getOtherProtocolRates(asset);

    // Compare and recommend
    return {
      bestProtocol: this.selectBest([slRates, ...otherRates]),
      rates: [slRates, ...otherRates],
      estimatedYield: this.calculateYield(amount, slRates.supplyRate),
    };
  }

  private async getStellarLendRates(asset: string) {
    const response = await this.stellarlend.get(`/v1/lending/rates/${asset}`);
    return {
      protocol: 'StellarLend',
      supplyRate: response.data.supplyRate,
      borrowRate: response.data.borrowRate,
      utilization: response.data.utilization,
    };
  }

  private selectBest(rates: any[]) {
    return rates.reduce((best, current) => 
      current.supplyRate > best.supplyRate ? current : best
    );
  }

  private calculateYield(amount: string, rate: number): number {
    return parseFloat(amount) * rate;
  }
}
```

## Authentication Methods

### 1. API Key (Easiest)

```javascript
const client = axios.create({
  headers: {
    'X-API-Key': 'your-api-key-here',
  },
});
```

### 2. JWT Token (More Secure)

```javascript
// Get token
const auth = await axios.post('https://api.stellarlend.io/api/auth/login', {
  email: 'user@example.com',
  password: 'password',
});

const token = auth.data.token;

// Use token
const client = axios.create({
  headers: {
    'Authorization': `Bearer ${token}`,
  },
});
```

### 3. Stellar Signature (For Sensitive Operations)

```javascript
import StellarSdk from 'stellar-sdk';

function signRequest(method, url, body, secretKey) {
  const keypair = StellarSdk.Keypair.fromSecret(secretKey);
  const timestamp = Date.now().toString();
  
  const payload = `${method}:${url}:${JSON.stringify(body)}:${timestamp}`;
  const signature = keypair.sign(Buffer.from(payload)).toString('base64');
  
  return {
    'X-User-Address': keypair.publicKey(),
    'X-Stellar-Signature': signature,
    'X-Payload-Timestamp': timestamp,
  };
}

// Usage
const headers = signRequest('POST', '/api/credit/create', body, secretKey);
const response = await axios.post(url, body, { headers });
```

## Error Handling

```typescript
async function handleRequest(request: Promise<any>) {
  try {
    return await request;
  } catch (error) {
    if (error.response) {
      const { status, data } = error.response;

      switch (status) {
        case 400:
          console.error('Validation Error:', data.error);
          break;
        case 401:
          console.error('Authentication Failed');
          break;
        case 403:
          console.error('Insufficient Permissions');
          break;
        case 429:
          console.error('Rate Limited - Wait before retrying');
          // Implement exponential backoff
          break;
        case 500:
          console.error('Server Error - Retry later');
          break;
      }
    }
    throw error;
  }
}
```

## Webhooks Setup

Register your application to receive real-time updates:

```typescript
async function setupWebhooks(apiKey: string, webhookUrl: string) {
  const client = axios.create({
    baseURL: 'https://api.stellarlend.io/api',
    headers: { 'X-API-Key': apiKey },
  });

  // Subscribe to events
  await client.post('/v1/webhooks/subscribe', {
    url: webhookUrl,
    events: [
      'position.created',
      'position.updated',
      'transaction.completed',
      'liquidation.triggered',
      'interest.accrued',
    ],
  });
}

// Webhook handler in your Express app
app.post('/webhooks/stellarlend', (req, res) => {
  const { event, data } = req.body;

  switch (event) {
    case 'position.created':
      handleNewPosition(data);
      break;
    case 'liquidation.triggered':
      handleLiquidationWarning(data);
      break;
    case 'transaction.completed':
      handleTransactionComplete(data);
      break;
  }

  res.json({ success: true });
});
```

## Rate Limiting & Best Practices

1. **Implement Caching**
   ```javascript
   const cache = new Map();
   const TTL = 60000; // 1 minute

   async function cachedRequest(key, fn) {
     const cached = cache.get(key);
     if (cached && Date.now() - cached.time < TTL) {
       return cached.value;
     }
     
     const value = await fn();
     cache.set(key, { value, time: Date.now() });
     return value;
   }
   ```

2. **Batch Requests**
   ```javascript
   // Instead of multiple requests
   const positions = await Promise.all([
     getPosition(addr1),
     getPosition(addr2),
     getPosition(addr3),
   ]);
   ```

3. **Implement Exponential Backoff**
   ```javascript
   async function retryWithBackoff(fn, maxRetries = 3) {
     for (let i = 0; i < maxRetries; i++) {
       try {
         return await fn();
       } catch (error) {
         if (error.response?.status === 429) {
           const delay = Math.pow(2, i) * 1000;
           await new Promise(resolve => setTimeout(resolve, delay));
        continue;
         }
         throw error;
       }
     }
   }
   ```

## Testing Your Integration

```bash
# Use StellarLend Testnet
export STELLARLEND_API_URL=https://testnet-api.stellarlend.io/api

# Test with small amounts first
curl -X POST $STELLARLEND_API_URL/v1/lending/deposit \
  -H "X-API-Key: $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"asset": "USDC", "amount": "1.00"}'

# Monitor test transactions
curl -X GET "$STELLARLEND_API_URL/v1/lending/positions/$TEST_ADDRESS" \
  -H "X-API-Key: $API_KEY"
```

## Common Integration Patterns

### Pattern 1: Auto-Rebalancing

```typescript
async function autoRebalance(userAddress: string, targetAllocation: {}) {
  const current = await getPositions(userAddress);
  const adjustments = calculateAdjustments(current, targetAllocation);
  
  for (const adjustment of adjustments) {
    if (adjustment.action === 'deposit') {
      await deposit(adjustment.asset, adjustment.amount);
    } else if (adjustment.action === 'withdraw') {
      await withdraw(adjustment.asset, adjustment.amount);
    }
  }
}
```

### Pattern 2: Health Factor Monitoring

```typescript
async function monitorHealth(userAddress: string, minHealthFactor: number) {
  setInterval(async () => {
    const health = await getHealthFactor(userAddress);
    
    if (health < minHealthFactor) {
      await notifyUser(`Health factor low: ${health}`);
      // Optionally auto-repay some debt
      await autoRepay(userAddress, calculateRepayAmount(health));
    }
  }, 60000); // Check every minute
}
```

## Support & Resources

- **API Reference:** See [API.md](./API.md)
- **Code Examples:** See [examples/](./examples/)
- **Issues:** https://github.com/Smartdevs17/stellarlend/issues
- **Discord:** https://discord.gg/stellarlend

## Next Steps

1. Create a developer account
2. Generate API credentials
3. Read the [API Reference](./API.md)
4. Explore [code examples](./examples/)
5. Test on testnet
6. Deploy to production
