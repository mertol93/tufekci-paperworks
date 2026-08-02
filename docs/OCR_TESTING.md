# OCR Corpus Testing

Tüfekci Paperworks has a generated, redistributable OCR corpus and an ignored native
test for the complete image-to-searchable-PDF path. The gate checks engine and
language readiness, creates a searchable PDF, reopens its text layer, verifies
page-level coverage, measures expected token recall, and requires engine progress to
reach 100% inside the overall scan interval of 76-90%. It never uploads a fixture,
stores recognised document text in its report, or prints that text to the test log.

## Public Corpus

Generate and validate the corpus with:

```bash
npm run qa:ocr-corpus
```

The command uses the bundled Liberation Sans standard font and fixed drawing rules to
create these UTF-8 pairs under `qa-fixtures/ocr-corpus`:

```text
english.png
english.txt
turkish.png
turkish.txt
rotated.png
rotated.txt
noisy.png
noisy.txt
```

`english` contains UK English vocabulary; `turkish` exercises `ç`, `ğ`, `ı`, `İ`,
`ö`, `ş`, and `ü`; `rotated` is physically rotated by 90 degrees; and `noisy`
contains deterministic low contrast, speckling, skew, a crease, and uneven lighting.
The manifest records dimensions, language, rotation, byte counts and SHA-256 digests.
The validator enforces plain filenames, ordinary files, strict UTF-8/LF text,
bounded sizes, expected language and recall contracts, PNG dimensions, and
non-blank pixel evidence. Generated fixtures and reports are ignored by Git.

The minimum unique-token recalls are 85% for English, 75% for Turkish, 80% for
rotated English, and 65% for noisy English. Changes to those thresholds must be
supported by recorded corpus results, not by weakening a failing release gate.

## Engine Gate

Install OCRmyPDF, Tesseract, and the `eng`, `tur`, and orientation (`osd`) language
data, then run:

```bash
npm run qa:ocr-engine
```

This regenerates and validates the public corpus before invoking the ignored native
scan-export, batch-publication, and standalone-publication tests. The publication case
first creates an image-only PDF, runs both the searchable archive recipe and the direct
Recognise Text workflow through the real local engine, then reopens both published
outputs and verifies page preservation and searchable coverage. A successful run writes
`ocr-engine-report-<platform>-<architecture>.json`. The path-free report contains
the Paperworks release version, OCRmyPDF and Tesseract versions, required language-data
identifiers, corpus manifest digest, observed recall for each case, searchable-page
count, and progress verification. A strict schema rejects unknown fields, including
recognised fixture text or local paths.

For direct native-test debugging, generate the corpus first and set
`PAPERWORKS_OCR_CORPUS` to its directory:

PowerShell:

```powershell
$env:PAPERWORKS_OCR_CORPUS = "C:\path\to\PDF_Editor\qa-fixtures\ocr-corpus"
cargo test --manifest-path src-tauri/Cargo.toml live_ocr_corpus -- --ignored --nocapture
```

macOS or Linux:

```bash
PAPERWORKS_OCR_CORPUS=/path/to/PDF_Editor/qa-fixtures/ocr-corpus \
  cargo test --manifest-path src-tauri/Cargo.toml live_ocr_corpus -- --ignored --nocapture
```

The `live_ocr_corpus` filter selects both the four-case recognition/recall test and the
English batch-and-standalone publication test. The evidence report retains only the four
bounded recall case records; the publication test is a mandatory pass/fail gate and
emits no document text.

## Platform Installation

Follow the current [OCRmyPDF installation guide](https://ocrmypdf.readthedocs.io/en/latest/installation.html)
and [Tesseract documentation](https://github.com/tesseract-ocr/tesseract) rather than
assuming Python packages alone provide native OCR dependencies.

On Ubuntu or Debian, install the distribution OCRmyPDF package and English, Turkish,
and orientation data. The tagged-release workflow uses:

```bash
sudo apt-get install ocrmypdf tesseract-ocr-eng tesseract-ocr-tur tesseract-ocr-osd
```

On macOS with Homebrew:

```bash
brew install ocrmypdf tesseract-lang
```

Tagged draft releases run the engine-backed corpus on Ubuntu, macOS, and Windows and
retain all three reports. Every release runner creates an isolated Python 3.12
environment with OCRmyPDF 17.8.1; this also provides the version required by the
PDF/A processing-only path. The pinned native Windows evidence setup additionally
installs the OCRmyPDF-recommended UB Mannheim Tesseract 5.4 package and downloads the
official Apache-2.0 Turkish model only after enforcing its recorded SHA-256 digest. It
copies Tesseract's standard configuration and orientation data into the same temporary
data directory. The model URL is pinned to tessdata commit
`ced78752cc61322fb554c280d13360b35b8684e4` and its SHA-256 is
`489B9504E80D7184ED1AC9A1976647884EE71149DA231FF3C2C1DC15370F2F3D`.

The Windows evidence job uses OCRmyPDF 17's `pypdfium2` rasteriser and requests plain
PDF output, so Ghostscript is not needed for this corpus. This does not claim PDF/A
conversion support. The same setup passed locally on Windows x64 with 100% observed
unique-token recall, one verified searchable page, and completed engine progress for
all four cases. A tagged release must reproduce the report on every target runner.

## Missing Tools

Ordinary tests use fixed machine, Rich, tqdm, malformed, chunked, and oversized
progress data and do not require OCR commands. A process-level test also confirms
that a streamed progress update can trigger prompt cancellation. The engine gate
fails with an actionable diagnostic when a command, required language datum, expected
case marker, progress result, searchable layer, or recall threshold is unavailable.
