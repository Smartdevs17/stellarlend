import { DifferentialTester, ContractImplementation, MockContract, TestCase } from '../test/differential/comparator';
import * as fs from 'fs';
import * as path from 'path';

// This script generates a differential testing report and saves it to a file
// Usage: npm run test:differential:report

async function generateReport(): Promise<void> {
  // Reference implementations stand in for real contract clients. Replace these
  // with live Soroban client bindings when wiring up end-to-end differential runs.
  const oldContract: MockContract = {
    call: async (input: any) => ({ version: 'v1', method: input?.method })
  };

  const newContract: MockContract = {
    call: async (input: any) => ({ version: 'v2', method: input?.method })
  };

  const implementations: ContractImplementation[] = [
    { name: 'v1', contract: oldContract, version: '1.0.0' },
    { name: 'v2', contract: newContract, version: '2.0.0' }
  ];

  const tester = new DifferentialTester(implementations);

  // Define comprehensive test cases
  const testCases: TestCase[] = [
    {
      name: 'Basic transfer functionality',
      input: { method: 'transfer', args: ['user1', 'user2', 100] },
      expectedBehavior: (output: any) => output && output.success === true
    },
    {
      name: 'Balance query',
      input: { method: 'getBalance', args: ['user1'] },
      expectedBehavior: (output: any) => typeof output === 'number' && output >= 0
    },
    {
      name: 'State consistency',
      input: { method: 'getState', args: [] },
      expectedBehavior: (output: any) => {
        return output &&
               typeof output.totalSupply === 'number' &&
               typeof output.userBalances === 'object';
      }
    },
    {
      name: 'Error handling - insufficient balance',
      input: { method: 'transfer', args: ['user1', 'user2', 999999] },
      expectedBehavior: (output: any) => {
        return output.error && output.error.includes('Insufficient balance');
      }
    },
    {
      name: 'Interest calculation',
      input: { method: 'calculateInterest', args: [1000, 5] },
      expectedBehavior: (output: any) => typeof output === 'number' && output > 0
    }
  ];

  // Run the differential tests
  const divergences = await tester.runTestSuite(testCases);

  // Generate the report
  const report = tester.getDivergenceReport();

  // Ensure the output directory exists
  const outputDir = path.join(__dirname, '../test-results');
  if (!fs.existsSync(outputDir)) {
    fs.mkdirSync(outputDir, { recursive: true });
  }

  // Save the report
  const outputPath = path.join(outputDir, 'differential-report.md');
  fs.writeFileSync(outputPath, report);

  console.log(`Differential testing report generated at: ${outputPath}`);

  // Also output to console
  console.log('\n' + report);

  // Exit with error code if there are critical divergences
  const hasCritical = divergences.some(d => d.severity === 'critical');
  if (hasCritical) {
    console.error('\nCritical divergences detected!');
    process.exit(1);
  }
}

generateReport().catch(error => {
  console.error('Error generating differential report:', error);
  process.exit(1);
});
