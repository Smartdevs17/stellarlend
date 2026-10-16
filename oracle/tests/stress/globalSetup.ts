/**
 * Vitest global setup/teardown for the oracle stress suite (#691).
 *
 * Runs once in the main process, around the whole run: clears any artifacts
 * from a previous run up front, then merges the per-worker scenario shards into
 * the final JSON and markdown report.
 */

import { resetStressReport, writeStressReport, REPORT_DIR } from './report.js';

export async function setup(): Promise<void> {
  resetStressReport();
}

export async function teardown(): Promise<void> {
  const summary = writeStressReport();

  // Printed rather than logged so the summary lands in CI output next to the
  // test results, where whoever triggered the run will actually see it.
  console.log(
    `\nOracle stress report: ${summary.passed}/${summary.totalScenarios} scenarios passed` +
      `\n  ${REPORT_DIR}/oracle-stress-report.md` +
      `\n  ${REPORT_DIR}/oracle-stress-report.json\n`
  );
}
