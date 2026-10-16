# StellarLend Developer Guide

## Development Setup

### Prerequisites

- **Node.js** 18+ and npm/yarn
- **Rust** 1.70+ with Soroban target
- **Docker** (for local testing)
- **Git**

### Installation

```bash
git clone https://github.com/stellar/stellarlend
cd stellarlend

# Install dependencies
npm install

# Setup Soroban environment
rustup target add wasm32-unknown-unknown
soroban contract build --manifest-path stellar-lend/Cargo.toml
```

### Configuration

Create `.env` file with required variables:

```env
# API Configuration
API_PORT=3000
API_HOST=localhost
NODE_ENV=development

# Database
DATABASE_URL=postgresql://localhost/stellarlend
REDIS_URL=redis://localhost:6379

# Stellar Configuration
STELLAR_NETWORK=TESTNET
STELLAR_RPC_URL=https://soroban-testnet.stellar.org
SOROBAN_ACCOUNT_SECRET=S...

# Admin Configuration
ADMIN_ADDRESS=GXXXX...

# JWT Configuration
JWT_SECRET=your-secret-key
JWT_EXPIRES_IN=24h

# API Keys
API_KEY_SECRET=your-api-key-secret
```

## Project Structure

```
stellarlend/
├── api/                     # Express.js API server
│   ├── src/
│   │   ├── controllers/     # Request handlers
│   │   ├── services/        # Business logic
│   │   ├── middleware/      # Express middleware
│   │   ├── routes/          # API routes
│   │   ├── types/           # TypeScript types
│   │   └── utils/           # Utility functions
│   └── tests/               # API tests
├── stellar-lend/            # Soroban smart contracts
│   ├── contracts/           # Contract implementations
│   ├── tests/               # Contract tests
│   └── docs/                # Contract documentation
└── docs/                    # Documentation
```

## API Development

### Adding a New Endpoint

1. **Create Controller:**

```typescript
// api/src/controllers/example.controller.ts
import { Request, Response } from 'express';

export async function getExample(req: Request, res: Response) {
  try {
    const data = await exampleService.get();
    return res.json({ success: true, data });
  } catch (error) {
    return res.status(500).json({ success: false, error: error.message });
  }
}
```

2. **Create Service:**

```typescript
// api/src/services/example.service.ts
export class ExampleService {
  async get() {
    // Implement business logic
  }
}

export const exampleService = new ExampleService();
```

3. **Add Route:**

```typescript
// api/src/routes/example.routes.ts
import { Router } from 'express';
import { getExample } from '../controllers/example.controller';
import { authenticateToken } from '../middleware/auth';

const router = Router();
router.get('/', authenticateToken, getExample);

export default router;
```

4. **Register Route in Main App:**

```typescript
// api/src/server.ts
import exampleRoutes from './routes/example.routes';

app.use('/api/example', exampleRoutes);
```

### Authentication in Controllers

```typescript
import { Request, Response } from 'express';
import { AuthRequest } from '../middleware/auth';

export async function protectedEndpoint(req: Request, res: Response) {
  const authReq = req as AuthRequest;
  const userAddress = authReq.user?.address;
  
  if (!userAddress) {
    return res.status(401).json({ error: 'Unauthorized' });
  }
  
  // Use userAddress in your logic
}
```

### Error Handling

```typescript
import { UnauthorizedError, ValidationError } from '../utils/errors';

try {
  // Your code
} catch (error) {
  if (error instanceof UnauthorizedError) {
    return res.status(401).json({ error: error.message });
  }
  if (error instanceof ValidationError) {
    return res.status(400).json({ error: error.message });
  }
  return res.status(500).json({ error: 'Internal server error' });
}
```

## Smart Contract Development

### Writing a Soroban Contract

```rust
// stellar-lend/contracts/lending/src/lib.rs
use soroban_sdk::{contract, contractimpl, Env, Symbol, Address, i128};

#[contract]
pub struct LendingContract;

#[contractimpl]
impl LendingContract {
    pub fn deposit(env: Env, user: Address, asset: Address, amount: i128) -> bool {
        user.require_auth();
        
        // Implementation
        true
    }
}
```

### Testing Contracts

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};

    #[test]
    fn test_deposit() {
        let env = Env::default();
        let contract_id = env.register_contract(None, LendingContract);
        
        // Your test logic
    }
}
```

Run tests:

```bash
cd stellar-lend
cargo test
```

## Testing

### Unit Tests

```typescript
// api/src/services/__tests__/example.service.test.ts
import { describe, it, expect } from '@jest/globals';
import { exampleService } from '../example.service';

describe('ExampleService', () => {
  it('should return data', async () => {
    const result = await exampleService.get();
    expect(result).toBeDefined();
  });
});
```

Run tests:

```bash
npm test
```

### Integration Tests

```bash
npm run test:integration
```

### E2E Tests

```bash
npm run test:e2e
```

## Database

### Migrations

```bash
npm run migrate:create -- add_example_table
npm run migrate:up
```

### Querying

```typescript
import { db } from '../utils/database';

const user = await db.query('SELECT * FROM users WHERE address = $1', [userAddress]);
```

## Logging

```typescript
import logger from '../utils/logger';

logger.info('User deposited', { userAddress, amount });
logger.error('Transaction failed', { error: err.message });
logger.warn('Low liquidity', { pool: 'USDC' });
```

## Performance Optimization

### Caching

```typescript
import { cache } from '../utils/cache';

// Get from cache or fetch
const data = await cache.get('key', async () => {
  return await fetchData();
}, 3600); // 1 hour TTL
```

### Database Indexing

```sql
CREATE INDEX idx_user_address ON positions(user_address);
CREATE INDEX idx_created_at ON transactions(created_at DESC);
```

## Security Checklist

- [ ] Validate all user inputs
- [ ] Use parameterized queries to prevent SQL injection
- [ ] Authenticate sensitive endpoints
- [ ] Implement rate limiting
- [ ] Use HTTPS in production
- [ ] Store secrets in environment variables
- [ ] Implement CORS correctly
- [ ] Log security events
- [ ] Use cryptographic signatures for sensitive operations

## Deployment

### Local Development

```bash
npm run dev
```

### Production Build

```bash
npm run build
npm start
```

### Docker

```bash
docker build -t stellarlend-api .
docker run -p 3000:3000 stellarlend-api
```

## Monitoring

### Health Checks

```bash
curl http://localhost:3000/health
```

### Logs

```bash
tail -f logs/app.log
```

### Metrics

Access Prometheus metrics at `/metrics`

## Common Tasks

### Add a New Field to a Model

1. Create database migration
2. Update TypeScript types
3. Update services to handle new field
4. Add tests

### Integrate a New Oracle

1. Create oracle service
2. Implement price feed interface
3. Add fallback mechanisms
4. Test with stress scenarios

### Add Governance Parameter

1. Update parameter store contract
2. Add admin endpoint to set parameter
3. Emit governance event
4. Update frontend

## Troubleshooting

### Build Errors

```bash
# Clear build cache
rm -rf node_modules dist
npm install
npm run build
```

### Contract Compilation Issues

```bash
# Clean Soroban build
cd stellar-lend
cargo clean
cargo build --target wasm32-unknown-unknown
```

### Database Connection Issues

```bash
# Check PostgreSQL is running
psql -U postgres -d stellarlend -c "SELECT 1"
```

## Resources

- [Soroban Documentation](https://soroban.stellar.org)
- [Stellar Development Foundation](https://developers.stellar.org)
- [Express.js Guide](https://expressjs.com)
- [TypeScript Handbook](https://www.typescriptlang.org/docs/)
