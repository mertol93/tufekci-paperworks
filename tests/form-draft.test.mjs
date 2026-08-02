import assert from "node:assert/strict";
import test from "node:test";
import {
  changedFormUpdates,
  createFormHistory,
  redoFormValues,
  undoFormValues,
  updateFormField,
  validateFormDraft
} from "../src/formDraft.ts";

const field = (overrides = {}) => ({
  combo: false,
  editable: true,
  editableChoice: false,
  fieldId: "field-1",
  flattenable: true,
  kind: "text",
  maxLength: null,
  multiline: false,
  multiSelect: false,
  name: "Name",
  options: [],
  password: false,
  readOnly: false,
  required: false,
  signaturePresent: false,
  values: ["Original"],
  widgets: [],
  ...overrides
});

test("tracks only changed editable fields", () => {
  const fields = [field(), field({ editable: false, fieldId: "field-2", values: ["Fixed"] })];
  let history = createFormHistory(fields);
  history = updateFormField(history, "field-1", ["Changed"]);
  history = updateFormField(history, "field-2", ["Ignored"]);
  assert.deepEqual(changedFormUpdates(fields, history.present), [
    { fieldId: "field-1", values: ["Changed"] }
  ]);
});

test("undoes and redoes field changes", () => {
  let history = createFormHistory([field()]);
  history = updateFormField(history, "field-1", ["First"]);
  history = updateFormField(history, "field-1", ["Second"]);
  history = undoFormValues(history);
  assert.deepEqual(history.present["field-1"], ["First"]);
  history = redoFormValues(history);
  assert.deepEqual(history.present["field-1"], ["Second"]);
});

test("validates required text and maximum lengths", () => {
  const required = field({ maxLength: 4, required: true });
  assert.equal(
    validateFormDraft([required], { "field-1": [] })["field-1"],
    "This required field cannot be empty."
  );
  assert.equal(
    validateFormDraft([required], { "field-1": ["Longer"] })["field-1"],
    "Use at most 4 characters."
  );
});

test("validates fixed and multi-select choices", () => {
  const choice = field({
    fieldId: "choice",
    kind: "choice",
    options: [
      { label: "Alpha", value: "A" },
      { label: "Beta", value: "B" }
    ],
    values: ["A"]
  });
  assert.equal(
    validateFormDraft([choice], { choice: ["C"] }).choice,
    "Choose a listed option."
  );
  assert.equal(
    validateFormDraft([choice], { choice: ["A", "B"] }).choice,
    "Choose one value."
  );
  assert.deepEqual(
    validateFormDraft([{ ...choice, multiSelect: true }], { choice: ["A", "B"] }),
    {}
  );
});
