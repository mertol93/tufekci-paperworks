import assert from "node:assert/strict";
import test from "node:test";

import {
  classifyPdfOpenError,
  classifyPdfRangeFailure,
  MAX_PDF_OPENING_PASSWORD_BYTES,
  pdfOpenErrorTranslationKey,
  validPdfOpeningPasswordInput
} from "../src/pdfPassword.ts";

function namedError(name, message = "Private native detail") {
  const error = new Error(message);
  error.name = name;
  return error;
}

test("classifies PDF opening failures without retaining exception details", () => {
  assert.equal(classifyPdfOpenError(namedError("InvalidPDFException")), "invalid");
  assert.equal(classifyPdfOpenError(namedError("PasswordException")), "password");
  assert.equal(classifyPdfOpenError(namedError("ResponseException")), "unreadable");
  assert.equal(classifyPdfOpenError(namedError("MissingPDFException")), "unreadable");
  assert.equal(
    classifyPdfOpenError(new Error("C:\\Users\\person\\private-document.pdf")),
    "unknown"
  );
  assert.equal(classifyPdfOpenError("C:\\Users\\person\\private-document.pdf"), "unknown");
  assert.equal(classifyPdfOpenError(namedError("PasswordException"), "changed"), "changed");
});

test("reduces native range failures to stable path-free codes", () => {
  assert.equal(
    classifyPdfRangeFailure(
      "The source PDF changed on disk at C:\\Users\\person\\private-document.pdf"
    ),
    "changed"
  );
  assert.equal(
    classifyPdfRangeFailure("Access denied at C:\\Users\\person\\private-document.pdf"),
    "unreadable"
  );
  assert.equal(classifyPdfRangeFailure(new Error("private native detail")), "unreadable");
});

test("maps every stable PDF opening outcome to a translation key", () => {
  assert.deepEqual(
    ["cancelled", "changed", "invalid", "password", "unreadable", "unknown"].map((code) => [
      code,
      pdfOpenErrorTranslationKey(code)
    ]),
    [
      ["cancelled", "app.document.error.cancelled"],
      ["changed", "app.document.error.changed"],
      ["invalid", "app.document.error.invalid"],
      ["password", "app.document.error.password"],
      ["unreadable", "app.document.error.unreadable"],
      ["unknown", "app.document.openFailedDetail"]
    ]
  );
});

test("bounds opening passwords by UTF-8 bytes and rejects control separators", () => {
  assert.equal(MAX_PDF_OPENING_PASSWORD_BYTES, 1024);
  assert.equal(validPdfOpeningPasswordInput(""), true);
  assert.equal(validPdfOpeningPasswordInput("paperworks-test"), true);
  assert.equal(validPdfOpeningPasswordInput("a".repeat(1024)), true);
  assert.equal(validPdfOpeningPasswordInput("a".repeat(1025)), false);
  assert.equal(validPdfOpeningPasswordInput("ü".repeat(512)), true);
  assert.equal(validPdfOpeningPasswordInput("ü".repeat(513)), false);
  assert.equal(validPdfOpeningPasswordInput("line\nbreak"), false);
  assert.equal(validPdfOpeningPasswordInput("line\rbreak"), false);
  assert.equal(validPdfOpeningPasswordInput("null\0byte"), false);
});
