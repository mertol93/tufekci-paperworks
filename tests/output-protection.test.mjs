import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  createOutputProtectionDraft,
  outputProtectionIsValid,
  toPdfOutputProtection,
  validPdfPassword
} from "../src/outputProtection.ts";
import {
  pdfJobProtectionPolicies,
  pdfJobProtectionSummary
} from "../src/pdfJobs.ts";

const nativeSource = (name) =>
  readFileSync(new URL(`../src-tauri/src/${name}`, import.meta.url), "utf8");

function nativePdfJobKinds() {
  const source = nativeSource("pdf_jobs.rs");
  const body = source.match(/pub enum PdfJobKind \{(?<body>[\s\S]*?)\n\}/u)?.groups?.body;
  assert.ok(body, "The native PDF job enum must remain statically inspectable");
  const kinds = [];
  let renamed = null;
  for (const line of body.split("\n")) {
    const rename = line.match(/#\[serde\(rename = "([^"]+)"\)\]/u);
    if (rename) {
      renamed = rename[1];
      continue;
    }
    const variant = line.match(/^\s*([A-Z][A-Za-z0-9]*),\s*$/u);
    if (variant) {
      kinds.push(renamed ?? variant[1].toLowerCase());
      renamed = null;
    }
  }
  return kinds;
}

test("disabled output protection needs no passwords", () => {
  const draft = createOutputProtectionDraft();
  assert.equal(outputProtectionIsValid(draft, false), true);
  assert.equal(toPdfOutputProtection(draft, false), null);
});

test("valid output protection maps only the two native passwords", () => {
  const draft = {
    enabled: true,
    openPassword: "opening-passphrase",
    openPasswordConfirmation: "opening-passphrase",
    ownerPassword: "administrator-passphrase",
    ownerPasswordConfirmation: "administrator-passphrase"
  };
  assert.equal(outputProtectionIsValid(draft, true), true);
  assert.deepEqual(toPdfOutputProtection(draft, true), {
    openPassword: "opening-passphrase",
    ownerPassword: "administrator-passphrase"
  });
});

test("output protection rejects unavailable QPDF, mismatches and shared passwords", () => {
  const draft = {
    enabled: true,
    openPassword: "same-password",
    openPasswordConfirmation: "same-password",
    ownerPassword: "same-password",
    ownerPasswordConfirmation: "same-password"
  };
  assert.equal(outputProtectionIsValid(draft, true), false);
  assert.equal(outputProtectionIsValid({ ...draft, ownerPassword: "different-password" }, true), false);
  assert.equal(outputProtectionIsValid(draft, false), false);
  assert.throws(() => toPdfOutputProtection(draft, true), /valid opening and administrator/u);
});

test("PDF passwords enforce character, control and UTF-8 byte bounds", () => {
  assert.equal(validPdfPassword("short"), false);
  assert.equal(validPdfPassword("line-one\nline-two"), false);
  assert.equal(validPdfPassword("é".repeat(63)), true);
  assert.equal(validPdfPassword("é".repeat(64)), false);
});

test("classifies every native PDF job under an exhaustive protection policy", () => {
  assert.deepEqual(
    Object.keys(pdfJobProtectionPolicies).sort(),
    nativePdfJobKinds().sort()
  );

  const jobsWithPolicy = (policy) =>
    Object.entries(pdfJobProtectionPolicies)
      .filter(([, value]) => value === policy)
      .map(([kind]) => kind)
      .sort();

  assert.deepEqual(jobsWithPolicy("optional-aes-256-output"), [
    "annotations",
    "batch",
    "bookmarks",
    "compression",
    "content",
    "finishing",
    "forms",
    "merge",
    "organise",
    "page-transfer",
    "privacy",
    "redaction",
    "scan",
    "searchable-ocr",
    "split"
  ]);
  assert.deepEqual(jobsWithPolicy("preserve-source-encryption"), ["certificate"]);
  assert.deepEqual(jobsWithPolicy("manage-aes-256"), ["protection"]);
  assert.deepEqual(jobsWithPolicy("password-aware-plain-output"), ["archive"]);
  assert.deepEqual(jobsWithPolicy("media-input-only"), [
    "ocr-review",
    "scan-preview",
    "scanner-capture"
  ]);
  assert.deepEqual(jobsWithPolicy("inspection-only"), [
    "annotation-inspection",
    "batch-inspection",
    "bookmark-inspection",
    "certificate-validation",
    "compression-preview",
    "content-inspection",
    "edit-safety-inspection",
    "finishing-inspection",
    "form-inspection",
    "health",
    "page-import-inspection",
    "privacy-inspection",
    "redaction-inspection"
  ]);
  assert.equal(
    pdfJobProtectionSummary("archive"),
    "Password-aware PDF/A export with unencrypted output"
  );
});

test("keeps shared structural publishers wired to AES-256 output and reopen checks", () => {
  const sharedPublishers = [
    "annotations.rs",
    "batch.rs",
    "bookmarks.rs",
    "combine.rs",
    "compression.rs",
    "content_editor.rs",
    "forms.rs",
    "page_finish.rs",
    "privacy.rs",
    "redaction.rs",
    "scan_export.rs"
  ];

  for (const file of sharedPublishers) {
    const source = nativeSource(file);
    assert.match(source, /PdfOutputProtection/u, `${file} lost its output-protection request`);
    assert.match(
      source,
      /validate_pdf_output_protection/u,
      `${file} lost password validation`
    );
    assert.match(
      source,
      /lock_pdf_changes(?:_from_source)?_with_control/u,
      `${file} lost cancellable QPDF publication`
    );
    assert.match(source, /encryption:/u, `${file} no longer reports output encryption`);
    assert.match(
      source,
      /(?:decrypt\(|Some\(&protection\.open_password\)|verify_protected_batch_output)/u,
      `${file} lost password-aware output reopening`
    );
  }

  for (const file of sharedPublishers.filter((file) => file !== "scan_export.rs")) {
    assert.match(nativeSource(file), /input_password/u, `${file} lost encrypted-input support`);
  }
});

test("keeps organiser, certificate, PDF/A and protection-specific rules explicit", () => {
  const organiser = nativeSource("export.rs");
  assert.match(organiser, /primary_input_password/u);
  assert.match(organiser, /pub struct DocumentLock/u);
  assert.match(organiser, /lock_pdf_changes_with_control/u);
  assert.match(organiser, /Some\(&lock\.open_password\)/u);

  const certificate = nativeSource("certificate.rs");
  assert.match(certificate, /input_password/u);
  assert.match(certificate, /Document::load_with_password/u);
  assert.match(certificate, /signed_encrypted != source_encrypted/u);

  const archive = nativeSource("archive.rs");
  assert.match(archive, /input_password/u);
  assert.doesNotMatch(archive, /PdfOutputProtection/u);

  const protection = nativeSource("protection.rs");
  const qpdfRunner = protection.match(
    /fn run_qpdf_with_control\([\s\S]*?\n\}\n\nfn verify_pdf_with_control/u
  )?.[0];
  assert.ok(qpdfRunner, "The QPDF standard-input adapter must remain statically inspectable");
  assert.match(qpdfRunner, /Command::new\("qpdf"\)/u);
  assert.match(qpdfRunner, /command\.arg\("@-"\)/u);
  assert.doesNotMatch(qpdfRunner, /command\.args/u);
  assert.match(protection, /\.stdin\(Stdio::piped\(\)\)/u);
  assert.match(protection, /const MAX_PASSWORD_BYTES: usize = 127/u);
});
