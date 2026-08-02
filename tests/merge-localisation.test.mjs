import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  localiseMergeWarnings,
  mergeActionErrorTranslationKey
} from "../src/mergeLocalisation.ts";
import { translate } from "../src/i18n.ts";

const number = (value) => String(value);
const t = (locale) => (key, values) => translate(locale, key, values);

test("localises bounded merge warnings without exposing unknown native detail", () => {
  const warnings = localiseMergeWarnings(
    [
      "source.pdf contains form fields. Check their appearances in the combined output.",
      "source.pdf: 2 bookmarks could not be preserved because the destination was unresolved or outside the selected pages.",
      "private/path/document.pdf: internal parser failed",
      "Private native merge failure with confidential detail"
    ],
    t("en-GB"),
    number
  );

  assert.deepEqual(warnings, [
    "source.pdf contains form fields. Check their appearances in the combined copy.",
    "2 bookmarks from source.pdf could not be preserved because their destinations were unresolved or outside the selected pages.",
    "The merge completed with a warning that could not be displayed safely."
  ]);
  assert.doesNotMatch(warnings.join(" "), /private\/path|internal parser|Private native/u);
});

test("uses locale templates for source and protection outcomes", () => {
  const protectedWarning =
    "The combined copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application.";
  const sourceWarning =
    "quelle.pdf was encrypted. The combined output is not password-protected.";

  assert.match(
    localiseMergeWarnings([protectedWarning], t("de-DE"), number)[0],
    /AES-256/u
  );
  assert.match(
    localiseMergeWarnings([sourceWarning], t("tr-TR"), number)[0],
    /quelle\.pdf/u
  );
  assert.equal(
    mergeActionErrorTranslationKey("cancel-failed"),
    "merge.error.cancel"
  );
});

test("keeps Merge display state typed and exception-free", async () => {
  const studio = await readFile(
    new URL("../src/MergeStudio.tsx", import.meta.url),
    "utf8"
  );

  assert.match(studio, /localisePdfJobFailure\(mergeJob\.job, t\)/u);
  assert.match(studio, /localiseMergeWarnings\(status\.result\.warnings/u);
  assert.doesNotMatch(
    studio,
    /reason\.message|String\(reason\)|job\.error \|\||function errorMessage/u
  );
});
