import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  defaultRebalanceSchedule,
  enabledSchedules,
  loadSchedule,
  parseCronExpression,
  validateSchedule,
} from "../schedule.ts";
import { RebalanceRunner } from "../runner.ts";
import { RebalanceBot } from "../index.ts";

describe("rebalance schedule", () => {
  it("loads and validates the default schedule", () => {
    const schedule = loadSchedule();
    assert.equal(schedule.version, 1);
    assert.deepEqual(
      enabledSchedules(schedule).map((entry) => entry.name),
      ["treasury-daily", "pool-weekly", "stablecoin-peg"],
    );
  });

  it("requires a non-empty asset list for the stablecoin peg job", () => {
    const peg = defaultRebalanceSchedule.schedules.find(
      (entry) => entry.name === "stablecoin-peg",
    );
    assert.deepEqual(peg?.assets, ["USDC", "USDT"]);

    const missing = structuredClone(defaultRebalanceSchedule);
    const pegEntry = missing.schedules.find((entry) => entry.name === "stablecoin-peg")!;
    pegEntry.assets = [];
    assert.ok(
      validateSchedule(missing).some((error) => error.includes("stablecoin-peg requires assets")),
    );
  });

  it("rejects malformed cron and duplicate jobs", () => {
    const bad = structuredClone(defaultRebalanceSchedule);
    bad.schedules[0]!.cron = "0 99 * * *";
    assert.ok(
      validateSchedule(bad).some((error) => error.includes("invalid cron")),
    );

    const duplicate = structuredClone(defaultRebalanceSchedule);
    duplicate.schedules[1]!.name = duplicate.schedules[0]!.name;
    assert.ok(
      validateSchedule(duplicate).some((error) => error.includes("duplicate")),
    );
    assert.throws(() => loadSchedule(bad), /Invalid rebalance schedule/);
  });

  it("parses step ranges", () => {
    assert.deepEqual(parseCronExpression("*/15 0-6/2 * * 1-5"), [
      "*/15",
      "0-6/2",
      "*",
      "*",
      "1-5",
    ]);
  });
});

describe("rebalance runner", () => {
  it("runs declarative jobs and records events", async () => {
    const events: Array<{ type: string; name: string }> = [];
    const bot = new RebalanceBot({
      handler: async (entry) => ({
        simulatedActions: entry.name === "treasury-daily" ? 2 : 1,
        executed: true,
      }),
      onEvent: (event) => events.push({ type: event.type, name: event.name }),
    });

    const results = await bot.run();
    assert.equal(results.length, 3);
    assert.equal(results[0]?.executed, true);
    assert.equal(results[0]?.simulatedActions, 2);
    assert.deepEqual(
      events.map((event) => event.type),
      ["start", "success", "start", "success", "start", "success"],
    );
  });

  it("does not execute dry-run jobs", async () => {
    const schedule = structuredClone(defaultRebalanceSchedule);
    schedule.schedules = [schedule.schedules[0]!];
    schedule.schedules[0]!.dryRun = true;
    const runner = new RebalanceRunner({
      schedule,
      handler: async () => ({ simulatedActions: 3, executed: true }),
    });

    const [result] = await runner.run();
    assert.equal(result?.executed, false);
    assert.equal(result?.simulatedActions, 3);
  });

  it("captures handler failures and unknown job names", async () => {
    const runner = new RebalanceRunner({
      handler: async () => {
        throw new Error("boom");
      },
    });

    const [result] = await runner.run("treasury-daily");
    assert.equal(result?.error, "boom");
    await assert.rejects(() => runner.run("missing"), /Unknown rebalance job/);
  });

  it("computes the next UTC run from the cron schedule", () => {
    const runner = new RebalanceRunner();
    const job = runner.getJobs()[0]!;
    const next = runner.nextRun(job, new Date("2026-01-01T00:00:01Z"));
    assert.equal(next.toISOString(), "2026-01-02T00:00:00.000Z");
  });
});
