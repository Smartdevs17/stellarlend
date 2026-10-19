import * as fc from "fast-check";

/**
 * Minimal contract handle used by the differential framework. The framework only
 * ever invokes `call(input)`, so tests supply mocks of this shape. (The Stellar
 * SDK's Soroban client was renamed in v14, hence this local type rather than a
 * direct SDK import.)
 */
export interface MockContract {
  call(input: any): any;
}

export interface ContractImplementation {
  name: string;
  contract: MockContract;
  version: string;
}

export interface TestCase {
  name: string;
  input: any;
  expectedBehavior?: (output: any) => boolean;
}

export interface Divergence {
  implementationA: string;
  implementationB: string;
  testCase: string;
  input: any;
  outputA: any;
  outputB: any;
  severity: "critical" | "warning" | "info";
}

export class DifferentialTester {
  private implementations: ContractImplementation[];
  private tolerance: number;
  private divergences: Divergence[] = [];

  constructor(
    implementations: ContractImplementation[],
    tolerance: number = 0,
  ) {
    if (implementations.length < 2) {
      throw new Error(
        "At least two implementations required for differential testing",
      );
    }
    this.implementations = implementations;
    this.tolerance = tolerance;
  }

  public async runTestSuite(testCases: TestCase[]): Promise<Divergence[]> {
    this.divergences = [];

    for (const testCase of testCases) {
      await this.runTestCase(testCase);
    }

    return this.divergences;
  }

  private async runTestCase(testCase: TestCase): Promise<void> {
    const outputs: Record<string, any> = {};

    for (const impl of this.implementations) {
      try {
        outputs[impl.name] = await this.executeContract(
          impl.contract,
          testCase.input,
        );
      } catch (error) {
        outputs[impl.name] = {
          error: error instanceof Error ? error.message : "Unknown error",
        };
      }
    }

    for (let i = 0; i < this.implementations.length; i++) {
      const implA = this.implementations[i];
      if (!implA) continue;
      for (let j = i + 1; j < this.implementations.length; j++) {
        const implB = this.implementations[j];
        if (!implB) continue;

        const divergence = this.detectDivergence(
          implA.name,
          implB.name,
          testCase,
          outputs[implA.name],
          outputs[implB.name],
        );

        if (divergence) {
          this.divergences.push(divergence);
        }
      }
    }
  }

  private async executeContract(
    contract: MockContract,
    input: any,
  ): Promise<any> {
    // Simplified execution - in real implementation this would call the contract
    // with the given input and return the output
    // Placeholder for actual contract execution logic
    return contract.call(input);
  }

  private detectDivergence(
    nameA: string,
    nameB: string,
    testCase: TestCase,
    outputA: any,
    outputB: any,
  ): Divergence | null {
    // Handle error cases
    if (outputA.error && outputB.error) {
      if (outputA.error !== outputB.error) {
        return {
          implementationA: nameA,
          implementationB: nameB,
          testCase: testCase.name,
          input: testCase.input,
          outputA,
          outputB,
          severity: "critical",
        };
      }
      return null;
    }

    if (outputA.error || outputB.error) {
      return {
        implementationA: nameA,
        implementationB: nameB,
        testCase: testCase.name,
        input: testCase.input,
        outputA,
        outputB,
        severity: "critical",
      };
    }

    // Custom behavior check if provided
    if (testCase.expectedBehavior) {
      const aValid = testCase.expectedBehavior(outputA);
      const bValid = testCase.expectedBehavior(outputB);
      if (aValid !== bValid) {
        return {
          implementationA: nameA,
          implementationB: nameB,
          testCase: testCase.name,
          input: testCase.input,
          outputA,
          outputB,
          severity: "critical",
        };
      }
      return null;
    }

    // Deep comparison with tolerance for numeric values
    if (!this.deepCompare(outputA, outputB, this.tolerance)) {
      return {
        implementationA: nameA,
        implementationB: nameB,
        testCase: testCase.name,
        input: testCase.input,
        outputA,
        outputB,
        severity: "warning",
      };
    }

    return null;
  }

  private deepCompare(a: any, b: any, tolerance: number): boolean {
    if (a === b) return true;
    if (
      typeof a !== "object" ||
      typeof b !== "object" ||
      a === null ||
      b === null
    ) {
      if (typeof a === "number" && typeof b === "number") {
        return Math.abs(a - b) <= tolerance;
      }
      return false;
    }

    const keysA = Object.keys(a);
    const keysB = Object.keys(b);

    if (keysA.length !== keysB.length) return false;

    for (const key of keysA) {
      if (!keysB.includes(key)) return false;
      if (!this.deepCompare(a[key], b[key], tolerance)) return false;
    }

    return true;
  }

  public getDivergenceReport(): string {
    if (this.divergences.length === 0) {
      return "No divergences detected.";
    }

    let report = `# Differential Testing Report\n\n`;
    report += `## Summary\n`;
    report += `- Total divergences: ${this.divergences.length}\n`;
    report += `- Critical: ${this.divergences.filter((d) => d.severity === "critical").length}\n`;
    report += `- Warning: ${this.divergences.filter((d) => d.severity === "warning").length}\n`;
    report += `- Info: ${this.divergences.filter((d) => d.severity === "info").length}\n\n`;

    report += `## Divergences\n\n`;
    for (const divergence of this.divergences) {
      report += `### ${divergence.severity.toUpperCase()}: ${divergence.testCase}\n`;
      report += `- Implementations: ${divergence.implementationA} vs ${divergence.implementationB}\n`;
      report += `- Input: \`${JSON.stringify(divergence.input)}\`\n`;
      report += `- Output A: \`${JSON.stringify(divergence.outputA)}\`\n`;
      report += `- Output B: \`${JSON.stringify(divergence.outputB)}\`\n\n`;
    }

    return report;
  }
}

export function createPropertyBasedTests(
  implementations: ContractImplementation[],
  inputArbitraries: fc.Arbitrary<any>[],
  maxRuns: number = 100,
): TestCase[] {
  const testCases: TestCase[] = [];

  for (let i = 0; i < maxRuns; i++) {
    const input = inputArbitraries.map((arb) => fc.sample(arb, 1)[0]);
    testCases.push({
      name: `Property test ${i + 1}`,
      input: input.length === 1 ? input[0] : input,
    });
  }

  return testCases;
}
