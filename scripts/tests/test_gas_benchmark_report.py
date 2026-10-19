"""Tests for the gas regression gate in gas_benchmark_report.py.

Run with: python3 -m unittest discover -s scripts/tests -t scripts/tests
"""

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path

_MODULE_PATH = Path(__file__).resolve().parents[1] / "gas_benchmark_report.py"
_spec = importlib.util.spec_from_file_location("gas_benchmark_report", _MODULE_PATH)
assert _spec and _spec.loader
report = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(report)


def _result(operation: str, instructions: int) -> dict:
    return {"operation": operation, "instructions": instructions}


class RegressionFailuresTest(unittest.TestCase):
    def test_flags_increase_above_threshold(self) -> None:
        baseline = {"deposit": _result("deposit", 1000)}
        results = [_result("deposit", 1200)]

        failures = report.regression_failures(results, baseline, 10.0)

        self.assertEqual([f["operation"] for f in failures], ["deposit"])
        self.assertAlmostEqual(failures[0]["increase_pct"], 20.0)

    def test_ignores_increase_within_threshold_and_decreases(self) -> None:
        baseline = {"deposit": _result("deposit", 1000), "borrow": _result("borrow", 1000)}
        results = [_result("deposit", 1050), _result("borrow", 500)]

        self.assertEqual(report.regression_failures(results, baseline, 10.0), [])


class UngatedOperationsTest(unittest.TestCase):
    def test_reports_new_and_zero_baseline_operations(self) -> None:
        baseline = {
            "deposit": _result("deposit", 1000),
            "legacy": _result("legacy", 0),
        }
        results = [
            _result("deposit", 1000),
            _result("legacy", 500),
            _result("brand_new", 300),
        ]

        self.assertEqual(
            report.ungated_operations(results, baseline),
            ["brand_new", "legacy"],
        )

    def test_gated_operations_are_not_reported(self) -> None:
        baseline = {"deposit": _result("deposit", 1000)}
        results = [_result("deposit", 1100)]

        self.assertEqual(report.ungated_operations(results, baseline), [])


if __name__ == "__main__":
    unittest.main()
