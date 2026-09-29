import { DifferentialTester, ContractImplementation } from './comparator';
import { Contract } from '@stellar/stellar-sdk/contract';

describe('Differential Testing - CI Integration', () => {
  it('should fail CI on critical divergences', async () => {
    const mockContractA = {
      call: jest.fn().mockRejectedValue(new Error('Implementation error'))
    } as unknown as Contract;

    const mockContractB = {
      call: jest.fn().mockResolvedValue({ success: true })
    } as unknown as Contract;

    const implementations: ContractImplementation[] = [
      { name: 'failing', contract: mockContractA, version: '1.0.0' },
      { name: 'working', contract: mockContractB, version: '2.0.0' }
    ];

    const tester = new DifferentialTester(implementations);
    const testCases = [
      {
        name: 'Critical error test',
        input: { method: 'test', args: [] }
      }
    ];

    const divergences = await tester.runTestSuite(testCases);
    const hasCritical = divergences.some(d => d.severity === 'critical');

    expect(hasCritical).toBe(true);
    // In CI, this would fail the build
  });

  it('should pass CI with no critical divergences', async () => {
    const mockContractA = {
      call: jest.fn().mockResolvedValue({ result: 1 })
    } as unknown as Contract;

    const mockContractB = {
      call: jest.fn().mockResolvedValue({ result: 1 })
    } as unknown as Contract;

    const implementations: ContractImplementation[] = [
      { name: 'implA', contract: mockContractA, version: '1.0.0' },
      { name: 'implB', contract: mockContractB, version: '1.0.0' }
    ];

    const tester = new DifferentialTester(implementations);
    const testCases = [
      {
        name: 'Consistent behavior test',
        input: { method: 'test', args: [] }
      }
    ];

    const divergences = await tester.runTestSuite(testCases);
    const hasCritical = divergences.some(d => d.severity === 'critical');

    expect(hasCritical).toBe(false);
  });

  it('should handle non-deterministic behavior with tolerance', async () => {
    const mockContractA = {
      call: jest.fn().mockResolvedValue({ value: 100 })
    } as unknown as Contract;

    const mockContractB = {
      call: jest.fn().mockResolvedValue({ value: 101 })
    } as unknown as Contract;

    const implementations: ContractImplementation[] = [
      { name: 'implA', contract: mockContractA, version: '1.0.0' },
      { name: 'implB', contract: mockContractB, version: '1.0.0' }
    ];

    // With tolerance of 1, these should not be considered divergent
    const tester = new DifferentialTester(implementations, 1);
    const testCases = [
      {
        name: 'Tolerant comparison test',
        input: { method: 'test', args: [] }
      }
    ];

    const divergences = await tester.runTestSuite(testCases);
    expect(divergences).toHaveLength(0);
  });
});
