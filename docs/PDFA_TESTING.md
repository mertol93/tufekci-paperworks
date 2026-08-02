# PDF Standards and PDF/A Corpus Testing

Tüfekci Paperworks supports explicit conversion to PDF/A-1b, PDF/A-2b, and PDF/A-3b,
plus validation-only reports for those profiles. The same local standards workflow now
provides formal veraPDF validation for PDF/UA-1 and PDF/UA-2, and clearly labelled
built-in structural preflight for PDF/X-1a:2001, PDF/X-3:2002, and PDF/X-4. PDF/UA or
PDF/X conversion and independent PDF/X certification are not supported. These workflows
remain experimental until their first tagged Windows, macOS, and Linux evidence sets pass.

## Engine Contract

Conversion requires:

- OCRmyPDF 17 or newer;
- Ghostscript;
- veraPDF 1.30.2 or a compatible CLI for PDF/A and PDF/UA; and
- Tesseract plus the selected language pack when searchable OCR is enabled.

Password-protected inputs additionally need QPDF for the private temporary unlock.
PDF/A output itself is never encrypted. Set `PAPERWORKS_VERAPDF` to the absolute
`verapdf` or `verapdf.bat` launcher when veraPDF is not on `PATH`.

The application does not download an engine during document work. Readiness probes are
bounded and optional: a missing archival engine disables only the dependent controls.

PDF/UA checks select veraPDF's exact `ua1` or `ua2` built-in flavour. They validate
machine-checkable structures but do not repair tags or replace hands-on testing with
assistive technology. The current boundary requires an exact unprotected source because
private decryption rewrites the bytes under review.

PDF/X preflight needs no external engine. Its bounded structural checks cover declared
profile IDs, trapping, encryption, GTS_PDFX output intents, ICC structure, embedded
fonts, object integrity, page boxes, JavaScript, forms, attachments, external content,
non-printing media, and transfer curves. A passing preflight is not ISO 15930
certification, colourimetric proofing, or print-service approval.

## Public Gate

Generate the synthetic OCR corpus and run every archival engine check with:

```bash
npm run qa:pdfa-engine
```

The command performs these checks against `qa-fixtures/ocr-corpus/english.png`:

1. Build one image-only, one-page source PDF without OCR and require veraPDF to report
   the original as non-conforming.
2. Convert separate copies to PDF/A-1b, PDF/A-2b, and PDF/A-3b with English OCR and
   deskew enabled.
3. Reopen every candidate, require one page, require no encryption, and require one
   page with a verified searchable text layer.
4. Validate each candidate with the exact matching veraPDF flavour and require zero
   failed rules and zero failed checks.
5. Run the real built-in-style PDF/A-2b Batch Recipe through OCR, deskew, archival
   conversion, independent validation, and publication.

The ordinary Rust suite leaves these engine tests ignored. To invoke them directly:

```bash
cargo test --manifest-path src-tauri/Cargo.toml live_pdfa -- --ignored --nocapture --test-threads=1
```

Set `PAPERWORKS_OCR_CORPUS` to the generated corpus directory first.

## Retained Evidence

Successful runs write
`qa-fixtures/pdfa-engine/pdfa-engine-report-<platform>-<architecture>.json`.
The report has a closed schema containing only:

- product, release, platform, and architecture identity;
- the public corpus manifest SHA-256 digest;
- OCRmyPDF, Ghostscript, Tesseract, and veraPDF versions;
- supported profile identifiers, page counts, searchable-page counts, compliance
  verdicts, and failed-rule/check totals; and
- the PDF/A-2b batch output count and independent-validation result.

It contains no PDF or image bytes, recognised text, password, local path, filename,
raw engine output, or user-document rule examples. The marker parser rejects duplicate,
missing, contradictory, oversized, or unknown evidence.

## Release Installation

The tagged release workflow runs the gate on Ubuntu, macOS, and Windows. It installs
OCRmyPDF 17.8.1 in an isolated Python 3.12 environment on every runner. This avoids the
older OCRmyPDF 15 package supplied by Ubuntu 24.04, which does not provide the required
processing-only engine mode. The workflow installs veraPDF 1.30.2 from the immutable
official archive and requires SHA-256
`6CC6341CB1AF644044054B81F00A6590A7918ABB18F762243DE115258BCAD838` before unattended
installation. Windows uses the official 64-bit Ghostscript 10.07.1 installer and
requires SHA-256
`3A4C28D0AAC47AA7CCCD35A5932C55110376E9DBD966898DDE388B7FABA444A4` before execution.
Linux and macOS use their runner package managers for Ghostscript and Tesseract. Every
runner records the observed engine versions in the evidence report.

The release job retains each platform report and attaches the combined evidence set to
the draft release. A missing report or any failed profile blocks package publication.

## Safety Boundaries

- Source PDFs are fingerprinted before work and again immediately before publication.
- Destinations must not exist and sources are never overwritten.
- Conversion candidates stay in ownership-token-protected private workspaces and are
  published atomically only after reopening and independent validation.
- Cancellation terminates OCRmyPDF, Ghostscript, and veraPDF process trees and prevents
  publication.
- Conversion is a structural rewrite and cannot preserve an existing certificate
  signature; signed sources require explicit acknowledgement.
- A protected source may be validated through a private decrypted copy, but the
  original verdict remains non-conforming because PDF/A forbids encryption.
- A veraPDF pass means conformance to the selected PDF/A or PDF/UA profile under that
  validator. PDF/UA still requires semantic and assistive-technology review.
- A PDF/X preflight pass means only that the disclosed bounded structural checks passed;
  it is not a conformance verdict, colour proof, or guarantee of print suitability.
- No standards report is a claim of malware freedom, legal admissibility, or fitness
  for a particular institutional policy.
