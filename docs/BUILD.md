# Build Notes

## Development Toolchain

Install these first:

- Node.js 22.13 or newer.
- Rust stable through `rustup`.
- Platform-specific Tauri prerequisites.

Then run:

```bash
npm install
npm run desktop:dev
```

For a production build:

```bash
npm run desktop:build
```

Generate the local PDF rendering fixtures when testing PDF.js layers:

```bash
cargo run --manifest-path src-tauri/Cargo.toml --example generate_pdf_fixtures -- qa-fixtures
```

Run the isolated native Tauri acceptance suite with:

```bash
npm run e2e
```

It verifies shell accessibility, real Rust-backed PDF opening, PDF.js rendering and
search, page drag reordering and structural operations, signature preparation,
encrypted-vault save/wrong-passphrase retry/deletion, placement and locking, plus Turkish reject/accept and German cancellation for an
AES-256 password prompt and translated Turkish/German live-canvas names. The E2E Cargo
feature, dedicated application identity, Linux
Xvfb command, production exclusion checks, and path-free release matrix are documented
in [native end-to-end testing](E2E_TESTING.md).

Generate and validate the public synthetic OCR corpus with:

```bash
npm run qa:ocr-corpus
```

The CI workflow in `.github/workflows/ci.yml` checks the frontend, Tauri backend, and
native application workflows on Windows, macOS, and Linux. Tagged builds create draft platform bundles and
verify their native metadata, package structure, publisher identity, timestamp, and
notarisation state. The credential-gated signing implementation is present; the first
real certificate-backed run, Windows reputation review, representative installation,
hardware and engine corpora, and retained package evidence remain release gates. The
separate `.github/workflows/apple-mobile.yml` workflow runs on macOS 15, generates the
ignored Xcode project, compiles the unsigned arm64 iOS simulator application, verifies
its iPhone/iPad metadata, and retains a hashed archive. It is a compile gate, not a
signed device or App Store release. The
exact public distinction between complete,
experimental, and unavailable workflows is maintained in
[feature status](FEATURE_STATUS.md).

Ordinary local builds deliberately omit the signed updater endpoint and public key and
may produce unsigned packages. Credential-backed tagged builds run only through the
protected release environment. Publisher setup is documented below; channel promotion,
rollback, and updater-key recovery are documented in [signed application updates](UPDATES.md).

## Document Tools

The backend combines an embedded structural engine with optional mature external tools:

- `lopdf` is compiled into the application for page reordering, deletion,
  duplication, rotation, paper-sized blank pages, selected-range import, merging,
  splitting, scan PDF creation, and verified structural export.
- Rust `image` codecs are compiled in for PNG, JPEG, TIFF, WebP, BMP, GIF, and
  portable-anymap input.
- `qpdf` provides AES-256 password protection and permissions. QPDF 11 or newer
  is recommended for those optional protection workflows.
- `magick` from ImageMagick optionally normalises formats not handled by the embedded
  codecs, including HEIC, HEIF, and AVIF where the local ImageMagick build supports them.
- `ocrmypdf` for searchable OCR output and reviewed vocabulary hints.
- `tesseract` for OCR recognition, installed language packs, and confidence TSV.
- `ghostscript` for OCRmyPDF's explicit PDF/A-1, PDF/A-2, and PDF/A-3 output modes.
- `verapdf` for independent PDF/A-1b, PDF/A-2b, PDF/A-3b, PDF/UA-1, and PDF/UA-2
  conformance validation. PDF/X structural preflight is built in and does not use
  veraPDF.
  Set `PAPERWORKS_VERAPDF` to an absolute `verapdf` or `verapdf.bat` launcher when it
  is not on `PATH`.
- `pyhanko` from the `pyHanko` and `pyhanko-cli` Python packages for digital signing
  and signature validation. Tagged evidence currently pins pyHanko 0.36.2 and
  pyhanko-cli 0.4.2; another reviewed compatible pair must still expose PKCS#12
  `--passfile` support.

The app probes these tools at startup and explains what is missing instead of
failing silently. OCR performs a second bounded readiness check against the selected
installed language before a scan job starts.

The ordinary test suite does not need OCR commands. With OCRmyPDF, Tesseract, and the
`eng`, `tur`, and `osd` language data installed, run the generated engine-backed
English, Turkish, rotated, and noisy corpus with:

```bash
npm run qa:ocr-engine
```

The command generates and validates the public corpus before the ignored native test,
then writes a path-free engine, searchable-layer, progress, and token-recall report.
Tagged draft releases require this evidence on Ubuntu, macOS, and Windows. Installation
details, the pinned OCRmyPDF 17.8.1 environments, native Windows dependencies, direct
native-test commands, safety limits, and thresholds are documented in
[OCR corpus testing](OCR_TESTING.md).

With OCRmyPDF 17 or newer, Ghostscript, veraPDF, Tesseract English data, and the public
OCR corpus available, run the archival engine gate with:

```bash
npm run qa:pdfa-engine
```

This converts one image-only fixture independently to PDF/A-1b, PDF/A-2b, and PDF/A-3b,
checks searchable-page coverage and matching veraPDF verdicts, and then runs the real
PDF/A-2b Batch Recipe. The retained report is closed-schema and excludes paths and
recognised text. Tagged releases run the gate on Windows, macOS, and Linux. The pinned
validator installation, engine evidence, native commands, and safety limits are in
[PDF/A corpus testing](PDFA_TESTING.md).

Certificate adapter unit and structural tests do not need pyHanko. With pyHanko,
pyhanko-cli, and OpenSSL installed, generate a disposable identity and run the real
visible/invisible, encrypted-input, reopening, integrity, and trust gate with:

```bash
npm run qa:certificate-engine
```

The command removes the generated identity and passwords and retains only a closed,
path-free report. Tagged releases set `PAPERWORKS_REQUIRE_CERTIFICATE_TIMESTAMP=1` and
provide `PAPERWORKS_TEST_TSA_URL`, so missing RFC 3161 and PAdES evidence fails the
release job. The custom-fixture contract is documented in
[certificate corpus testing](CERTIFICATE_TESTING.md). Never commit a test PKCS#12
identity or its passphrase.

PDF.js is an npm dependency rather than a system tool. Vite bundles its worker
and copies its CMaps, standard fonts, colour profiles, and WASM helpers into the
application so rendering remains fully local and offline. It also copies the
annotation icon set used by the display-only annotation and AcroForm appearance
layer. Embedded scripts and link navigation remain disabled, and form controls are
rendered inert; this layer is not a form-filling feature.

Desktop PDF intake gives PDF.js an initial 64 KiB sample and fulfils later requests
through the Tauri command layer in 64 KiB chunks. The backend rejects any individual
range larger than 1 MiB, PDFs larger than 64 GiB, and reads after the source size or
available modification time changes. This avoids copying an entire local PDF into
JavaScript before loading starts. PDF.js may still request every range for a
non-linearised or otherwise non-optimised document. Browser-selected files use an
in-memory `ArrayBuffer` because the browser does not expose a stable local path.

The shared opening-password dialogue keeps its value in session memory only, marks the
field as autocomplete-off, rejects line separators and null bytes, and caps input at
1,024 UTF-8 bytes. It remains mounted while PDF.js validates a retry so focus and the
announced incorrect-password state stay intact. Whole-file and native range failures
are reduced to stable cancelled, changed, invalid, password, unreadable, or unknown
codes before the interface stores them; raw exception messages and operating-system
paths are not retained in React state.

Progressive document search normalises compatibility characters and lowercases with the
selected interface locale, rather than a fixed English locale. A page-text extraction
failure is retained only as the stable `text-unavailable` code; its rejected cache entry
is removed so a later query can retry. Full-page and thumbnail canvas labels, rendering
status, display-only annotation controls, and the single render-failure state use the
same typed catalogue as the rest of the interface.

The Print workflow uses the same loaded PDF.js documents and needs no separate print
engine. It prepares selected pages locally at 150 or 300 dpi, then calls the webview's
system print dialogue: WebView2 on Windows, WKWebView on macOS, and WebKitGTK on Linux.
The operating system supplies printer discovery and printer-specific controls. See
[printing testing](PRINTING_TESTING.md) for the physical and PDF-target release matrix;
the automated E2E bridge simulates only the final dialogue request so CI never sends an
unexpected physical print job.

The fixture generator creates `qa-fixtures/annotations-and-form.pdf` for checking
a note annotation, an external link appearance, and a populated AcroForm widget. It
also creates a 320-page `qa-fixtures/range-loading.pdf` for exercising bounded local
loads, plus `qa-fixtures/accessibility-review.pdf` with a tagged structure tree, a
custom RoleMap Figure, and one deliberately missing alternative description.

Run the complete generated rendering gate with:

```bash
npm run qa:rendering-corpus
```

The command also generates AES-256 encrypted, certificate-structure, image-only scan,
CJK and RTL Type3, unusual-page-size, and malformed fixtures. PDF.js must challenge for
the test password, reject an incorrect password, decode every page's operator list,
preserve expected searchable text and RTL direction, expose expected annotations and
geometry, raster representative pages through the cross-platform native canvas backend,
meet bounded non-white-pixel thresholds, and reject the malformed file. The 320-page
fixture renders its first, middle, and final pages. A path-free
`rendering-report-<platform>-<architecture>.json` records hashes and bounded pixel
evidence. CI runs this on Windows, macOS, and Linux and tagged builds retain the reports.
Every fixture is synthetic and generated from repository source. The generated
directory is ignored by Git and is safe to recreate locally.

The synthetic OCR fixture gate is separate:

```bash
npm run qa:ocr-corpus
```

It generates clean UK English, Turkish, physically rotated, and noisy A4-scale PNGs
from the bundled Liberation Sans font, plus exact UTF-8 expected text. The checker
validates hashes, dimensions, orientation contracts, language and recall thresholds,
UK English and Turkish coverage, bounded decoding, and non-blank pixels. CI runs this
on Windows, macOS, and Linux and retains
`ocr-corpus-report-<platform>-<architecture>.json`.

Password protection and password removal are already connected to QPDF. The
adapter sends its arguments through standard input so passwords do not appear in
the spawned process command line. They run through the shared native FIFO manager
with visible stages, status retry, same-process reattachment, and queued or running
cancellation. Standard output and error are drained concurrently with a 1 MiB retained
limit; QPDF is stopped after a 30-minute timeout or cancellation. The reviewed source
size and modification time are checked before work and after output verification.
Output is written to a temporary file, checked, and then published under a new filename.
Every app-owned temporary PDF candidate is also registered in a private app-data lease
registry before work begins. The same lease mechanism covers isolated batch folders,
one-use certificate passfiles, OCR user-word hints, the embedded OCR progress plug-in,
and scan-normalisation rasters.
Each live owner holds an exclusive operating-system lock. On the next process start,
the backend scans at most 4,096 registry entries and removes only unlocked targets with
an exact app-owned filename, unchanged canonical parent, expected ordinary file type,
and, for a recursive batch removal, the matching ownership token. It rejects links,
Windows reparse points, malformed or oversized records, unknown fields, and future
timestamps. Aggregate clean-up status contains no target paths.

Active shared jobs use a separate `active-pdf-jobs` journal in the per-user
application-data directory. Job acceptance creates and flushes a strict create-new
record and holds its matching operating-system lock for the job lifetime. The record
contains only schema version, opaque entry ID, workflow kind, and start time. It never
contains a job request, source or destination path, filename, password, OCR hint,
signature, page raster, document data, stage, error, progress, or result.

Start-up examines at most 512 entries, skips locks held by another live application
instance, and claims only unlocked or orphaned records. Unknown fields, malformed or
oversized records, links, Windows reparse points, mismatched identifiers, and
future-dated entries are rejected. Up to the newest 32 valid entries are removed from
the journal and exposed once as failed snapshots with the stage `Previous job
interrupted`. They are deliberately not resumed or replayed. The interface tells the
user to review the current workflow and start a new request. Publication users must
also check the chosen destination because the previous process may have stopped in the
narrow interval after create-new publication but before reporting success. Normal
success, failure, or cancellation retires the record.
Unix journal directories and files use user-only modes; Windows inherits the current
user's application-data ACL.

Connected-scanner captures do not use immediate temporary leases because they support
session recovery. They remain confined to their existing app-owned capture root and are
pruned by the separate seven-day retention policy. An ordinary capture failure or
cancellation removes its current private workspace. A process interruption may leave
pages there, but its recovery journal contains neither their paths nor the scanner
request and never replays the acquisition.

Bookmark review runs through a distinct shared read-only job. The source path and
password stay transient, bounded named-destination and outline traversal report
cancellable progress, public failures are content-free, and the source fingerprint is
rechecked before the typed tree is delivered. Its synchronous entry point remains
available only to native tests and controlled worker dispatch.
Bookmark publication also uses this manager. It writes and verifies an unencrypted
prepared copy first, optionally applies the same bounded QPDF AES-256 adapter, decrypts
the protected candidate to repeat bookmark, page-count, and form-preservation checks,
rechecks the source fingerprint, and only then publishes the create-new destination.
Annotation review runs first through its own shared read-only job. The source path and
password remain transient, page-by-page inspection reports cancellable progress, public
failures are content-free, and exact source size and modification time are checked again
before the typed report is delivered. Inspection bounds editable extraction to 500
entries per page and 2,000 overall, returns stable source and PDF.js viewer identities
only for self-contained representable indirect annotations, and accounts for every
other source item as read-only. The direct inspection function is retained only for
native tests and worker dispatch, not registered as a Tauri command.
Annotation publication follows the same controlled sequence. It validates separate
new, updated, and removed sets; rejects stale, invented, duplicated, cross-page, or
type-changing source identities; preserves unsupported annotations; and verifies exact
per-page totals plus stable replacement markers, subtypes, appearances, image resources,
pages, and forms. It optionally repeats those checks after decrypting an AES-256
candidate and retains only content-free errors in public job history.
Page Finish review runs through a distinct shared read-only job. The source path and
password stay transient, bounded page-geometry and annotation traversal report
cancellable progress, public failures are content-free, and the source fingerprint is
rechecked before the typed workspace model is delivered. Its synchronous entry point
remains available only to native tests and controlled worker dispatch.
Page Finish publication also uses the shared manager. It checkpoints selected-page and
verification loops, rechecks the source immediately before publication, and verifies page
boxes, forms, bookmarks, annotation counts, operation markers, and generated mark layers
before and, when selected, after QPDF AES-256 protection and decryption. Public failures
do not retain watermark, header, footer, or Bates text.
Form review runs through a distinct shared read-only job. The source path and password
stay transient, bounded page/widget discovery and recursive field parsing report
cancellable progress, public failures are content-free, and the source fingerprint is
rechecked before the typed field model is delivered. Its synchronous entry point is not
registered as a Tauri command.
Form publication follows the same prepare-protect-verify sequence. Its protected check
uses stable field names and exact values rather than indirect object numbers, verifies
remaining fields, appearances and flattened markers, and retains only content-free
failure diagnostics in public job history.

Visual signature flattening has no system dependency. The embedded exporter adds
the prepared transparent PNG and alpha mask to the chosen page, including rotated
pages, then reopens the output and checks that the signature resource is present.
The optional signed-copy lock additionally needs QPDF and applies a new opening
password, a distinct administrator password, AES-256 encryption, and no-change
reader permissions before a second verification pass.

The encrypted visual-signature library also has no system dependency. RustCrypto
implements Argon2id key derivation and AES-256-GCM authenticated encryption inside the
desktop process. Each versioned entry uses a random salt and nonce, encrypts its label
and image metadata with the PNG, and is written as a create-new file in the app-data
directory. Passphrases must contain at least 12 characters, are never saved, and have
no recovery path. Unix vault directories and files are created with user-only modes;
Windows access follows the current user's app-data ACLs.

Certificate-backed signing uses the optional pyHanko CLI. It accepts bounded `.p12`
or `.pfx` identities and up to 16 bounded trust certificates, writes the confirmed
PKCS#12 passphrase to a create-new one-use passfile, and never places the passphrase on
the command line. Remote timestamp URLs must use HTTPS and cannot contain credentials,
queries, or fragments. Visible fields are positioned inside the selected page for all
four standard page rotations with the integer coordinates required by the CLI;
invisible signatures omit the field rectangle.

The source PDF, signing identity, and trust roots are SHA-256 checked and copied into a
private registered workspace before pyHanko receives them. pyHanko writes an incremental
temporary PDF. The backend then reopens it, requires an increased signature count and a
signature byte range covering the final revision, runs bounded cryptographic
validation, and publishes a new destination only when integrity is confirmed. An
intact signature without a configured trusted chain is published with a clear warning
and reported as indeterminate rather than trusted. Standard password-protected PDFs can
be signed and validated directly. Their bounded password is supplied to pyHanko through
a fixed private standard-input bridge, never a process argument; signed output must
reopen with the same password and preserve the input encryption state.
Certificate signing runs through the shared native queue. Cancellation terminates and
awaits pyHanko before temporary files are released; the original PDF, identity, and
trust roots are hashed again immediately before publication; and retained job results omit paths, field
contents, certificate diagnostics, passphrases, trust roots, and timestamp-service data.
Existing-signature validation uses a distinct shared read-only job. It bounds recursive
signature inspection to 64 direct levels, two million nodes, 512 reported fields, and
1,024 UTF-8 bytes per field value; drains at most 512 KiB
of pyHanko output for a 150-second validation command, terminates the process tree on
cancellation, supports same-process and interrupted-state reattachment, and rechecks
the exact source and trust-root SHA-256 fingerprints before returning a report. Queued
snapshots and restart records omit local paths and PDF passwords; successful reports
scrub those paths and password values; retained failures are content-free.

Page organisation export does not require QPDF. It writes beside the selected
destination under an isolated temporary name, flushes and reopens the result,
checks the page tree and count, and only then publishes a create-new destination.
The graphical organiser runs through the shared native FIFO manager, fingerprints
every primary and imported source when opened, checks those fingerprints at worker
start and immediately before publication, and reports cancellable source/page/signature
and verification progress. Optional signed-copy locking still requires QPDF.

Selected-page import review uses a distinct shared read-only job before PDF.js opens
the source. It bounds the password, page-range expression and expanded selection,
reports staged progress, honours cancellation through certificate-structure traversal,
keeps the complete request out of public and interrupted snapshots, retains only
content-free failures, and rechecks the source fingerprint before returning the typed
selection. Its synchronous entry point is retained only for native tests and worker
dispatch.

Merge, import, split, and extraction also have no system dependency. Source PDFs
are renumbered into isolated object ranges, selected pages receive a new page tree,
and the output is reopened before publication. Split jobs prepare every requested
part first and remove any outputs created by the job if the complete set cannot be
published. Standalone merge and split run through the shared native FIFO manager,
report source/page/part progress, accept cancellation while queued or running, and
recheck every prepared source fingerprint before any verified output is published.
Retained failures exclude paths, passwords, and document content. When QPDF is
available, Merge can protect the already verified candidate with distinct AES-256
opening and administrator passwords; the backend requires encrypted output, decrypts
it, and repeats page-tree and fresh-catalogue verification before publication. Source
bookmark trees and AcroForm catalogues are reported but not combined in this first
merge engine. Split uses the same passwords for every requested part and prepares the
entire protected, decrypted-verification-checked set before publishing any output.
Their graphical planners use a dependency-free, bounded 100-step in-memory undo/redo
model. Merge snapshots omit source passwords, password edits are never operations, and
undo or redo therefore clears transient source passwords. Split history contains only
the page-group expression and resets when the selected source changes.

Direct organiser import uses the same embedded range parser and object-renumbering
rules. PDF.js keeps each imported source available for thumbnails, full-page preview,
and text search, while the Rust exporter resolves every planned page by source ID and
reopens the composed output before publication. Unencrypted imported sources can be
reopened from recovery drafts; imported passwords remain memory-only and therefore
disable recovery-draft saving for that plan.

The document health check is also embedded. It bounds decompressed page content to
32 MB per page or Form XObject during inspection and reads image dimensions and
compressed resource sizes without decoding full image rasters. It follows Form
XObject resources through at most 32 levels and 100,000 page-specific contexts, stops
cycles, and includes nested fonts, colour spaces, images, content streams, and missing
named resources in the report. ICC streams are decoded to at most 16 MB and their
header, declared size, component agreement, connection space, date, rendering intent,
reserved bytes, and at most 4,096 tag ranges are checked. This structural parser does
not replace antivirus, colourimetric analysis, or a dedicated PDF conformance validator.
Its accessibility section checks the document
information Title and DisplayDocTitle preference, catalogue Lang, MarkInfo,
StructTreeRoot, semantic structure elements, page StructParents links, Figure Alt
text including RoleMap aliases, and structured tab-order signals. It cannot prove
that tags are semantically correct or in a meaningful reading order, so the final
document still needs a screen-reader or accessibility-API review.
Document Health runs through the shared native scheduler. It reports staged progress
across object references, fonts, colour resources, accessibility, page content, and
nested Form XObjects; checks cancellation inside the bounded traversals; supports
same-process reattachment and one-time interrupted-state recovery; and rechecks the
source's exact modification time and size before delivering a report. Its queued public
snapshot contains neither source path nor password, and retained failures use a
content-free diagnostic.

Opened workspace PDFs and standalone Merge, Split, Protect, Privacy Cleaner,
Compress PDF, and Batch Recipes
sources receive a smaller edit-safety preflight. It checks the catalogue and object
graph for certificate signatures, AcroForm, and XFA without decoding page streams
or image resources. One debounced aggregate read-only job accepts up to 250 sources,
keeps every path and password transient, maps staged progress across each source,
bounds password, object-stream, object, and page work, cancels stale selections by
their exact job ID, scopes reattachment per consuming workflow, and returns ordered
path-free results or content-free per-source failures after final size/modification-time
checks. Rewrite controls remain unavailable
until the current-source check finishes, and the shared panel provides cancellation
and retry.
Certificate-signed sources require explicit acknowledgement in the interface, and
the Rust command repeats that guard before creating output.

The privacy cleaner is embedded as well. It rewrites a new PDF, removes only the
selected metadata, active-content, attachment, annotation/form, and thumbnail
structures, prunes unreachable objects, reopens the output, and checks each selected
category before publishing. Optional QPDF AES-256 output is applied only after this
prepared check; the backend decrypts the protected candidate, repeats the page-count
and selected-category checks, rechecks the inspected source fingerprint, and only then
publishes.

Privacy Inspection runs as a separate shared read-only job before cleaning. Its
bounded direct-object, optional-content, page-resource, annotation, and content-stream
walks report staged progress and honour cancellation. Source paths and passwords stay
in transient request memory, queued and interrupted snapshots omit both, retained
reports normalise the filename, failures are content-free, and the exact source size
and modification time are checked again before report delivery.

Compression uses the same prepare-protect-verify discipline. The embedded engine first
writes and reopens a smaller unencrypted candidate, checking page count plus preserved
form and bookmark structures. Optional QPDF AES-256 protection then produces a separate
candidate, which must be encrypted, decrypt with the new opening password, repeat the
same structural checks, and still be smaller than the source. The source size and
modification time are rechecked immediately before create-new publication, and retained
job failures do not include paths or passwords.
The dry-run compression preview is a separate shared read-only job. Its staged image
processing, sample encoding, cancellation, same-process reattachment, and one-time
interrupted-state recovery use the common scheduler. The source path and optional
password never enter public queued or restart records; retained failures are
content-free; the report filename is normalised; and exact source size and modification
time are checked before the volatile source/candidate PNG samples are returned.

Permanent redaction is embedded and has no system dependency. Source review first runs
through a distinct shared read-only job. Its path and password stay transient, bounded
page-geometry and annotation traversal report cancellable progress, public failures are
content-free, and the source fingerprint is rechecked before the typed workspace model
is delivered. The frontend then prepares bounded lossless page rasters, while the shared
native FIFO manager validates and rebuilds each marked page as one image-only page,
strips private and interactive structures, prunes unreachable objects, and reopens the
complete output. Native
progress covers page decoding, pixel flattening, compression, privacy cleaning,
writing, and verification. Cancellation is checked throughout and before publication;
the reviewed source size and modification time are checked when work starts and again
after verification immediately before the create-new destination is published.
When QPDF is available, Redaction Studio can apply distinct opening and administrator
passwords after the prepared candidate passes. The backend then requires encrypted
output, decrypts it in memory with the opening password, and repeats the image-only,
marker, page-geometry, searchable-text, and whole-document privacy checks before
publication.

Session recovery is embedded and has no external dependency. The app stores up to
three versioned snapshots in Tauri's per-user application-data directory. Each
snapshot is created under a new filename and flushed before older generations are
pruned, allowing the loader to skip an incomplete newest file. Snapshots contain
source paths and editor settings, but no passwords, signature images, document text,
or PDF/image bytes. Standalone Merge snapshots add only source identities, order, and
page-range text; Split adds only its source and page-group text. The schema bounds
sources and ranges, rejects unknown or secret-bearing fields, and checks that every
source still exists before restoration. Undo history, certificate acknowledgements,
output protection, and complete job requests are not recovered. The separate active-job
journal restores only a secret-free interrupted outcome. Unix snapshot files are
created with user-only permissions.

Scan export also has no mandatory system dependency for the embedded formats. It
auto-orients images from metadata, bounds decoding and dimensions, optionally detects
and crops page edges, corrects projective camera perspective, balances uneven lighting,
applies the selected colour mode, writes a temporary PDF, and reopens every page before
publishing it. Page detection is confidence-gated and preserves the original framing
when no reliable boundary is found. The 900-pixel before/after preview uses this same
pipeline through a distinct shared read-only job. It reports staged progress, cancels
stale settings, supports retry and reattachment, rechecks the source after encoding,
and keeps its JPEG bytes in volatile result state only. Searchable output additionally
needs OCRmyPDF and Tesseract; unsupported image formats additionally need ImageMagick.

OCR export checks both commands and every selected Tesseract language pack before
image decoding. Confidence review runs Tesseract against the same cleaned preview
raster, bounds TSV output to 8 MB and returned low-confidence words to 250, and does
not write recognised text to recovery or job history. Confidence review uses a
separate shared read-only job with staged preparation, cancellation of the complete
ImageMagick or Tesseract process tree, reattachment, source/settings-free snapshots,
content-free failures, and final source-image revalidation. Recognised words remain
only in its volatile typed result. Explicitly corrected words are stored only in memory
and supplied through a temporary user-word file. The finished PDF is reopened and its
searchable text is checked page by page before publication.

During OCRmyPDF execution, a one-use embedded progress plug-in emits content-free OCR
phase records to standard error. The backend drains the stream continuously, bounds
each candidate record to 16 KiB, accepts the exact machine marker or guarded Rich/tqdm
OCR records, and ignores malformed, unrelated, duplicate, or decreasing values. Engine
0–100% is mapped monotonically into overall scan progress 76–90%; searchable-text
verification begins at 91%. The plug-in file uses the private temporary lease registry
and user-only mode on Unix, and progress records are excluded from failure diagnostics.

When QPDF is available, scan export can protect the already verified image or OCR copy
with distinct opening and administrator passwords. It creates a separate encrypted
temporary candidate, decrypts it through the bounded loader, repeats page and embedded
image checks, and requires identical searchable-text coverage for OCR output. Every
source image's size and modification time is rechecked immediately before create-new
publication. Passwords are never placed in recovery data or public job snapshots.

The standalone Recognise Text workspace sends an existing PDF through the same local
OCRmyPDF and Tesseract engine path. It supports protected input, installed-language
choice, deskew, certificate acknowledgement, optional AES-256 output, page-level
searchable-text coverage, shared-job cancellation and retry, and final source checks
before create-new publication. Queued and failed snapshots contain neither paths nor
passwords.

Scan creation, standalone searchable OCR, scan clean-up preview, OCR confidence review, connected-scanner
acquisition, and OCR export share one
bounded native FIFO manager with compression,
privacy inspection and cleaning, batch source review and publication, merge, split, organiser export,
page-import review, bookmark, annotation, page-content review and publication,
Page Finish publication and review, form and certificate publication, permanent
redaction publication and review, password
protection, Document Health, certificate validation, and compression preview. The
frontend uses the generic start, get, list, and cancellation commands with typed,
kind-tagged snapshots for monotonic stage progress, retries temporary status failures,
and can reattach after a frontend refresh while the same application process is alive.
Scan export, standalone searchable OCR, certificate validation, compression preview, Document Health, scan
clean-up preview, OCR confidence review, and scanner capture are registered only
through that scheduler; their obsolete direct Tauri wrappers have been removed.
After a process restart, the frontend can instead reattach to a one-time interrupted
terminal snapshot; the old request is never retained or resumed. External tool output
is drained with a 1 MB retained diagnostic limit. Cancellation prevents temporary
output from being published or a stale health report from being returned. Windows Job
Objects and separate macOS/Linux process groups terminate the whole external-tool tree
on cancellation, timeout, monitor failure, parent exit, or wrapper drop; the immediate
child is awaited before temporary work is released.

Every scheduled workflow uses the same terminal-job panel after failure or
cancellation. Retry rebuilds work from the current interface state; publication
workflows ask for a new destination, while the fifteen read-only jobs rerun from current
inputs. Connected-scanner acquisition asks for a fresh capture after the current device,
source, feeder, and paper are reviewed. No password-bearing or document-bearing request
is retained for replay.
The copyable diagnostic is generated from job kind, opaque identifier, status, stage,
bounded progress and times, the content-free public error, and any status-connection
error. Typed result data is intentionally excluded.

The shared scheduler also writes one local Activity entry when a job first reaches
succeeded, failed, or cancelled. This is a separate schema from job snapshots: it
contains an opaque audit-entry ID, operation kind, terminal outcome, start and
completion times, and duration only. It excludes the job ID, progress, stage, error,
warning, filename, path, password, document content, and complete result.

Activity storage has no external dependency. Writers from separate application
processes take a short-lived operating-system lock, reload the newest valid generation,
and write a create-new snapshot. Up to three generations are retained for
interrupted-write fallback; each snapshot is bounded to 500 entries and 512 KiB.
Unknown fields, duplicate IDs, malformed timing, links, reparse points, and future
timestamps are rejected. Unix directories and files use user-only modes. The interface
can filter outcomes, export the path-free schema to a new JSON file, and clear all
older generations after a second confirmation.

Batch Recipe source review is one distinct shared read-only job for up to fifty unique
canonical PDFs. It maps the existing bounded Privacy Inspection stages across the
complete set, cancels the active traversal immediately, keeps every path and password
only in the transient request, and returns ordered path-free reports or content-free
per-file failures after individual final fingerprint checks. The direct Privacy
Inspection function is available only to native tests and controlled worker dispatch,
not as a separately registered Tauri command.

Batch Recipes inspect and fingerprint up to fifty PDFs and 20 GiB of source data, then
compose optional OCRmyPDF/Tesseract OCR and deskew with the embedded privacy cleaner and
compression exporter in an isolated workspace beside the chosen output folder. OCR
runs first, preserves the inspected page count, and reports pages without searchable
text. Protected OCR inputs require QPDF and are unlocked only inside that workspace.
The executor prepares and verifies every required copy before publication, never
replaces an existing destination, and attempts to remove any files published by an
incomplete set. Saved custom recipes contain only their name and processing settings;
paths, passwords, inspection findings, and output folders remain session-only. When
QPDF is available, one pair of distinct AES-256 passwords can protect the complete
output set after all other steps. PDF/A-1b/2b/3b is available as an unencrypted final
recipe step; connected-scanner intake is not available as a batch step.

Connected capture uses the same scan-review and PDF-export path after acquisition.
Windows uses the operating system's WIA COM interface through a fixed local script.
Linux uses the SANE `scanimage` command and emits bounded PNM pages. macOS uses a
packaged Objective-C ImageCaptureCore sidecar with an asynchronous run loop for device
discovery, functional-unit inspection, flatbed or feeder acquisition, duplex selection,
and file-based page transfer. Rust sends a versioned JSON request through standard input,
bounds retained output and execution time, and owns each adapter in a platform process
tree before applying the same capture-directory, page-count, file-size, extension, and
path-confinement checks used by the other adapters.

Capture starts through the shared typed scheduler. It reports monotonic stages, supports
queued and running cancellation, terminates and awaits the full adapter process tree,
and reattaches after a frontend refresh while the same application process is alive.
Queued and interrupted snapshots omit the device identifier and settings; retained
failures are content-free. Before successful delivery, Rust checks each captured page
against its validated size and modification time. The interface reads successful pages
sequentially into scan review and offers an explicit retry or discard if opening them
fails. Physical-device testing remains required on all platforms.

`npm run build:macos-scanner` is a no-op outside macOS. On macOS it uses Xcode's
`clang` and `lipo` to create Intel, Apple Silicon, and universal binaries under
`src-tauri/binaries/`. These generated files are ignored by Git. The npm `predev` and
`prebuild` hooks run the compiler automatically; `build.rs` also runs the same idempotent
builder before direct macOS Cargo builds. `tauri.macos.conf.json` bundles the matching
helper as an external binary. The macOS CI job requires both architectures in the
universal helper. The app currently targets macOS 12 or newer.

## Publisher Signing

Tagged builds use the protected GitHub Environment named `updater-signing`. Configure
required reviewers and restrict release-tag and environment administration. The
platform-specific steps receive only the credentials for their runner and invoke:

```bash
npm run release:platform-signing
```

The command validates the complete credential contract in memory and writes the
Git-ignored `src-tauri/platform-signing.release.conf.json`. That overlay contains only
public Tauri settings: the Windows SHA-1 certificate thumbprint, SHA-256 digest and HTTPS
timestamp URL; the macOS Developer ID identity and hardened-runtime setting; or an
explicit credential-free Linux bundle object. Certificate bytes, passwords, API private
keys, and temporary-keychain details are never written to the overlay or source archive.

For Windows, add these environment values:

- Variable `PAPERWORKS_WINDOWS_CERTIFICATE_THUMBPRINT`: the exact 40-character SHA-1
  thumbprint of the code-signing certificate.
- Variable `PAPERWORKS_WINDOWS_TIMESTAMP_URL`: an ordinary credential-free HTTPS
  timestamp-service URL.
- Secret `WINDOWS_CERTIFICATE`: the PFX bytes encoded as canonical single-line base64.
- Secret `WINDOWS_CERTIFICATE_PASSWORD`: the PFX password.

PowerShell can prepare the PFX value outside the repository:

```powershell
[Convert]::ToBase64String([IO.File]::ReadAllBytes('certificate.pfx'))
```

The Windows runner rejects a pre-existing certificate at that thumbprint, imports the
PFX as non-exportable into the current user's temporary build context, and requires that
the expected certificate is the only newly introduced private key, with Code Signing
extended-key usage, current validity, and more than 30 days of remaining validity.
Cleanup removes the temporary PFX and every certificate introduced by that import while
leaving pre-existing store entries untouched.

For macOS, add these environment values:

- Variable `PAPERWORKS_APPLE_TEAM_ID`: the exact ten-character Apple team identifier.
- Variable `APPLE_SIGNING_IDENTITY`: the full `Developer ID Application: ... (TEAMID)`
  identity for that team.
- Secrets `APPLE_CERTIFICATE` and `APPLE_CERTIFICATE_PASSWORD`: the Developer ID P12 as
  canonical single-line base64 and its password.
- Secret `KEYCHAIN_PASSWORD`: a unique temporary-keychain password of at least 16
  characters.
- Secrets `APPLE_API_ISSUER`, `APPLE_API_KEY`, and `APPLE_API_PRIVATE_KEY`: the App Store
  Connect issuer UUID, ten-character key identifier, and LF-formatted PKCS#8 private key.

OpenSSL can prepare the P12 value outside the repository:

```bash
openssl base64 -A -in certificate.p12
```

The macOS runner imports the identity into an ephemeral keychain, confirms exactly one
matching code-signing identity, and places the App Store Connect key in a mode-`600`
runner-temporary file. Tauri signs with a secure timestamp, submits the universal app
for notarisation, and staples the accepted ticket to the app. Cleanup restores the
original default keychain and search list and removes the keychain, certificate, and
API-key files even when the build fails.

Linux uses no publisher credential. The release workflow still generates an explicit
Linux overlay so a platform matrix entry cannot silently inherit another platform's
signing configuration.

## Source Release Gate

Audit the complete Git candidate tree before staging a release or publishing the
repository:

```bash
npm run release:source-check
```

The audit includes tracked and untracked, non-ignored candidates. It requires the
release, licence, lockfile, workflow, and feature-status documents; accepts only
reviewed UTF-8/LF source formats and application image assets; bounds file count,
individual size, and total size; and rejects generated directories, environment files,
private-key containers, credential signatures, and personal absolute home paths.
Deliberate negative-test values must be assembled at runtime rather than committed as a
copy-pasteable credential string.

After all checks pass, stage the exact intended tree and create its source release:

```bash
npm run release:source-archive
```

The command requires no unstaged or untracked source candidates, snapshots the Git index
without requiring a commit, reopens the ZIP, byte-compares every file with the audited
worktree, rechecks the index, and publishes create-new files under
`artifacts/source-release/`: a versioned source ZIP, a per-file JSON manifest, and a
SHA-256 checksum. It derives `SOURCE_DATE_EPOCH` from the current commit. Before the
repository's first commit, set `SOURCE_DATE_EPOCH` explicitly to a reviewed Unix
timestamp. Reusing that timestamp and index tree ID reproduces the archive.

## Release Metadata

Before release metadata is generated, each native build inspects its own package set:

- Windows requires one MSI and one NSIS installer. The verifier checks Compound File and
  PE/NSIS signatures, MSI Product metadata, the numeric WiX version, x64 identity,
  valid Authenticode state, the exact expected signer thumbprint, a trusted timestamp,
  byte size, and SHA-256.
- macOS requires one universal DMG. The verifier mounts it read-only, validates its
  application property list and identifier, checks Intel and Apple Silicon slices in the
  bundled Mach-O executable, requires the exact expected Developer ID team, a secure
  signing timestamp, successful Gatekeeper assessment, and a valid stapled notarisation
  ticket, then hashes both package and payload.
- Linux requires one AppImage, deb, and rpm. The verifier checks each native container,
  package-manager identity, release version, x64 architecture, desktop entry, executable
  ELF payload, and SHA-256. The executable payload must be byte-identical in all three
  formats.

Run the native verifier against bundles produced on the current platform, for example:

```bash
npm run release:verify-bundles -- src-tauri/target/release/bundle package-evidence --platform windows --architecture x64 --signature-policy unsigned-allowed
```

Use `--platform linux --architecture x64` on Linux. For a universal macOS build, use
`--platform macos --architecture universal` and the
`src-tauri/target/universal-apple-darwin/release/bundle` root. A signed release changes
the policy to `signed-required` and adds
`--signing-config src-tauri/platform-signing.release.conf.json`. A missing or unexpected
Windows/macOS publisher, missing timestamp, failed Gatekeeper assessment, or absent
stapled notarisation ticket then fails the gate. The `unsigned-allowed` policy is for
ordinary local diagnostics only and is never used by tagged release builds.

The tagged workflow builds Linux on Ubuntu 22.04, the oldest supported Tauri v2 baseline
used by this project. It then extracts the AppImage on Ubuntu 22.04 and installs the deb
on Debian 13 and the rpm on Fedora 43 in clean Docker containers. The deb and rpm tests
also require a complete dynamic-library closure. Run that networked Linux-only gate with:

```bash
npm run release:linux-install -- path/to/linux-bundles path/to/install-evidence
```

The package identity uses ASCII `tufekci-paperworks` for package-manager portability;
the desktop entry retains the visible `Tüfekci Paperworks` name. Downloaded AppImages
have their executable mode restored before container extraction because GitHub artefacts
do not preserve Unix permissions.

The metadata job accepts exactly one path-free native report for Windows x64, universal
macOS, and Linux x64, plus one three-distribution Linux installation report. It hashes
and aggregates those reports before checksums or SBOMs can be produced:

```bash
npm run release:package-evidence -- path/to/platform-reports path/to/linux-install-report path/to/summary
```

Tagged builds then gather every platform bundle and attach SHA-256 checksums, a release
manifest, separate npm and Cargo CycloneDX 1.5 SBOMs, and a combined dependency-licence
inventory to the draft GitHub release. Generate the same metadata locally with:

```bash
npm run release:metadata -- path/to/release-assets path/to/release-metadata
```

Before a tagged build, npm, the npm lockfile, Cargo, and Tauri must expose the same
semantic version and the tag must be exactly `v<version>`. Check that contract locally:

```bash
npm run release:version -- v0.1.0-alpha.1
```

Windows Installer does not accept alphabetic prerelease identifiers. The public app
version remains `0.1.0-alpha.1`, while `bundle.windows.wix.version` is the explicitly
validated numeric `0.1.0.1`. The final numeric prerelease component is the MSI build
sequence and must increase for later candidates sharing the same three-part version.

Use a clean artefact directory containing only the packages intended for that release.
The command rejects duplicate filenames and missing packages. See
[release metadata](RELEASE_METADATA.md) for the review and reproducibility rules.

## Windows

Install:

- Node.js LTS.
- Rust stable.
- Microsoft C++ Build Tools.
- WebView2 runtime.
- A WIA-compatible scanner driver for connected capture; WIA itself is built into
  Windows and does not require another command-line tool.
- QPDF.
- ImageMagick for optional extended image formats.
- Python with OCRmyPDF, pyHanko, and pyhanko-cli.
- Tesseract OCR and required language data, including English.

## macOS

Install:

- Node.js LTS.
- Rust stable.
- Xcode Command Line Tools.
- An Image Capture-compatible scanner. No separate scanner command-line package is
  required; the native bridge is built from source with Xcode Command Line Tools.
- QPDF.
- ImageMagick for optional extended image formats.
- OCRmyPDF.
- Tesseract OCR and required language data, including English.
- pyHanko and pyhanko-cli.

For one Intel and Apple Silicon bundle, install both Rust targets and use Tauri's
universal target:

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run tauri build -- --target universal-apple-darwin
```

## iPhone And iPad

Apple mobile builds can be generated only on macOS. Install:

- a current supported Xcode release and its command-line tools;
- Node.js 22.13 or newer and npm 10 or newer;
- Rust stable with `aarch64-apple-ios` and `aarch64-apple-ios-sim` targets;
- an Apple Developer account and development-team identifier for device or App Store
  signing; and
- matching signing certificates, provisioning profiles, and App Store Connect access
  for distribution.

Validate the checked-in mobile contract, generate the ignored Xcode project, and build
the unsigned Apple Silicon simulator application:

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
npm ci
npm run release:apple-mobile-check
npm run mobile:ios:init
npm run mobile:ios:build-simulator
npm run release:apple-mobile-bundle -- src-tauri/gen/apple artifacts/apple-mobile-simulator
```

Use Xcode's simulator/device selection through Tauri during interactive development:

```bash
npm run mobile:ios:dev -- "iPhone 16 Pro"
```

For a signed App Store Connect archive, expose the intended team only in the local or
protected CI environment, then provide a monotonically increasing build number:

```bash
export APPLE_DEVELOPMENT_TEAM="YOUR_TEAM_ID"
npm run mobile:ios:build -- --build-number 1
```

Do not commit a development-team identifier, certificates, private keys, provisioning
profiles, App Store Connect keys, or the generated `src-tauri/gen/` directory. The
credential-free simulator job deliberately disables code signing and uses a non-secret
placeholder team only to generate the project. A distributable IPA is not complete
until signing, installation on representative iPhone and iPad hardware, TestFlight,
privacy metadata, screenshots, accessibility review, and App Store submission have
passed.

The current mobile runtime enables the self-contained PDF.js/Rust workflows and visual
signature vault. It rejects desktop-process OCR, QPDF document protection, PDF/A,
pyHanko certificate signing, and connected-scanner jobs. Camera capture and system-wide
Open In document hand-off are not implemented. Updates are delivered through the App
Store rather than the desktop updater.

## Linux

Install:

- Node.js LTS.
- Rust stable.
- Tauri Linux prerequisites for your distribution.
- SANE and the `scanimage` command, plus the backend for the intended scanner.
- QPDF.
- ImageMagick for optional extended image formats.
- OCRmyPDF.
- Tesseract OCR and required language data, including English.
- pyHanko and pyhanko-cli.

Tagged drafts build AppImage, deb, and rpm packages. The release gate described above
checks their structures and installs them on the declared distribution baselines before
metadata is published. Follow the [Tauri AppImage baseline guidance](https://v2.tauri.app/distribute/appimage/)
when changing the oldest supported Linux build environment.

Run the ignored private hardware gate and record the release matrix as described in
[connected-scanner testing](SCANNER_TESTING.md). No scanner or personal-document fixture
is committed to the repository.
