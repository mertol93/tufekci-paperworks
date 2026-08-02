import test from "node:test";
import assert from "node:assert/strict";
import { checksFromInspection } from "../src/pdfEditSafety.ts";

const sources = [
  { id: "first", label: "First PDF", path: "C:/private/first.pdf" },
  { id: "second", label: "Second PDF", path: "C:/private/second.pdf" }
];

const readyResult = {
  certificateSignature: true,
  encrypted: false,
  formFields: true,
  pageCount: 3,
  sourceModifiedAtMs: 1,
  sourceSize: 2,
  xfa: false
};

test("maps aggregate edit-safety items back to source order", () => {
  const checks = checksFromInspection(sources, {
    failedCount: 1,
    inspectedCount: 1,
    items: [
      { error: "Password required", result: null, sourceIndex: 1 },
      { error: null, result: readyResult, sourceIndex: 0 }
    ],
    sourceCount: 2
  });

  assert.equal(checks[0].id, "first");
  assert.equal(checks[0].status, "ready");
  assert.equal(checks[0].result?.certificateSignature, true);
  assert.equal(checks[1].id, "second");
  assert.equal(checks[1].status, "error");
  assert.equal(checks[1].error, "Password required");
});

test("fails closed when an aggregate result does not match the current sources", () => {
  const checks = checksFromInspection(sources, {
    failedCount: 0,
    inspectedCount: 2,
    items: [
      { result: readyResult, sourceIndex: 0 },
      { result: readyResult, sourceIndex: 0 }
    ],
    sourceCount: 2
  });

  assert.deepEqual(checks.map((check) => check.status), ["error", "error"]);
  assert.ok(checks.every((check) => check.error?.includes("did not match")));
});

test("bounds retained per-source edit-safety diagnostics", () => {
  const checks = checksFromInspection([sources[0]], {
    failedCount: 1,
    inspectedCount: 0,
    items: [{ error: "x".repeat(5_000), sourceIndex: 0 }],
    sourceCount: 1
  });

  assert.equal(checks[0].status, "error");
  assert.equal(checks[0].error?.length, 4_096);
});
