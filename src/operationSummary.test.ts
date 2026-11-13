import assert from "node:assert/strict";
import test from "node:test";
import type { OperationProgress } from "./contracts.ts";
import { operationSummary } from "./operationSummary.ts";

function progress(statuses: OperationProgress["items"][number]["status"][], phase: OperationProgress["phase"] = "completed"): OperationProgress {
  return {
    operationId: "test", sequence: 1, phase, slow: false, stopped: false,
    items: statuses.map((status, index) => ({ targetId: String(index), status, error: null })),
  };
}

test("only nonempty completed all-success results get the success sentence", () => {
  assert.equal(operationSummary("zh-Hans", progress(["succeeded"])), "已更新 1 个目录的图标。");
  assert.equal(operationSummary("en", progress(["succeeded"])), "Updated the icon for 1 folder.");
  assert.equal(operationSummary("en", progress(["succeeded", "succeeded"])), "Updated the icons for 2 folders.");
  for (const locale of ["en", "zh-Hans"] as const) {
    assert.equal(operationSummary(locale, progress([])), "");
    for (const phase of ["checking", "running", "uncertain"] as const) {
      assert.equal(operationSummary(locale, progress(["succeeded"], phase)), locale === "en" ? "Completed 1 / 1" : "已完成 1 / 1");
    }
  }
});

test("mixed results omit zero counts and keep stopped remaining items and uncertain writes", () => {
  const mixed = progress(["succeeded", "failed", "succeeded"]);
  assert.equal(operationSummary("zh-Hans", mixed), "成功 2 个 · 失败 1 个");
  assert.equal(operationSummary("en", mixed), "Succeeded: 2 · Failed: 1");
  const stopped = progress(["succeeded", "pending", "pending"]);
  stopped.stopped = true;
  assert.equal(operationSummary("zh-Hans", stopped), "成功 1 个 · 未执行 2 个");
  assert.equal(operationSummary("en", stopped), "Succeeded: 1 · Not started: 2");
  const uncertain = progress(["succeeded", "unknown"]);
  assert.equal(operationSummary("zh-Hans", uncertain), "成功 1 个 · 结果待确认 1 个");
  assert.equal(operationSummary("en", uncertain), "Succeeded: 1 · Result uncertain: 1");
  uncertain.phase = "uncertain";
  assert.equal(operationSummary("zh-Hans", uncertain), "已完成 1 / 2");
  assert.equal(operationSummary("en", uncertain), "Completed 1 / 2");
  assert.equal(operationSummary("zh-Hans", progress(["running"])), "正在处理 1 个");
  assert.equal(operationSummary("en", progress(["running"])), "Processing: 1");
});
