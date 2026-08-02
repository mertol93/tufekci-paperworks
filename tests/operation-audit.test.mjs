import assert from "node:assert/strict";
import test from "node:test";
import {
  filterOperationAudit,
  formatOperationDuration,
  operationAuditLabel,
  operationAuditOutcomeLabel
} from "../src/operationAudit.ts";
import { formatNumber, translate } from "../src/i18n.ts";

const british = (key, values) => translate("en-GB", key, values);
const britishNumber = (value, options) => formatNumber("en-GB", value, options);

test("provides a plain UK-English label for every audited workflow", () => {
  const kinds = [
    "annotations",
    "annotation-inspection",
    "archive",
    "batch",
    "batch-inspection",
    "bookmark-inspection",
    "bookmarks",
    "certificate",
    "certificate-validation",
    "compression",
    "compression-preview",
    "content",
    "content-inspection",
    "edit-safety-inspection",
    "finishing",
    "finishing-inspection",
    "form-inspection",
    "forms",
    "health",
    "merge",
    "ocr-review",
    "searchable-ocr",
    "organise",
    "page-import-inspection",
    "page-transfer",
    "privacy",
    "privacy-inspection",
    "protection",
    "redaction",
    "redaction-inspection",
    "scan",
    "scan-preview",
    "scanner-capture",
    "split"
  ];
  assert.deepEqual(
    kinds.map((kind) => operationAuditLabel(kind, british)),
    [
      "Annotations",
      "Annotation review",
      "PDF/A archive",
      "Batch recipe",
      "Batch source review",
      "Bookmark review",
      "Bookmarks",
      "Certificate signing",
      "Certificate validation",
      "Compression",
      "Compression preview",
      "Page-content editing",
      "Page-content review",
      "Edit-safety review",
      "Page Finish",
      "Page Finish review",
      "Form review",
      "Forms",
      "Document Health",
      "Merge",
      "OCR confidence review",
      "Searchable OCR",
      "Organise and export",
      "Page import review",
      "Cross-document page transfer",
      "Privacy Cleaner",
      "Privacy Inspection",
      "Password protection",
      "Permanent redaction",
      "Redaction review",
      "Scan and OCR",
      "Scan clean-up preview",
      "Connected scanner capture",
      "Split"
    ]
  );
  assert.equal(operationAuditOutcomeLabel("failed", british), "Could not complete");
  assert.equal(
    operationAuditLabel("page-transfer", (key, values) =>
      translate("de-DE", key, values)
    ),
    "Seitenübertragung zwischen Dokumenten"
  );
});

test("formats bounded operation durations without false precision", () => {
  assert.equal(formatOperationDuration(-25, british, britishNumber), "Under 1 second");
  assert.equal(formatOperationDuration(999, british, britishNumber), "Under 1 second");
  assert.equal(formatOperationDuration(1_000, british, britishNumber), "1 second");
  assert.equal(formatOperationDuration(12_400, british, britishNumber), "12 seconds");
  assert.equal(formatOperationDuration(60_000, british, britishNumber), "1 minute");
  assert.equal(formatOperationDuration(8_100_000, british, britishNumber), "2.3 hours");
  assert.equal(
    formatOperationDuration(
      8_100_000,
      (key, values) => translate("de-DE", key, values),
      (value, options) => formatNumber("de-DE", value, options)
    ),
    "2,3 Stunden"
  );
});

test("filters outcomes without changing the source history", () => {
  const entries = [
    {
      id: "one",
      operation: "merge",
      outcome: "succeeded",
      startedAtMs: 1,
      completedAtMs: 2,
      durationMs: 1
    },
    {
      id: "two",
      operation: "split",
      outcome: "cancelled",
      startedAtMs: 3,
      completedAtMs: 4,
      durationMs: 1
    }
  ];
  assert.equal(filterOperationAudit(entries, "all"), entries);
  assert.deepEqual(filterOperationAudit(entries, "cancelled"), [entries[1]]);
  assert.equal(entries.length, 2);
});
