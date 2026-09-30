# Differential Testing Framework

## Overview

The differential testing framework compares behavioral outputs between different implementations of Stellar smart contracts (e.g., original vs upgraded versions) to detect subtle behavioral changes that unit tests might miss.

## Key Features

- **Multi-implementation Testing**: Run the same test cases against multiple contract versions
- **Property-based Testing**: Generate random inputs to explore edge cases
- **Divergence Detection**: Identify differences in outputs between implementations
- **Severity Classification**: Categorize divergences as critical, warning, or info
- **Migration Verification**: Special tests for state consistency during upgrades
- **CI Integration**: Automated regression detection in continuous integration

## Usage

### Basic Setup

```typescript
import { DifferentialTester, ContractImplementation } from '../test/differential/comparator';
import { Contract } from '@stellar/stellar-sdk/contract';

// Define your contract implementations
const implementations: ContractImplementation[] = [
  { name: 'v1', contract: oldContract, version: '1.0.0' },
  { name: 'v2', contract: newContract, version: '2.0.0' }
];

// Create the tester with optional numeric tolerance
const tester = new DifferentialTester(implementations, 0.0001);
```

### Running Test Cases

```typescript
const testCases = [
  {
    name: 'Basic functionality',
    input: { method: 'getBalance', args: ['user1'] },
    expectedBehavior: (output) => typeof output === 'number'
  },
  {
    name: 'State consistency',
    input: { method: 'getState', args: [] }
  }
];

const divergences = await tester.runTestSuite(testCases);
```

### Property-based Testing

```typescript
import * as fc from 'fast-check';
import { createPropertyBasedTests } from '../test/differential/comparator';

const inputArbitraries = [
  fc.record({
    method: fc.constant('transfer'),
    args: fc.array(fc.string())
  })
];

const propertyTests = createPropertyBasedTests(
  implementations,
  inputArbitraries,
  100 // number of test cases
);

const divergences = await tester.runTestSuite(propertyTests);
```

### Generating Reports

```typescript
const report = tester.getDivergenceReport();
console.log(report);
```

## Configuration

### Tolerance

Set a numeric tolerance for floating-point comparisons:

```typescript
// Allow differences up to 0.01
const tester = new DifferentialTester(implementations, 0.01);
```

### Severity Levels

Divergences are classified as:

- **Critical**: Different error behaviors or violations of expected behavior
- **Warning**: Different outputs that don't violate expected behavior
- **Info**: Minor differences that may not affect functionality

## CI Integration

The framework is designed to work with GitHub Actions. Example workflow:

```yaml
name: Differential Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-node@v3
      - run: npm install
      - run: npm run test:differential
```

To fail the build on critical divergences:

```typescript
const divergences = await tester.runTestSuite(testCases);
const hasCritical = divergences.some(d => d.severity === 'critical');
if (hasCritical) {
  process.exit(1);
}
```

## Best Practices

1. **Start with Critical Paths**: Focus on core contract functionality first
2. **Use Property-based Tests**: For exploring edge cases and non-deterministic behavior
3. **Set Appropriate Tolerances**: For numeric comparisons where small differences are acceptable
4. **Review All Divergences**: Even non-critical ones may indicate potential issues
5. **Update Tests with New Versions**: Add new implementations to the test suite as they're developed
6. **Document Known Divergences**: Some differences may be intentional and should be documented

## Handling Non-deterministic Behavior

For contracts with non-deterministic outputs:

1. Use the `expectedBehavior` function to validate outputs rather than comparing exact values
2. Set appropriate tolerance levels for numeric values
3. Consider mocking external dependencies that cause non-determinism

## Example Test Cases

### Basic Functionality

```typescript
{
  name: 'Token transfer',
  input: { method: 'transfer', args: ['sender', 'receiver', 100] },
  expectedBehavior: (output) => output.success === true
}
```

### State Verification

```typescript
{
  name: 'State consistency after upgrade',
  input: { method: 'getState', args: [] },
  expectedBehavior: (output) => {
    return output &&
           typeof output.totalSupply === 'number' &&
           typeof output.userBalances === 'object';
  }
}
```

### Error Cases

```typescript
{
  name: 'Insufficient balance error',
  input: { method: 'transfer', args: ['user', 'receiver', 999999] },
  expectedBehavior: (output) => {
    return output.error && output.error.includes('Insufficient balance');
  }
}
```

## Troubleshooting

### False Positives

If the framework reports divergences that are actually correct:

1. Add an `expectedBehavior` function to the test case
2. Increase the tolerance if dealing with numeric differences
3. Document the expected difference in the test case name

### Performance Issues

For slow contracts:

1. Reduce the number of property-based test cases
2. Run differential tests separately from unit tests
3. Consider caching contract responses for identical inputs

## Contributing

When adding new contract implementations:

1. Add the implementation to the `implementations` array
2. Update the test suite to cover new functionality
3. Verify that all existing tests still pass
4. Add any implementation-specific test cases
