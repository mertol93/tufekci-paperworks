# Interface Localisation

Tüfekci Paperworks has one explicit local interface-locale contract:

| Locale | Release name | Role |
| --- | --- | --- |
| `en-GB` | English (UK) | Canonical source, default, and fallback |
| `en-US` | English (US) | Supported release locale |
| `tr-TR` | Türkçe (Türkiye) | Supported release locale |
| `de-DE` | Deutsch (Deutschland) | Supported release locale |

The selected identifier is the only localisation preference stored in browser-backed
application storage. Changing it does not upload a document, filename, signature,
annotation, or any other document data. The root HTML `lang` value changes with the
interface so assistive technology receives the current language.

## Current Boundary

The typed runtime, persisted selector, workflow navigation, editor tools, shared open,
drop, loading, recovery, edit-safety and operation states, complete page organiser,
Merge workspace, output-protection fields, shared PDF-job controls, complete
visual-signature/initials workflow, standalone searchable OCR, OCR confidence review,
image-scan settings and results, scan clean-up preview, and connected-scanner discovery
and capture surface, plus complete print settings, Split, password protection,
Compression, local Activity, and signed-Update settings, validation, progress,
cancellation, preview, privacy, accessibility, and system-dialogue states are localised
in all four catalogues. Mobile core readiness, unavailable desktop-engine and connected-
scanner explanations, and App Store-managed update states are also present in every
catalogue. The shared protected-PDF opening dialogue is also complete,
including local-only password guidance, an announced incorrect-password state, retry
without focus loss, cancellation, and path-free changed, malformed, password, read, and
unknown-file outcomes. Document Health, Privacy Cleaner, PDF Comparison, Page Finish,
Annotation, Forms, Page Content, and Permanent Redaction are also complete across their
controls, validation, findings, warnings, preview, results, and accessibility names.
Full-page and thumbnail canvas names, rendering progress and failures, display-only
annotation layers, inert control titles, and progressive document-search outcomes are
complete too. Search normalisation follows the selected locale rather than a fixed
English locale, so Turkish dotted and dotless I retain their intended distinction.
PDF Standards and Batch Recipes are complete across profile and recipe selection,
engine readiness, OCR and archive options, source inspection, scanner hand-offs,
validation reports, recipe storage, per-file outcomes, and accessibility names.
Bookmarks are complete across outline editing, heading analysis, generated linked A4
contents, validation, output protection, results, warnings, and accessibility names.
Certificate Signatures is complete across PKCS#12 signing, visible-field placement,
timestamps, PAdES options, trust-root selection, existing-signature review, bounded
field metadata, validation, cancellation, results, warnings, and accessibility names.
Stable organiser, Merge, OCR review, searchable OCR, scan
export, scan preview, scanner-capture, split, protection, compression,
compression-preview, health, privacy, privacy-inspection, finishing, and
finishing-inspection, annotation, annotation-inspection, forms, form-inspection,
content, content-inspection, redaction, redaction-inspection, archive, batch,
batch-inspection, bookmarks, bookmark-inspection, certificate, and
certificate-validation
stages are translated at the interface boundary. Organiser, split,
compression, privacy, health, comparison, page-finishing, annotation, form,
content-editing, redaction, standards, batch, bookmark, and certificate outcomes map from bounded
codes or exact allow-lists and unknown text becomes one generic translated warning;
raw job, engine, driver, storage, and update detail is retained only in bounded
diagnostics. Shared job-history and live-status failures are stored as two content-free
codes. The visual-mark vault accepts only eight native outcome codes, including one
deliberately ambiguous incorrect-passphrase-or-altered-entry result; native filesystem
and cryptography prose never crosses into display state. PDF.js and native range failures are reduced to six stable opening codes
before React stores or presents them; exception messages and native paths are discarded.
PDF text extraction uses one stable content-free failure code and removes a rejected
cache entry so the user can retry without retaining parser prose.

Locale-specific interpolation, dates, numbers, natural-language lists, and document
sizes are active on the migrated surface. The native-shell acceptance suite switches
Turkish and German, checks translated navigation, page actions, thumbnail, editor, and
live page-canvas accessibility names, Merge, searchable OCR, scan settings, connected-scanner text,
Split, Protect, Compression, Document Health, Privacy Cleaner, PDF Comparison, Page
Finish, Annotation, Forms, Page Content, Permanent Redaction, PDF Standards, Batch
Recipes, Bookmarks, and Certificate Signatures; opens translated Activity and signed-Update text; verifies the stored locale
and root `lang`;
proves that an already-open document adopts German number formatting,
rejects and accepts the generated AES-256 fixture through the Turkish password dialogue,
reopens the same prompt in German and cancels it safely, captures the organiser and
Turkish release surfaces, then restores British English. The
complete visual-signature journey currently runs in British English;
packaged switching evidence for that workflow remains part of the three-platform
release matrix.

This is an implementation foundation, not complete product localisation. Remaining
native outcomes, installer strings, accessibility names, release metadata, and user
documentation have not all been migrated. The application
must remain pre-alpha and must not describe any of the four locales as a complete
release language until that inventory is zero and the packaged matrix below passes.

## Catalogue Contract

- Every key originates in `src/locales/en-GB.ts`.
- The other catalogues must contain the exact same key set.
- Interpolation placeholders must match the canonical source exactly.
- Blank values, malformed UTF-8, implicit browser-language selection, and silent
  fallback to a partly translated locale are rejected.
- Missing or unsupported stored values resolve to `en-GB`.
- Brand names, file extensions, standards identifiers, cryptographic names, and page
  range tokens remain unchanged where translation would alter their meaning.
- American English uses American spelling where it differs; it is not an alias label
  placed over the British catalogue.

## Migration Rules

1. Move complete user journeys, including labels, tooltips, accessibility names,
   validation, empty states, progress, cancellation, and success text.
2. Do not concatenate translated sentence fragments where word order or grammar can
   differ. Use complete keyed messages with named interpolation values.
3. Use `Intl` with the selected locale for numbers, dates, and times. Do not use locale
   formatting for machine protocols, page-range syntax, hashes, or evidence files.
4. Replace native free-text stages and errors with stable codes plus bounded structured
   values before claiming native diagnostics are localised. Raw engine and driver text
   must not be presented as a translated application message.
5. Review controls at German expansion lengths and with Turkish characters at desktop
   and narrow widths. Text must wrap without clipping or overlap.
6. Keep the British source in plain, precise UK English and translate meaning rather
   than word order.

## Release Evidence

Before localisation can be marked complete, retain on Windows, macOS, and Linux:

- catalogue parity, placeholder, fallback, and locale-formatting tests;
- native GUI switching and restart-persistence evidence for every locale;
- keyboard and screen-reader checks with the correct accessible names and language;
- screenshots of every workflow at desktop and narrow widths, including long errors;
- save-and-reopen tests proving locale changes do not alter PDF content or metadata;
- translated installer, permission, update, recovery, encryption, signature, OCR,
  scanner, privacy, and destructive-action review;
- reviewed user documentation for all four locales; and
- human linguistic review by fluent British English, American English, Turkish, and
  German reviewers.

Machine tests prove structural completeness, not linguistic quality. A catalogue that
passes key parity but contains awkward, misleading, or untranslated copy is not ready.

For iPhone and iPad, the same release claim additionally requires reviewed compact and
regular-width layouts in portrait, landscape, and iPad split view; safe-area and Dynamic
Type expansion; VoiceOver and hardware-keyboard names; App Store update wording; and
clear, accurate explanations for every desktop-only engine. The current source and
catalogue contracts do not replace simulator, physical-device, or fluent human review.
