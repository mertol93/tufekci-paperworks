import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { rovingNavigationIndex } from "../src/accessibility.ts";

const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
const accessibility = readFileSync(
  new URL("../src/accessibility.ts", import.meta.url),
  "utf8"
);
const styles = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");

const modalFiles = [
  "AnnotationStudio.tsx",
  "BookmarkStudio.tsx",
  "ComparisonStudio.tsx",
  "ContentEditStudio.tsx",
  "FormStudio.tsx",
  "ImportPagesDialog.tsx",
  "OcrReviewDialog.tsx",
  "OperationAuditDialog.tsx",
  "PageFinishStudio.tsx",
  "PdfPasswordDialog.tsx",
  "RedactionStudio.tsx",
  "UpdateDialog.tsx"
];

test("moves through a roving workflow set with wrapping and boundary keys", () => {
  assert.equal(rovingNavigationIndex(0, 19, "ArrowDown"), 1);
  assert.equal(rovingNavigationIndex(18, 19, "ArrowDown"), 0);
  assert.equal(rovingNavigationIndex(0, 19, "ArrowUp"), 18);
  assert.equal(rovingNavigationIndex(9, 19, "ArrowLeft"), 8);
  assert.equal(rovingNavigationIndex(9, 19, "ArrowRight"), 10);
  assert.equal(rovingNavigationIndex(9, 19, "Home"), 0);
  assert.equal(rovingNavigationIndex(9, 19, "End"), 18);
  assert.equal(rovingNavigationIndex(9, 19, "Enter"), null);
  assert.equal(rovingNavigationIndex(-1, 19, "ArrowDown"), null);
  assert.equal(rovingNavigationIndex(0, 0, "ArrowDown"), null);
});

test("provides a keyboard skip target and an announced roving workflow tab set", () => {
  assert.match(app, /className="skip-link"[\s\S]{0,80}href="#document-editor"/u);
  assert.match(app, /document\.getElementById\("document-editor"\)\?\.focus\(\)/u);
  assert.match(app, /id="document-editor"\s+tabIndex=\{-1\}/u);
  assert.match(app, /aria-orientation="vertical"[\s\S]{0,120}role="tablist"/u);
  assert.match(app, /aria-controls="workflow-details"/u);
  assert.match(app, /aria-selected=\{active\}/u);
  assert.match(app, /role="tab"[\s\S]{0,80}tabIndex=\{active \? 0 : -1\}/u);
  assert.match(app, /rovingNavigationIndex\(currentIndex, workflows\.length, event\.key\)/u);
  assert.match(app, /id="workflow-details"\s+role="tabpanel"\s+tabIndex=\{0\}/u);
  assert.doesNotMatch(app, /aria-label="Workflow settings"/u);
  assert.match(styles, /\.skip-link:focus-visible/u);
  assert.match(styles, /\.document-area:focus-visible/u);
  assert.match(styles, /outline: 3px solid #174ea6/u);
});

test("contains focus, handles safe Escape, and returns focus for every modal", () => {
  assert.match(accessibility, /document\.addEventListener\("keydown", handleKeyDown, true\)/u);
  assert.match(accessibility, /if \(event\.key !== "Tab"\)/u);
  assert.match(accessibility, /event\.stopPropagation\(\)/u);
  assert.match(accessibility, /previouslyFocused\?\.isConnected/u);
  assert.match(accessibility, /data-dialog-initial-focus/u);

  for (const file of modalFiles) {
    const source = readFileSync(new URL(`../src/${file}`, import.meta.url), "utf8");
    assert.match(source, /aria-modal="true"/u, `${file} must remain modal`);
    assert.match(source, /useDialogFocus/u, `${file} must use shared focus management`);
    assert.match(source, /data-dialog-root/u, `${file} must expose its focus boundary`);
    assert.match(source, /ref=\{dialogRef\}/u, `${file} must attach the dialog ref`);
    assert.match(source, /tabIndex=\{-1\}/u, `${file} must provide a focus fallback`);
    assert.match(
      source,
      /data-dialog-initial-focus/u,
      `${file} must identify a predictable initial focus target`
    );
  }
});
