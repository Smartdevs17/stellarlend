import {
  DifferentialTester,
  ContractImplementation,
  MockContract,
  createPropertyBasedTests,
} from "./comparator";
import * as fc from "fast-check";

describe("Differential Testing Framework", () => {
  describe("DifferentialTester", () => {
    let mockContractA: jest.Mocked<MockContract>;
    let mockContractB: jest.Mocked<MockContract>;
    let implementations: ContractImplementation[];
    let tester: DifferentialTester;

    beforeEach(() => {
      mockContractA = {
        call: jest.fn(),
      } as unknown as jest.Mocked<MockContract>;

      mockContractB = {
        call: jest.fn(),
      } as unknown as jest.Mocked<MockContract>;

      implementations = [
        { name: "implA", contract: mockContractA, version: "1.0.0" },
        { name: "implB", contract: mockContractB, version: "2.0.0" },
      ];

      tester = new DifferentialTester(implementations);
    });

    describe("constructor", () => {
      it("should throw error with less than 2 implementations", () => {
        expect(() => {
          new DifferentialTester([implementations[0]!]);
        }).toThrow(
          "At least two implementations required for differential testing",
        );
      });

      it("should accept valid implementations", () => {
        expect(() => {
          new DifferentialTester(implementations);
        }).not.toThrow();
      });
    });

    describe("runTestSuite", () => {
      it("should run test cases against all implementations", async () => {
        mockContractA.call.mockResolvedValue("resultA");
        mockContractB.call.mockResolvedValue("resultB");

        const testCases = [{ name: "test1", input: { method: "test" } }];

        await tester.runTestSuite(testCases);

        expect(mockContractA.call).toHaveBeenCalledWith({ method: "test" });
        expect(mockContractB.call).toHaveBeenCalledWith({ method: "test" });
      });

      it("should detect divergences between implementations", async () => {
        mockContractA.call.mockResolvedValue("resultA");
        mockContractB.call.mockResolvedValue("resultB");

        const testCases = [
          { name: "divergence test", input: { method: "test" } },
        ];

        const divergences = await tester.runTestSuite(testCases);

        expect(divergences.length).toBe(1);
        expect(divergences[0]!.implementationA).toBe("implA");
        expect(divergences[0]!.implementationB).toBe("implB");
        expect(divergences[0]!.testCase).toBe("divergence test");
      });

      it("should handle errors in implementations", async () => {
        mockContractA.call.mockRejectedValue(new Error("Test error"));
        mockContractB.call.mockResolvedValue("success");

        const testCases = [{ name: "error test", input: { method: "test" } }];

        const divergences = await tester.runTestSuite(testCases);

        expect(divergences.length).toBe(1);
        expect(divergences[0]!.severity).toBe("critical");
        expect(divergences[0]!.outputA.error).toBe("Test error");
      });

      it("should use expectedBehavior for validation", async () => {
        mockContractA.call.mockResolvedValue(10);
        mockContractB.call.mockResolvedValue(20);

        const testCases = [
          {
            name: "behavior test",
            input: { method: "test" },
            expectedBehavior: (output: any) => output > 5,
          },
        ];

        const divergences = await tester.runTestSuite(testCases);

        // Both pass the expectedBehavior check, so no divergence
        expect(divergences.length).toBe(0);
      });
    });

    describe("deepCompare", () => {
      it("should compare primitive values", () => {
        const tester = new DifferentialTester(implementations);

        expect((tester as any).deepCompare(1, 1, 0)).toBe(true);
        expect((tester as any).deepCompare(1, 2, 0)).toBe(false);
      });

      it("should compare objects", () => {
        const tester = new DifferentialTester(implementations);

        const obj1 = { a: 1, b: 2 };
        const obj2 = { a: 1, b: 2 };
        const obj3 = { a: 1, b: 3 };

        expect((tester as any).deepCompare(obj1, obj2, 0)).toBe(true);
        expect((tester as any).deepCompare(obj1, obj3, 0)).toBe(false);
      });

      it("should compare with tolerance for numbers", () => {
        const tester = new DifferentialTester(implementations, 0.1);

        expect((tester as any).deepCompare(1.0, 1.05, 0.1)).toBe(true);
        expect((tester as any).deepCompare(1.0, 1.15, 0.1)).toBe(false);
      });

      it("should compare nested objects", () => {
        const tester = new DifferentialTester(implementations);

        const obj1 = { a: { b: 1 } };
        const obj2 = { a: { b: 1 } };
        const obj3 = { a: { b: 2 } };

        expect((tester as any).deepCompare(obj1, obj2, 0)).toBe(true);
        expect((tester as any).deepCompare(obj1, obj3, 0)).toBe(false);
      });
    });

    describe("getDivergenceReport", () => {
      it("should return no divergences message when empty", () => {
        const report = tester.getDivergenceReport();
        expect(report).toContain("No divergences detected");
      });

      it("should format report with divergences", async () => {
        mockContractA.call.mockResolvedValue("resultA");
        mockContractB.call.mockResolvedValue("resultB");

        const testCases = [{ name: "test case", input: { method: "test" } }];

        await tester.runTestSuite(testCases);
        const report = tester.getDivergenceReport();

        expect(report).toContain("Differential Testing Report");
        expect(report).toContain("Total divergences: 1");
        expect(report).toContain("implA vs implB");
        expect(report).toContain("test case");
      });
    });
  });

  describe("createPropertyBasedTests", () => {
    it("should generate test cases from arbitraries", () => {
      const implementations: ContractImplementation[] = [
        { name: "v1", contract: {} as MockContract, version: "1.0.0" },
        { name: "v2", contract: {} as MockContract, version: "2.0.0" },
      ];

      const inputArbitraries = [fc.constant({ method: "test" })];

      const testCases = createPropertyBasedTests(
        implementations,
        inputArbitraries,
        5,
      );

      expect(testCases.length).toBe(5);
      expect(testCases[0]!.name).toContain("Property test");
      expect(testCases[0]!.input).toEqual({ method: "test" });
    });

    it("should generate multiple inputs when multiple arbitraries provided", () => {
      const implementations: ContractImplementation[] = [
        { name: "v1", contract: {} as MockContract, version: "1.0.0" },
      ];

      const inputArbitraries = [fc.constant("arg1"), fc.constant("arg2")];

      const testCases = createPropertyBasedTests(
        implementations,
        inputArbitraries,
        1,
      );

      expect(testCases[0]!.input).toEqual(["arg1", "arg2"]);
    });
  });
});
