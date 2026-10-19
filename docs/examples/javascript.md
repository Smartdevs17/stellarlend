# JavaScript/TypeScript Examples

## Installation

```bash
npm install axios stellar-sdk
```

## Depositing Collateral

```typescript
import axios from 'axios';
import StellarSdk from 'stellar-sdk';

const API_BASE = 'https://api.stellarlend.io/api';
const token = 'your-jwt-token';

async function depositCollateral(asset: string, amount: string) {
  try {
    const response = await axios.post(
      `${API_BASE}/v1/lending/deposit`,
      {
        asset,
        amount,
      },
      {
        headers: {
          Authorization: `Bearer ${token}`,
          'Content-Type': 'application/json',
        },
      }
    );

    console.log('Deposit successful:', response.data);
    return response.data;
  } catch (error) {
    console.error('Deposit failed:', error.response?.data);
    throw error;
  }
}

// Usage
depositCollateral('USDC', '1000.00');
```

## Borrowing Assets

```typescript
async function borrowAssets(asset: string, amount: string) {
  try {
    const response = await axios.post(
      `${API_BASE}/v1/lending/borrow`,
      {
        asset,
        amount,
      },
      {
        headers: {
          Authorization: `Bearer ${token}`,
        },
      }
    );

    console.log('Borrow successful:', response.data);
    return response.data;
  } catch (error) {
    console.error('Borrow failed:', error.response?.data);
    throw error;
  }
}

// Usage
borrowAssets('USDT', '500.00');
```

## Checking Position

```typescript
async function getPosition(userAddress: string) {
  try {
    const response = await axios.get(
      `${API_BASE}/v1/lending/positions/${userAddress}`,
      {
        headers: {
          Authorization: `Bearer ${token}`,
        },
      }
    );

    const positions = response.data.positions;
    
    positions.forEach((pos) => {
      console.log('Position:', {
        collateral: pos.collateral,
        borrowed: pos.borrowed,
        healthFactor: pos.healthFactor,
      });
    });

    return positions;
  } catch (error) {
    console.error('Failed to fetch positions:', error.response?.data);
    throw error;
  }
}

// Usage
getPosition('GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX');
```

## Creating Multi-Step Transaction

```typescript
async function createMultiStepTransaction(userAddress: string) {
  try {
    const response = await axios.post(
      `${API_BASE}/transactions`,
      {
        userAddress,
        description: 'Swap and deposit',
        steps: [
          {
            operation: 'swap',
            amount: '100',
            assetAddress: 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA',
          },
          {
            operation: 'deposit',
            amount: '100',
            assetAddress: 'CBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB',
          },
        ],
        ttlSeconds: 600,
      },
      {
        headers: {
          Authorization: `Bearer ${token}`,
        },
      }
    );

    console.log('Transaction created:', response.data);
    return response.data;
  } catch (error) {
    console.error('Failed to create transaction:', error.response?.data);
    throw error;
  }
}
```

## Stellar Signature Authentication

```typescript
import * as StellarSdk from 'stellar-sdk';

function createStellarSignature(
  method: string,
  url: string,
  body: any,
  secretKey: string
): { signature: string; timestamp: string } {
  const keypair = StellarSdk.Keypair.fromSecret(secretKey);
  const timestamp = Date.now().toString();
  
  const payload = `${method}:${url}:${JSON.stringify(body)}:${timestamp}`;
  const signature = keypair.sign(Buffer.from(payload)).toString('base64');
  
  return { signature, timestamp };
}

async function creditLineWithSignature(
  delegateAddress: string,
  maxAmount: string,
  secretKey: string
) {
  const userAddress = StellarSdk.Keypair.fromSecret(secretKey).publicKey();
  const body = {
    delegateAddress,
    maxAmount,
    interestRate: '0.06',
    maturityDate: new Date(Date.now() + 365 * 24 * 60 * 60 * 1000).toISOString(),
  };

  const { signature, timestamp } = createStellarSignature(
    'POST',
    '/api/credit/create',
    body,
    secretKey
  );

  try {
    const response = await axios.post(
      `${API_BASE}/credit/create`,
      body,
      {
        headers: {
          'X-User-Address': userAddress,
          'X-Stellar-Signature': signature,
          'X-Payload-Timestamp': timestamp,
          'Content-Type': 'application/json',
        },
      }
    );

    console.log('Credit line created:', response.data);
    return response.data;
  } catch (error) {
    console.error('Failed to create credit line:', error.response?.data);
    throw error;
  }
}
```

## Error Handling

```typescript
async function handleApiError(error: any) {
  if (error.response) {
    const { status, data } = error.response;
    
    switch (status) {
      case 401:
        console.error('Unauthorized: Check your token');
        break;
      case 403:
        console.error('Forbidden: Insufficient permissions');
        break;
      case 404:
        console.error('Not found:', data.error);
        break;
      case 429:
        console.error('Rate limited: Too many requests');
        break;
      default:
        console.error('Error:', data.error);
    }
  } else if (error.request) {
    console.error('No response received:', error.request);
  } else {
    console.error('Error:', error.message);
  }
}
```

## Complete Example: Lending Workflow

```typescript
class StellarLendClient {
  private api: axios.AxiosInstance;
  private userAddress: string;

  constructor(token: string, userAddress: string) {
    this.userAddress = userAddress;
    this.api = axios.create({
      baseURL: API_BASE,
      headers: {
        Authorization: `Bearer ${token}`,
        'Content-Type': 'application/json',
      },
    });
  }

  async deposit(asset: string, amount: string) {
    const response = await this.api.post('/v1/lending/deposit', {
      asset,
      amount,
    });
    return response.data;
  }

  async getPosition() {
    const response = await this.api.get(
      `/v1/lending/positions/${this.userAddress}`
    );
    return response.data;
  }

  async borrow(asset: string, amount: string) {
    const response = await this.api.post('/v1/lending/borrow', {
      asset,
      amount,
    });
    return response.data;
  }

  async repay(asset: string, amount: string) {
    const response = await this.api.post('/v1/lending/repay', {
      asset,
      amount,
    });
    return response.data;
  }
}

// Usage
async function main() {
  const client = new StellarLendClient(
    'your-jwt-token',
    'GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX'
  );

  // Deposit collateral
  await client.deposit('USDC', '1000');
  console.log('Deposited 1000 USDC');

  // Check position
  const position = await client.getPosition();
  console.log('Position:', position);

  // Borrow
  await client.borrow('USDT', '500');
  console.log('Borrowed 500 USDT');

  // Check updated position
  const updatedPosition = await client.getPosition();
  console.log('Updated position:', updatedPosition);
}

main().catch(console.error);
```
