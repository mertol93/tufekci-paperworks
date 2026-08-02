import assert from "node:assert/strict";
import test from "node:test";
import {
  commitBoundedHistory,
  createBoundedHistory,
  redoBoundedHistory,
  replaceBoundedHistory,
  undoBoundedHistory
} from "../src/useBoundedHistory.ts";

test("commits bounded history and clears the redo branch", () => {
  let history = createBoundedHistory("zero");
  history = commitBoundedHistory(history, "one", undefined, 2);
  history = commitBoundedHistory(history, "two", undefined, 2);
  history = commitBoundedHistory(history, "three", undefined, 2);

  assert.deepEqual(history, {
    future: [],
    past: ["one", "two"],
    present: "three"
  });

  history = undoBoundedHistory(history);
  history = commitBoundedHistory(history, "replacement", undefined, 2);
  assert.deepEqual(history.future, []);
  assert.equal(history.present, "replacement");
});

test("undo and redo move between complete operation states", () => {
  let history = createBoundedHistory(["first"]);
  history = commitBoundedHistory(history, ["first", "second"]);
  history = undoBoundedHistory(history);
  assert.deepEqual(history.present, ["first"]);
  assert.deepEqual(history.future, [["first", "second"]]);

  history = redoBoundedHistory(history);
  assert.deepEqual(history.present, ["first", "second"]);
  assert.deepEqual(history.future, []);
});

test("replacement updates live input without creating an operation", () => {
  const initial = createBoundedHistory("draft");
  const replaced = replaceBoundedHistory(initial, "typing");

  assert.equal(replaced.present, "typing");
  assert.deepEqual(replaced.past, []);
  assert.deepEqual(replaced.future, []);
});

test("snapshot sanitisation keeps passwords out of undo and redo state", () => {
  const sanitise = (sources) => sources.map((source) => ({ ...source, password: "" }));
  let history = createBoundedHistory([
    { id: "one", pageRange: "all", password: "private-input-password", path: "one.pdf" }
  ]);
  history = commitBoundedHistory(
    history,
    [
      { id: "one", pageRange: "1-3", password: "private-input-password", path: "one.pdf" }
    ],
    sanitise
  );

  assert.equal(history.past[0][0].password, "");
  history = undoBoundedHistory(history, sanitise);
  assert.equal(history.future[0][0].password, "");
  assert.doesNotMatch(JSON.stringify(history), /private-input-password/u);
});
