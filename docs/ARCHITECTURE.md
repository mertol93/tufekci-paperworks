# Architecture

## Product Direction

Tüfekci Paperworks should be a local-first desktop app that handles the common
document jobs people usually need separate tools for: scanning, page operations,
OCR, signing, form work, annotations, and export.

The application should avoid uploading documents to a server unless a user
explicitly configures a future integration. Many PDF files and scans contain
personal, legal, medical, or business data, so privacy is a product feature, not
just a technical detail.

## Stack

- Desktop shell: Tauri.
- Backend: Rust command layer.
- Frontend: TypeScript, React, and Vite.
- Rendering: PDF.js worker with lazy thumbnails, high-DPI canvas pages, local
  password handling, bundled CMaps, standard fonts, ICC profiles, WASM, and a
  static annotation/form-appearance layer with scripts and links disabled. Desktop
  PDFs use a custom bounded Tauri range transport; browser-selected files remain
  memory-backed. Opening passwords stay in session memory and are bounded to 1,024
  UTF-8 bytes. PDF.js exceptions and native range failures cross the presentation
  boundary only as stable path-free codes; the shared dialogue remains mounted during
  retry so focus and its live incorrect-password state are preserved.
  Evaluate native PDFium only if measured performance or fidelity becomes a blocker.
- Structural PDF operations: embedded lopdf organiser/export adapter, with QPDF
  retained for encryption and operations where its repair model is preferable.
- Scan import: embedded Rust image conversion for common formats, with an optional
  ImageMagick normalisation fallback for formats unavailable in the bundled codecs.
- OCR: OCRmyPDF and Tesseract adapter first.
- PDF standards: explicit OCRmyPDF/Ghostscript PDF/A conversion followed by independent
  veraPDF validation; validation-only PDF/UA through exact veraPDF flavours; and a
  separate bounded, non-certifying built-in PDF/X structural preflight.
- Digital signatures: pyHanko-compatible adapter first.
- Application updates: Tauri's Minisign-enforcing updater plug-in, enabled only in
  credential-backed release builds and reached through a narrow Rust command layer.

## Why This Shape

PDF editing is hard because "PDF" is both a page description format and a large
set of document workflows. A good open-source editor should not start by writing
a PDF engine from scratch. It should use proven engines and build a careful app
around them.

The Rust backend owns validation, paths, process execution, temp files, and error
normalisation. The UI owns document interaction, workflow state, and previews.

## High-Level Flow

```text
User action
  -> React workflow state
  -> Tauri command
  -> Rust validation and job setup
  -> PDF/scan/OCR/signing engine adapter
  -> output PDF and job report
  -> UI refreshes preview and history
```

## Core Modules

### Frontend

- Document workspace.
- Scan intake for one or many image files.
- Paper format chooser for A4, US Letter, business cards, ID cards, and driving licences.
- Page thumbnail strip.
- Typed, non-destructive page plan with operation history. A separate stable-ID selection
  model keeps one active preview page while supporting ordered toggle/range selection;
  group drag, step movement, rotation, duplication and deletion each commit one history
  snapshot and preserve imported-source identities and visual-mark page bindings.
- Source-aware page identities for the primary PDF and directly imported PDFs, with
  one reversible insertion operation for each selected range. Native source review uses
  a distinct shared read-only job with progress, cancellation, retry, reattachment, and
  a final source fingerprint before PDF.js opens the selected pages.
- A bounded two-document transfer workspace maps selected stable page IDs into a
  destination plan without duplicating source registrations. It preserves source-page
  rotation, blank geometry and visual-mark placement IDs, offers drag and exact numeric
  insertion, and renders at most eighty source and eighty destination thumbnails. Copy
  uses a distinct `page-transfer` scheduler identity and the composed organiser publisher.
  Move invokes the same verified
  create-new publication first and commits source removal to page-plan history only after
  the destination reopens successfully; no original path is a publication target.
- Main page preview and cancellable PDF.js canvas rendering.
- Cancellable PDF.js range transport for primary, imported, and recovered desktop
  PDFs, beginning with a 64 KiB sample and requesting bounded local byte ranges.
- Cancellable PDF.js text and annotation layers with local annotation icons, inert
  form controls, and a non-navigating link service for safe display-only previews.
- Shared password-aware edit-safety state for standalone PDF sources, backed by one
  debounced aggregate read-only job with exact stale-job cancellation, staged progress,
  retry, path-free ordered results, stale-result rejection, action gating, and explicit
  certificate-risk acknowledgement.
- Reusable bounded in-memory planning history for standalone Merge and Split. Merge
  commits source additions, removals, ordering, and page-range edits through a
  password-stripping snapshot function; password edits replace only the live state.
  Split stores only page-group text, and selecting a new source resets its history.
- Privacy Cleaner with a required local inspection state, compact concealment summary,
  typed findings, direct links to safe removal switches, and an exact source fingerprint
  carried into create-new export. Ambiguous layer or concealed-artwork findings never
  select a destructive action automatically.
- Existing-text search with selected-locale compatibility normalisation, cached per-page
  extraction, progressive results, rejected-cache eviction, and one content-free
  presentation failure code. Full-page and thumbnail rendering states use translated
  accessible names; only the visible page announces progress or failure.
- Two-document comparison over the same local PDF.js range transport, with progressive
  page geometry and bounded selectable-text analysis, cancellable task ownership, and
  a selected-page visual renderer capped at two million pixels. Comparison text and
  raster buffers remain session-only and are discarded when the workspace closes.
- Bookmark workspace backed by the local PDF.js range transport for page preview and
  bounded streamed heading analysis. Draft editing moves, indents, outdents, and deletes
  whole branches so hierarchy remains valid; generated heading suggestions remain
  review-only until explicitly applied to the draft. An optional contents planner filters
  selected hierarchy levels, previews shifted physical page numbers, and submits only a
  bounded title, level, and sidebar-bookmark choice. Source inspection and publication
  use distinct shared jobs so bounded destination and outline traversal can be cancelled,
  reattached, and rejected when the source changes without retaining its path or password.
  Publication freezes editing and offers optional session-only AES-256 output passwords.
- Annotation workspace layered over the local PDF.js page canvas with normalised
  top-left coordinates, drag creation and movement, selection and property editing,
  bounded image preparation, and a capped session-only history. Supported existing
  indirect annotations are converted into the same draft model; their source-layer
  appearances are selectively hidden while the editable overlay is active. Unsupported
  PDF.js annotations remain visible and inert. A baseline-to-current change set separates
  new, updated, removed, and unchanged items, and duplication deliberately clears all
  source and viewer identities.
  Source inspection and publication use distinct shared jobs so inspection can report
  page-level progress, cancel safely, reattach, and reject a changed source without
  retaining its path or password. Publication freezes the draft while active and
  offers session-only AES-256 output passwords.
- Page-content workspace layered over the same bounded PDF.js canvas, with native-
  reviewed normalised text and image bounds, keyboard and pointer selection, image
  dragging, exact percentage geometry, replacement previews, and a capped session-only
  history. The frontend submits only changed opaque source identities and never decides
  whether a PDF operator is editable. Inspection and publication are separate jobs;
  publication freezes the draft and offers session-only AES-256 output passwords.
- AcroForm workspace over the same bounded PDF.js range transport, with a searchable
  field model, typed controls, normalised widget overlays, page-linked selection,
  client-side validation, and a capped session-only value history. Main-preview form
  controls remain inert; only the explicit workspace can submit reviewed changes.
  Source inspection and publication use distinct shared jobs so page/widget discovery
  and recursive field parsing can be cancelled, reattached, and rejected when the source
  changes without retaining its path or password. Publication freezes the draft and
  offers session-only AES-256 output passwords.
- Page Finish workspace with a shared range grammar, visual-edge crop and fit-to-paper
  model, responsive layout/marks/numbering controls, and a live clipped PDF.js preview.
  The pure preview model mirrors native crop, target-paper, token, and Bates calculations
  without persisting mark text, source passwords, or rendered page data. Source
  inspection and publication use distinct shared jobs so page geometry and annotation
  traversal can report progress, cancel safely, reattach, and reject a changed source
  without retaining its path or password. Publication freezes settings while active
  and offers session-only AES-256 output passwords.
- Permanent-redaction workspace over the bounded PDF.js range transport with normalised
  manual regions, drag movement, black or white fills, a capped undo/redo history, and
  explicit destructive-export acknowledgement. Local page-by-page text indexing offers
  selectable literal, email-address, and safe wildcard suggestions; suggestions are
  visually distinct and remain uncommitted until reviewed. Export renders only marked
  pages to clean bounded lossless rasters, sends reviewed normalised regions separately,
  and releases each canvas after encoding. Native publication rejects malformed or
  excessive geometry, applies one-pixel-expanded opaque masks in reviewed order, and
  records the SHA-256 digest of the complete flattened RGB image. Source
  inspection and publication use distinct shared jobs so page geometry and annotation
  traversal can report progress, cancel safely, reattach, and reject a changed source
  without retaining its path or password. Session-only AES-256 output passwords use the
  shared protection controls; native publication verifies exact masked pixels in the
  prepared copy, then decrypts and repeats every redaction, pixel-digest, and privacy
  check on the protected candidate.
- Multi-document page previews and search routed through the source identity retained
  by each planned page.
- Inspector for selected page, document, scan settings, or operation.
- Keyboard shell navigation with a visible-on-focus document-editor skip target, one
  roving tab stop for the vertical workflow chooser, Arrow/Home/End selection, announced
  active-workflow state, and a consistent high-contrast `:focus-visible` treatment.
- One shared modal-focus hook for every dialog and full-screen workflow workspace. It
  chooses a declared initial target, contains forward and reverse Tab movement, applies
  Escape only when the workflow permits closing, and returns focus to the connected
  opener. Static contracts require every `aria-modal` consumer to use this boundary.
- Typed local interface catalogues for `en-GB`, `en-US`, `tr-TR`, and `de-DE`, with
  exact key and placeholder parity, British English default/fallback, an explicit
  persisted selector, root `lang` updates, and locale-aware number, date, and list
  helpers. The migrated boundary covers workflow navigation, shared document open,
  drop, loading, recovery, edit-safety and operation states, the complete page organiser,
  Merge, output protection, shared PDF-job controls, the complete visual-mark creation,
  placement, and encrypted-vault surface, searchable OCR, OCR confidence review,
  image-scan processing, and connected-scanner controls. Document byte counts remain
  numeric state and are formatted only at render time, so live locale changes also
  update sizes. Native snapshots carry stable stage codes for organiser export, OCR
  review, searchable OCR, scan export, scan preview, and scanner capture; scanner
  discovery carries a stable status. Organiser warnings are translated from exact known
  messages, while unknown warnings and failures use content-free catalogue messages.
  A missing or legacy running-stage code falls back to the translated generic starting
  state rather than displaying native prose. Remaining native stages and errors still
  require the same code-and-arguments migration before release completeness.
- Signature studio with named signature and initials assets created from freehand
  canvas strokes, locally processed images, or typed styles. Session assets are
  reusable and draggable; normalised placements support pointer and keyboard movement,
  proportional resize, free rotation, duplication, deletion, per-placement locking,
  and a bounded undo/redo history tied to stable page identities.
- Flattened visual-signature export using bounded PNG decoding, one embedded image per
  used asset, reusable PDF soft masks, page-specific resources, arbitrary mark angles,
  and placement matrices mapped through page rotations of 0, 90, 180, and 270 degrees.
- Certificate studio with separate sign and validate modes, PKCS#12 selection,
  visible or invisible field controls, trusted timestamp and PAdES options, optional
  trust roots, an optional source-PDF password, structured validation reports, and a
  guard against signing stale
  on-disk bytes while the open workspace contains unexported edits. Signing uses the
  shared job hook for progress, cancellation, terminal reattachment, and settings
  freezing. Validation uses a distinct shared read-only job with the same lifecycle,
  while preserving separate signing and validation reports in the interface.
- Scan/OCR, compression, privacy-clean, batch-recipe publication and source review,
  merge, split, organiser-export,
  page-import review, bookmark publication and review, annotation publication and review, Page Finish
  publication and review, form publication and review, certificate-signing,
  permanent-redaction publication and review, password-protection and PDF/A publication plus Document Health
  and certificate-validation progress backed by one bounded native FIFO
  queue, with cancellation, transient-status retry, terminal-result reattachment by
  opaque job identifier, and no source paths, passwords, document text, or document
  bytes in running snapshots.
- Connected-scanner discovery and acquisition controls backed by one typed platform
  contract. The UI only offers sources, duplex, resolutions, and colour modes reported
  by the selected device.
- Debounced selected-page scan comparison using the same native clean-up pipeline as
  export, with stale-result rejection and short-lived object URLs.
- OCR readiness state tied to the selected installed language, plus a responsive
  confidence-review dialog with clickable low-confidence boxes and editable local
  recognition hints. Review text and hints are intentionally excluded from recovery.
- Error review with copyable diagnostics.

### Backend

- Tool discovery.
- Filesystem boundary checks.
- Canonical local-PDF metadata and bounded range-read commands with opening size and
  modification-time checks to detect a source that changes while it is open.
- Temporary workspace management.
- Atomic, create-new PDF publication followed by structural reopening and validation.
- Page range parser.
- Multi-source page assembly and prepare-before-publish multi-output splitting, with
  source/page/part cancellation checkpoints, bounded object-stream reopening,
  content-free retained failures, and final source-fingerprint gates. Merge can apply
  optional QPDF AES-256 to its verified prepared copy, then decrypt and repeat page-tree,
  exact rebuilt-bookmark-tree, and catalogue checks before publication. Resolved source
  bookmarks are filtered through selected ranges, remapped to the first copied
  occurrence, and safely promoted when an ancestor is omitted; unresolved and
  unselected entries are counted. Split builds a complete second temporary set when
  protection is selected; every part must encrypt, decrypt, and repeat the same checks
  before any part is published. AcroForm catalogues remain reported rather than merged
  until a collision and field-renaming policy is implemented.
- Composed organiser export that renumbers imported object graphs, rewrites selected
  pages into the primary page tree, prunes unreachable objects, and verifies the copy.
  It fingerprints primary and imported range sources at opening, checks them when the
  worker starts and immediately before publication, and exposes cancellation checkpoints
  while snapshotting, arranging, flattening signatures, reopening, and finalising.
  The same expected per-page visual-mark counts are checked on the prepared output and
  again after optional QPDF AES-256 protection is decrypted.
- Fail-closed page-content inspection and publication. Native code exposes editable
  identities only for exact unshared page-level `Tj` text operators with a bounded
  original-font encode/decode round trip, and exact unambiguous `q`, `cm`, image `Do`,
  `Q` paint blocks. Shared streams, nested forms, complex text operators, direct or
  malformed content, arbitrary vectors, and unsupported encodings remain read-only.
  Export accepts only rediscovered identities, rechecks the complete source SHA-256 at
  worker start and immediately before publication, edits operations in descending
  stream order, clones image resources page-locally, and never prunes unrelated objects.
  Reopening verifies edited markers and decoded hashes, untouched stream hashes,
  replacement-image pixel hashes and dimensions, page count, forms, bookmarks, and
  annotations. Optional QPDF AES-256 is decrypted and verified again before create-new
  publication.
- Read-only document preflight with bounded object-graph traversal, strict page-stream
  decoding, named-resource checks, font embedding and ToUnicode inspection, output
  intents, bounded binary ICC header/tag-table validation, unmanaged Device CMYK
  detection, and cycle-safe recursive Form XObject resource/content traversal through
  32 levels and 100,000 page-specific contexts. It exposes invalid-profile, nested-form,
  stream, and resource counts with disclosed truncation and deliberately stops short of
  standards or colourimetric conformance certification.
- Static accessibility preflight for title display, language, tagged-PDF declarations,
  structure-tree consistency, page structure links, RoleMap-aware Figure alternative
  text, and interactive tab-order signals. It never claims to prove reading order.
- Lightweight post-open edit-safety preflight that checks certificate and form
  structures without decompressing page streams. Its aggregate scheduler request
  supports up to 250 sources, bounds passwords, object streams, objects and pages,
  honours cancellation during object traversal, rechecks each source fingerprint,
  and gates rewrite controls until its current-source result is available.
- Mandatory certificate-signature acknowledgement validation in organiser export,
  merge, split, privacy-cleaning, compression, annotation, password-addition, and
  password-removal commands before output is written or published.
- Versioned authenticated signature-vault envelopes with per-entry Argon2id key
  derivation, AES-256-GCM encryption, random salts and nonces, bounded PNG validation,
  opaque locked listings, and create-new private files in app-owned storage. Version
  two encrypts the signature/initials kind and draw/image/type method with the existing
  metadata and pixels; version-one records remain readable as image signatures. The
  four command boundaries serialise only an allow-listed outcome code, so filesystem,
  parser, key-derivation, and cryptography details stay inside Rust.
- Bounded certificate command adapter with PKCS#12 and trust-root validation, private
  one-use passfiles, a bounded standard-input password bridge for encrypted PDFs, HTTPS
  timestamp URL validation, rotation-aware field geometry, controlled child-process
  cancellation, source fingerprinting immediately before create-new incremental
  publication, source/output encryption-parity checks, sanitised retained job results,
  structural byte-range checks, pyHanko post-validation, and typed integrity and trust
  results.
- Bounded privacy preflight for metadata, active content, attachments, forms,
  thumbnails, optional-content groups and default-hidden use, invisible text modes,
  zero-opacity page graphics, hidden annotations, non-empty crop/media differences,
  Web Capture provenance, PDX attachments, and declared private-extension containers.
  It scans page-level streams and resources conservatively and reports unsupported
  Form XObject, visibility-expression, clipping, colour, and binary-payload analysis.
  The preflight uses a distinct shared read-only job with cancellation inside bounded
  object and page traversal, path/password-free lifecycle snapshots, content-free
  failures, a path-free report, and a final source fingerprint.
- Fingerprinted privacy-clean export with explicit data categories, Web Capture
  `SpiderInfo`/`URLS`/`IDS` removal, unreachable-object pruning, create-new publication,
  category-by-category output verification, final source revalidation, and optional
  AES-256 output whose decrypted candidate repeats the selected-category checks.
- Typed preservation-first compression analysis and export with bounded DeviceRGB and
  DeviceGray image decoding, JPEG candidate generation, representative before/after
  previews, exact counting-writer estimates, modern object streams, create-new output,
  page, form, and bookmark reopening checks, content-free retained failures, and final
  source revalidation. Preview analysis uses a distinct shared read-only job with
  cancellation, reattachment, source/password-free lifecycle snapshots, path-free
  report metadata, and a final source fingerprint before its volatile image samples are
  delivered. Optional QPDF AES-256 protection is applied only to the verified
  smaller candidate; the protected copy must decrypt, repeat every structural check,
  and remain smaller than the source before publication. Masks, specialist colour
  spaces, unsupported filters, signatures, and images outside resource limits are not
  JPEG-recompressed; ordinary lossless stream optimisation may still apply.
- Distinct Batch Recipe source-review job for up to fifty unique PDFs. It maps the
  existing bounded Privacy Inspection control into a progress range for each source,
  stops the active traversal on cancellation, keeps the aggregate path/password request
  transient, and returns ordered path-free reports or content-free per-file errors only
  after each successful source passes its final fingerprint check.
- Bounded batch-recipe orchestration for up to fifty fingerprinted PDFs and 20 GiB of
  source data. It composes the cancellable OCRmyPDF adapter with the existing verified
  privacy and compression adapters in OCR, privacy, then compression order. Protected
  OCR sources are decrypted only into the private output-side workspace. Every OCR
  stage must preserve the inspected page count and is reopened to inspect searchable
  text-layer coverage; textless pages become bounded warnings. Child progress maps into
  per-file and per-step ranges, including temporary unlock work. Optional PDF/A-1b,
  PDF/A-2b, or PDF/A-3b conversion follows the other transformations, reopens the
  candidate, and requires a matching veraPDF verdict before the complete output set can
  be published. Archival output excludes QPDF protection because PDF/A forbids
  encryption. Otherwise, optional shared AES-256 protection is applied once to the
  final candidate, which is decrypted in
  memory to repeat page-count and searchable-layer comparisons. The executor prepares
  every required copy before publication, rechecks every original source, and rolls back files
  created by the batch if publication cannot complete. Existing files are never replaced.
- Scan-to-recipe intake composes existing publication boundaries rather than introducing
  a second image conversion path. Connected-scanner or local-image pages first pass scan
  review, clean-up, optional OCR, create-new publication, and reopening verification.
  Only the resulting PDF path and a generic session-only origin are handed to Batch
  Recipes. Passwords, device identifiers, capture page paths, image bytes, OCR hints,
  and scan settings are excluded; protected output must be unlocked again by the user.
- Bounded bookmark parser and writer preserving Unicode titles, hierarchy, page targets,
  style, colour, and expansion state. It rejects malformed cycles and over-deep trees,
  fingerprints the reviewed source at worker start and immediately before publication,
  rebuilds whole-page Fit destinations, and reopens and compares the complete bookmark
  tree and form presence. Optional QPDF AES-256 protection is applied to the prepared
  copy and followed by the same checks after decrypting with the new opening password.
- Optional printed-contents composer that prepends bounded A4 pages to the existing page
  tree, embeds Liberation Sans as a Type0/CIDFontType2 font with a ToUnicode map, creates
  direct whole-page link annotations, shifts source and bookmark destinations together,
  and optionally adds a top-level contents bookmark. Reopening verifies private page
  markers, entry counts, title streams, Unicode and embedded-font structures, link
  targets, the first untouched source page, forms, and the complete resulting outline.
- Bounded annotation writer for standard `FreeText`, `Highlight`, `Stamp`, `Ink`,
  `Square`, `Circle`, and `Line` objects. It maps normalised visual coordinates through
  inherited page boxes and rotation, creates an appearance stream for every item,
  embeds bounded PNG image stamps, preserves existing annotation arrays and forms,
  fingerprints the reviewed source at worker start and immediately before publication,
  and verifies every new marker, subtype, appearance, image resource, page count, and
  annotation count after reopening the create-new copy. Optional QPDF protection receives
  the same verification after decryption; retained job failures are content-free.
- Bounded AcroForm field-tree parser and writer for inherited text, checkbox, radio,
  choice, push-button, and signature properties. It maps widget geometry through page
  boxes and rotation, validates exact field updates, stores Unicode values as UTF-16BE,
  generates Windows Latin text and button appearances, and can place generated form
  XObjects into page content before pruning only safely flattenable fields. Export
  rejects XFA, fingerprints the source at start and immediately before publication,
  preserves unrelated annotations and fields, and verifies updated values or flattened
  markers after reopening the create-new copy. Optional QPDF protection receives a
  second decrypted verification keyed by stable field names because external rewriting
  may renumber indirect objects.
- Bounded page-finishing writer that maps visual crop margins through inherited page
  boxes and rotation, optionally wraps existing content in a clipped proportional
  fit-to-paper transform, and carries standard annotation and widget geometry. It places
  rotation-aware Form XObject layers for watermarks, token-expanded headers and footers,
  and selected-range Bates labels; preserves forms and bookmarks; fingerprints sources;
  and verifies boxes, counts, page markers, and mark streams after create-new reopening.
  Optional QPDF protection receives the same verification after decryption, and retained
  job failures are content-free.
- Bounded permanent-redaction writer that validates source fingerprints, passwords,
  certificate acknowledgement, page geometry, raster dimensions, pixel counts, PNG
  payloads, region counts, and total allocations. Each selected page is rebuilt upright
  with one Flate-compressed RGB image and one image-only content stream. The writer uses
  the privacy scrubber to remove global metadata, actions, attachments, annotations,
  forms, navigation, thumbnails, optional-content catalogue data, and structure trees;
  prunes unreachable objects; then reopens and verifies exact
  markers, page boxes, raster lengths, resource isolation, empty searchable text, page
  count, privacy residue, and unlocked output. Shared job control reports checkpoints
  through page decoding, pixel flattening, compression, privacy cleaning, writing, and
  reopening; honours cancellation before publication; and rechecks the source
  fingerprint after verification immediately before create-new publication.
- Versioned recovery snapshots for page plans, scan batches, and standalone Merge and
  Split plans, written as rotating create-new files so a partial newest write cannot
  destroy the previous valid draft. Merge stores stable source identities, paths, order,
  and range text; Split stores its source path and group text. Restoration verifies
  source-file presence before mounting either planner, while passwords, certificate
  acknowledgements, output protection, undo stacks, and jobs remain session-only.
- Scan preset catalogue.
- Pure-Rust scan clean-up with downsampled border analysis, robust page-edge fitting,
  confidence gates, projective inverse mapping, bounded bilinear resampling, and local
  illumination normalisation. Failed page detection preserves the original framing.
- Bounded OCRmyPDF and Tesseract readiness probes with explicit command, timeout,
  language-discovery, and missing-pack diagnostics before expensive scan processing.
- An embedded one-use OCRmyPDF progress plug-in and bounded streaming parser report only
  the OCR phase. Exact machine records and guarded Rich/tqdm records map engine 0–100%
  monotonically into overall scan progress 76–90%; malformed, unrelated, duplicate,
  decreasing, and oversized records are ignored.
- Strict Tesseract TSV confidence parsing with a maximum 8 MB response, 20,000 words,
  validated page-relative boxes, and at most 250 returned low-confidence words.
  Confidence review uses a distinct shared read-only job with staged image preparation,
  process-tree cancellation, reattachment, source/settings-free lifecycle snapshots,
  content-free failures, a final source-image fingerprint, and volatile recognised
  words excluded from diagnostics and Activity history.
- Page-level searchable-text reopening reports after OCR and temporary user-word files
  for explicitly reviewed vocabulary hints. Hint and progress plug-in files are deleted
  after the command and covered by crash-safe temporary leases.
- A dedicated existing-PDF searchable-OCR publisher wraps the same bounded batch OCR
  engine path. Its strict request remains transient, accepts protected input and
  optional AES-256 output, enforces certificate acknowledgement, reports page coverage,
  and uses the shared cancellation, recovery, source-recheck, create-new publication,
  and content-free failure boundaries.
- Scan publication fingerprints every canonical source image after validation and
  checks its size and modification time again immediately before publication. Optional
  QPDF AES-256 receives only the verified image or OCR candidate; the encrypted
  candidate is loaded with a 64 MiB object-stream bound, required to be encrypted,
  decrypted in memory, and checked again for exact page count, embedded images, and
  unchanged searchable-text coverage.
- Connected-scanner request validation, app-owned capture workspaces, output
  confinement, size and page limits, stale-session clean-up, versioned native adapter
  protocols, and a typed shared acquisition job. The job reports staged progress,
  terminates complete adapter process trees on cancellation, fingerprints every
  validated page by size and modification time before result delivery, and removes its
  private workspace after an ordinary failure or cancellation.
- Shared native manager for scan/OCR creation, standalone searchable OCR, scan clean-up
  preview, compression
  publication and preview, OCR confidence review, privacy inspection and cleaning,
  connected-scanner acquisition, batch source review and publication, merge, split,
  organiser export, page-import review, bookmark publication and review, annotation publication and review,
  Page Finish publication and review, form publication and review, page-content
  publication and review, and
  certificate publication, permanent redaction publication and review, password protection, PDF/A archival,
  Document Health, and certificate validation, with
  two workers, sixteen non-terminal jobs, thirty-two retained public snapshots, FIFO
  dispatch, monotonic stage progress, cancellation before publication, and typed
  operation results. Secret-bearing queued requests are transient native state and are
  removed when cancelled or handed to a worker. Scan creation uses the generic typed
  start, get, list, and cancellation commands directly, without a second scheduler or
  compatibility IPC layer. Scan failures retained by that manager contain neither
  paths, OCR hints, passwords, nor engine diagnostics.
- Every native structural publication and validation workflow is available only through
  the generic scheduler commands. An exact registered-command allow-list regression
  permits direct IPC only for bounded document transport, capability and readiness
  probes, recovery, Activity history, signature-vault operations, scanner discovery,
  signed application updates, and aggregate status. Synchronous helpers remain internal
  to backend enforcement or native tests.
- The shared TypeScript job contract classifies all thirty-three native job kinds under one
  exhaustive protection policy. Fourteen structural publication kinds support optional
  AES-256 output; certificate publication preserves the source encryption state;
  PDF/A archival accepts an encrypted source but forbids encrypted output; Protection
  adds or removes AES-256; and read-only or media-intake jobs publish no structural PDF.
  A source-level regression compares this map with the Rust enum and checks each native
  publisher for its password, QPDF, reporting, and protected-output reopening boundary.
- Shared terminal-job presentation for every scheduled workflow. Retry invokes the
  workflow's current reviewed setup instead of retaining an old request; publication
  workflows repeat destination selection, while the fifteen read-only jobs rerun
  against current inputs. Connected-scanner acquisition asks the user to review the
  current device and feeder before starting a fresh capture. Copyable diagnostics are built
  from an allow-list of public snapshot fields and deliberately ignore typed result
  payloads.
- Privacy-preserving Activity history at the shared scheduler's terminal transition.
  Succeeded, failed, and cancelled jobs are recorded exactly once with only an opaque
  audit-entry ID, operation kind, outcome, and timing. A cross-process lock, three
  create-new generations, strict schemas, a 500-entry/512-KiB bound, and older-generation
  fallback protect the local store. The interface filters, exports, and clears it;
  job IDs, stages, errors, warnings, paths, passwords, content, and results never enter
  the audit schema.
- Private crash-safe temporary-workspace registry for PDF candidates, batch directories,
  certificate input workspaces and passfiles, pyHanko password-bridge directories,
  OCR user-word hints, and
  scan-normalisation rasters. A live
  artefact owns an exclusive operating-system lock; bounded start-up clean-up handles
  only unlocked strict records after canonical-parent, filename, type, link/reparse, and
  batch ownership-token validation. Its public status is aggregate and path-free.
  Recoverable connected-scanner captures remain under their separate seven-day policy.
- Private active-job recovery journal for every scheduled workflow. One
  strict create-new record and operating-system lock represent each accepted job.
  Records contain only a schema version, opaque entry ID, workflow kind, and start
  time. Start-up skips locks held by live application instances, removes stale records
  exactly once, and reconstructs a bounded interrupted terminal snapshot. Requests,
  paths, passwords, OCR hints, signatures, page data, progress, errors, and results
  never enter the journal. Interrupted work is not resumed or replayed; publication
  users check the destination, read-only checks run again from current inputs, and
  scanner acquisition starts afresh after the device and feeder are reviewed.
- Document Health as the first read-only shared job. Its request stays in transient
  native memory, public queued snapshots omit the source and password, retained
  failures are content-free, and the typed report is delivered only after the source
  size and exact modification time are rechecked.
- Edit-safety inspection as one aggregate read-only shared job for the thirteen rewrite
  interfaces that consume it. Up to 250 source/password entries remain transient;
  queued and interrupted snapshots omit the complete request; bounded structural
  checks occupy per-source progress ranges and honour cancellation; stale selections
  cancel their exact prior job before replacement; per-workflow storage scopes prevent
  cross-interface reattachment; and ordered path-free successes or content-free
  per-source failures are delivered only after individual final source fingerprints.
- Annotation review as a separate read-only shared job. Its source and password remain
  transient; queued and interrupted snapshots omit both; page traversal honours
  cancellation; extraction inspects at most 500 annotation entries per page and 2,000
  overall; only self-contained representable indirect annotations receive stable source
  and viewer identities; failures are content-free; and the typed report is delivered
  only after the exact source size and modification time are rechecked.
- Bookmark review as a separate read-only shared job. Its path and password remain
  transient; queued and interrupted snapshots omit both; named-destination and outline
  traversal honour cancellation; failures are content-free; and the complete typed tree
  is delivered only after the exact source fingerprint is rechecked.
- Page Finish review as a separate read-only shared job. Its path and password remain
  transient; queued and interrupted snapshots omit both; page-geometry and annotation
  traversal honour cancellation; failures are content-free; and the typed workspace
  model is delivered only after the exact source fingerprint is rechecked.
- Form review as a separate read-only shared job. Its path and password remain
  transient; queued and interrupted snapshots omit both; page annotation, widget, and
  recursive field traversal honour cancellation; failures are content-free; and the
  typed field model is delivered only after the exact source fingerprint is rechecked.
- Redaction review as a separate read-only shared job. Its path and password remain
  transient; queued and interrupted snapshots omit both; page-geometry and annotation
  traversal honour cancellation; failures are content-free; and the typed destructive
  workspace model is delivered only after the exact source fingerprint is rechecked.
- Page-import review as a separate read-only shared job. Its source path, password, and
  selected range remain transient; queued and interrupted snapshots omit the complete
  request; bounded range parsing and certificate-structure traversal honour limits and
  cancellation; failures are content-free; and the typed selected-page report is
  delivered only after the exact source fingerprint is rechecked.
- Certificate validation as a separate read-only shared job. Public snapshots and
  restart records omit the source password, PDF path, and trust-root paths; retained
  failures are content-free; recursive signature inspection and pyHanko execution honour
  cancellation; successful reports scrub local paths; and the source plus every trust
  root are fingerprinted again immediately before report delivery.
- Compression preview as a separate read-only shared job. Its password-bearing request
  remains transient; queued and interrupted snapshots omit the source and password;
  failures are content-free; image traversal and sample creation honour cancellation;
  retained report metadata omits the filename; and exact source size and modification
  time are checked before volatile preview samples are returned.
- Privacy Inspection as a separate read-only shared job. Its password-bearing request
  remains transient; queued and interrupted snapshots omit the source and password;
  failures are content-free; direct-object, optional-content, resource, annotation, and
  page-stream traversal honour cancellation; retained reports omit the filename; and
  exact source size and modification time are checked before report delivery.
- Batch Recipe source review as a separate read-only shared job. Up to fifty unique
  source/password requests remain transient; queued and interrupted snapshots omit the
  complete set; each bounded Privacy Inspection occupies a mapped progress range and
  honours immediate cancellation; failures remain content-free; and ordered path-free
  reports are returned only after their individual final source checks.
- OCR confidence review as a separate read-only shared job. Its source and clean-up
  settings remain transient; queued and interrupted snapshots omit the complete
  request; failures are content-free; ImageMagick and Tesseract descendants are
  terminated on cancellation; exact source size and modification time are checked
  before report delivery; and recognised words remain only in volatile result state.
- Scan clean-up preview as a separate read-only shared job. Its source and clean-up
  settings remain transient; stale configurations are cancelled; queued and
  interrupted snapshots omit the complete request; failures are content-free; exact
  source size and modification time are checked after bounded JPEG encoding; and
  preview bytes remain only in volatile result state.
- Connected-scanner acquisition as a separate shared job. Its device identifier and
  settings remain transient; queued and interrupted snapshots omit the complete
  request; failures are content-free; WIA, Image Capture, and SANE descendants are
  terminated on cancellation; each output's size and modification time are checked
  again before delivery; and successful paths remain only in volatile result state.
  The interface opens pages sequentially, supports retry and explicit discard, and can
  reattach while the same application process is alive. After a process interruption,
  capture files may remain under the separate seven-day recovery policy.
- PDF standards work as a shared job. Source and password remain transient, public and
  interrupted snapshots omit both, errors are content-free, conversion and validation
  descendants terminate on cancellation, certificate-bearing rewrites require explicit
  acknowledgement, and publication occurs only after profile, page-count, encryption,
  searchable-page, source-fingerprint, and veraPDF checks. PDF/UA validation is limited
  to exact unprotected sources. PDF/X uses the same lifecycle for a built-in bounded
  structural preflight whose typed outcome cannot be presented as conformance.
- WebView PDF.js rendering, search, previews, comparison, and print preparation deliberately stay outside
  the native job queue. They use fingerprinted bounded range reads, PDF.js loading-task
  destruction, cancellable text streams and render tasks, page clean-up, stale-run
  rejection, progressive comparison results, and explicit page, text, token, and pixel
  limits. Print preparation uses PDF.js print intent and current annotation storage,
  composites selected visual marks, converts one canvas at a time to a volatile PNG
  object URL, and then invokes the webview's system print dialogue. It rejects more than
  100 pages, 50 megapixels on one page, or 120 megapixels in one request and revokes all
  URLs on any settings or document change. Static regressions guard that separate
  cancellation and resource boundary.
- Engine adapters.
- Audit log for document operations.

### Engine Adapters

Adapters should be thin and replaceable. They should receive typed job requests
and return typed job reports. They should not leak raw command output directly
into UI state.

Long-running PDF, OCR, image-normalisation, certificate, and scanner commands run
inside an owned process tree. Windows starts the child suspended, assigns a Job Object
with kill-on-close, then resumes it; macOS and Linux use a separate process group.
Cancellation, timeout, monitor failure, normal
parent exit, and wrapper drop terminate descendants as well as the immediate child and
await the child before releasing temporary work. Piped output is drained concurrently
and retained only up to each adapter's fixed diagnostic limit.

Initial adapters:

- `lopdf`: embedded page selection, reordering, duplication, rotation, blank-page
  creation, selected-range import, multi-source merge, splitting, and verified
  export without a system dependency. It also owns the first privacy-cleaning pass
  for metadata, scripts and actions, attachments, annotations, forms, and page
  thumbnails, standard annotation writing and appearance generation, plus structural
  compression and modern object-stream rewriting.
- `qpdf`: encrypt, decrypt, and apply permissions through a standard-input argument
  file so passwords are absent from process arguments. Shared job control drains bounded
  diagnostics concurrently, enforces a 30-minute timeout, terminates cancelled work,
  fingerprints the reviewed source before and after processing, and publishes only
  verified create-new output. QPDF may later provide advanced repair, merge, split,
  extraction, and linearisation paths where appropriate.
- Rust `image` and `lopdf`: decode bounded image inputs, apply orientation, colour,
  crop, perspective, and lighting processing, and place them onto selected paper/card
  sizes without a system dependency. They also sample and recompress compatible PDF
  raster images without flattening text, vectors, forms, links, or OCR layers.
- `magick`: optional fallback normalisation for formats such as HEIC and AVIF.
- `ocrmypdf`: OCR, deskew, rotate pages, language selection, skip existing text, and
  temporary reviewed vocabulary hints. Its archival mode requests only explicit
  `pdfa-1`, `pdfa-2`, or `pdfa-3` output and disables optimisation to keep conversion
  policy separate from compression.
- `ghostscript`: local rendering and conversion dependency used by OCRmyPDF for explicit
  PDF/A output. It is probed independently so a missing executable disables archival
  conversion before a document workspace is created.
- `verapdf`: independent local PDF/A and PDF/UA validator. The adapter fixes the candidate name
  inside an ownership-token-protected workspace, selects an exact `1b`, `2b`, `3b`,
  `ua1`, or `ua2`
  flavour, bounds output to 4 MiB and 200,000 JSON nodes, retains at most fifty generic
  failed-rule summaries, and never forwards raw validator output into interface state.
- Built-in PDF/X preflight: validation-only profile selection for PDF/X-1a:2001,
  PDF/X-3:2002, and PDF/X-4. It reuses bounded health audits for object references,
  nested font resources and binary ICC structures, then checks declarations, trapping,
  encryption, GTS_PDFX output intents, page boxes, scripts, forms, attachments,
  external and non-printing content, and transfer curves. Its report type and interface
  language explicitly exclude ISO certification, colour proofing, and print approval.
- `tesseract`: bounded OCR availability and language-pack inspection, plus TSV word
  confidence and bounding-box review over the cleaned scan raster.
- Windows WIA: fixed-script discovery and acquisition with environment-bound request
  values, bounded output, discovery/capture timeouts, process-tree ownership, and
  support for flatbeds, feeders, and driver-reported duplex capability.
- Linux SANE: `scanimage` discovery, device-option inspection, and flatbed or bounded
  feeder acquisition into portable-anymap pages, with bounded output, timeouts, and
  process-tree ownership.
- macOS Image Capture: packaged Objective-C ImageCaptureCore sidecar with a main-thread
  run loop, persistent device identifiers, session and functional-unit inspection,
  capability-aware flatbed/feeder/duplex capture, file-based page callbacks, page-limit
  cancellation, and universal Intel/Apple Silicon packaging. The Rust boundary uses
  strict versioned JSON over standard input/output, bounded readers, timeouts, and
  process-tree ownership, then revalidates every returned path inside its app-owned
  capture workspace.
- `pyhanko`: add visible or invisible PKCS#12 signatures incrementally, request RFC 3161
  timestamps, embed PAdES validation information, validate trust and integrity, and
  inspect existing signature fields. The adapter requires a CLI version exposing
  `--passfile`; certificate passphrases and encrypted-PDF passwords are never process
  arguments. A fixed private `sitecustomize.py` redirects pyHanko's PDF-password prompt
  to a bounded standard-input line and contains no secret itself. Before pyHanko starts,
  the source PDF, PKCS#12 identity, and selected trust roots are SHA-256 checked and
  copied into a user-private registered workspace. The child receives only those
  disposable snapshot paths; the originals are hashed again before publication or a
  validation result is returned.

Visual signatures and initials are handled by the embedded export adapter and do not
require pyHanko. Assets remain transient session data unless the user explicitly
encrypts one in the vault. Placement history and pixels never enter recovery snapshots
or public job snapshots. During export, reused images are embedded once and placements
become ordinary page content; reopening must find the exact generated resource count
for every marked page. Optional signed-copy password protection uses QPDF and repeats
that check after decryption. The pyHanko adapter provides the separate certificate-backed
workflow for cryptographic integrity and signer trust; it does not turn flattened page
artwork, an editor placement lock, or PDF reader permission into a digital signature.

The visual-signature vault is also embedded. Its encrypted envelope covers the label,
mark kind, creation method, source name, dimensions, timestamp, and PNG bytes.
Passphrases and derived keys are never persisted; native sensitive buffers are cleared
when they leave scope. A vault entry cannot be recovered without its passphrase, and
unlocking it does not convert the visual mark into a certificate-backed digital
signature.

## Runtime Platforms

The frontend obtains one closed `runtime_capabilities` value from Rust during start-up.
It names the current platform and mobile state, then separately advertises local file
dialogues, the self-contained PDF core, image-to-PDF, external processes, connected
scanning, camera capture, searchable OCR, certificate signing, archival PDF, document
password protection, desktop updates, and store-managed updates. Parsing rejects
unknown, missing, or internally inconsistent values before they become application
state.

This contract is presentation guidance, not the security boundary. The shared Rust job
scheduler independently rejects OCR, PDF/A, certificate, QPDF protection, batch, and
connected-scanner requests when the compiled target cannot provide the required
desktop service. A plain image-to-PDF request remains available because scan clean-up
and PDF creation are self-contained Rust operations. The updater dependency and plug-in
are compiled only for non-mobile targets; iOS returns a stable App Store-managed state.

The iOS target reuses the React application and Rust library through Tauri's mobile
entry point. `tauri.ios.conf.json` provides an iOS 16 baseline and compact initial
window, while `Info.ios.plist` supplies iPhone/iPad orientation, multitasking, document,
keyboard, and pointer settings. Vite accepts Tauri's development host, the interface
uses safe-area insets, dynamic viewport units, and coarse-pointer targets, and the
macOS scanner prebuild skips Apple mobile targets. Tauri generates the Xcode project
under ignored `src-tauri/gen/`; generated Apple project files are not source artefacts.

## Important Rules

- Never mutate a source PDF or source image in place. Always write to a new output path.
- Keep document passwords in memory only. Never write them to logs or expose them in
  process command lines.
- Keep signature-library passphrases in memory only, minimise frontend retention, and
  never include prepared or unlocked signatures in recovery snapshots.
- Keep certificate passphrases out of process arguments and persistent state. Use a
  private one-use passfile and clear frontend passphrase state after a signing attempt.
- Run certificate engines only against verified private snapshots; reject any same-size
  source, identity, or trust-root mutation detected by the final SHA-256 recheck.
- Report certificate integrity and certificate-chain trust separately. Never label an
  intact but untrusted signature as proof of signer identity.
- Treat PDF reader permissions as advisory controls, not cryptographic enforcement.
- Treat redaction as destructive content removal, not a black rectangle overlay.
- Preserve signatures unless an operation necessarily invalidates them, and warn
  before doing so.
- Describe editor placement locks and PDF permission locks separately from
  certificate-backed digital signatures; only the latter provides cryptographic
  tamper evidence.
- Keep page ranges and scan page sizes explicit and previewable before writing output.
- Make every long-running operation cancellable.
- Keep all processing local by default.
- Keep passwords, signature images, document text, and document bytes out of recovery
  snapshots. Treat stored document names and local source paths as private app data.
- Treat a process-recovered job as interrupted, never as definitely failed before
  publication. Do not infer whether a destination exists, and do not replay a retained
  request. Ask publication users to check the chosen destination before a fresh retry;
  read-only checks should run again against their current source. Ask scanner users to
  review the device and feeder before starting a fresh acquisition because app-owned
  pages may already have been created.
- Treat health findings as preflight guidance rather than a malware verdict, and
  never remove suspected blank or duplicate pages without explicit confirmation.
- Treat general accessibility findings as machine-checkable signals rather than WCAG
  conformance. The separate PDF Standards workflow can formally validate PDF/UA rules,
  but semantic accuracy and reading order still need manual review with assistive
  technology or an accessibility API.
- Treat shell and modal accessibility tests as regression protection, not a substitute
  for testing packaged WebView builds with a keyboard, operating-system contrast and
  magnification settings, accessibility APIs, and screen readers. Follow
  [ACCESSIBILITY_TESTING.md](ACCESSIBILITY_TESTING.md) before release.
- Treat privacy cleaning as verified removal of selected PDF structures, not as
  redaction, antivirus scanning, or proof that no visually concealed content exists.

## Application Updates

On Windows, macOS, and Linux, the release channel, HTTPS manifest endpoint, and Minisign
public key are compiled into official builds. Configuration is all-or-nothing: ordinary
development builds register no updater plug-in and make no update request. The frontend reads only channel, current
version, restart-required state, candidate version, and bounded download progress. It
checks only after explicit user action and cannot supply a different endpoint or key.

iOS does not compile or register the Tauri updater. Its readiness command returns only
the store-managed state, and the interface directs users to App Store updates without
accepting an endpoint, key, channel change, package download, or in-app installation.

The Rust update state owns one transient pending Tauri `Update`, an exclusive operation
guard, and an installed/restart-required flag. Checks clear stale candidates; downloads
stream public progress through an IPC channel; Tauri performs signature verification and
installation; and restart remains a separate user action. Native errors are generic and
path-free. No update request, package bytes, URL, key, or error is placed in recovery or
Activity history.

Tagged builds derive alpha, beta, or stable from the exact semantic version, obtain
credentials only inside the protected `updater-signing` environment, and generate two
Git-ignored, secret-free overlays. The updater overlay contains only
`createUpdaterArtifacts`; the platform overlay contains only the expected public Windows
certificate thumbprint and timestamp settings, macOS Developer ID identity and hardened
runtime setting, or an explicit credential-free Linux bundle object. Windows imports
its PFX non-exportably for the build. macOS uses an ephemeral keychain and a
runner-temporary App Store Connect key. Platform credentials are removed immediately
after Tauri returns. Serial platform builds
assemble `latest.json`; a dependent gate validates supported platform families,
bounded signatures, and immutable GitHub release URLs before retaining its SHA-256
evidence. A separate manually dispatched `updater-promotion` environment revalidates a
published immutable release, copies only the reviewed manifest to the channel release,
and requires downloaded bytes to match. Withdrawal, forward-version rollback, manual
recovery, and signing-key operations are defined in [UPDATES.md](UPDATES.md).

## Packaging

The release pipeline should build:

- Windows installer and portable build.
- macOS universal or per-architecture app bundle.
- Linux AppImage and deb/rpm packages.
- iPhone/iPad simulator application as credential-free compile evidence; signed device
  and App Store archives remain a separate Apple release gate.

Each native release worker validates its own packages before the metadata job. Windows
inspects MSI and NSIS container and product metadata, exact Authenticode signer, and
trusted timestamp. macOS mounts the DMG read-only and checks the application property
list, universal executable, exact Developer ID team, secure signing timestamp,
Gatekeeper assessment, and stapled notarisation ticket.
Linux extracts AppImage, deb, and rpm containers, checks package-manager and desktop
identity, and requires the same x64 ELF payload in every format. The Linux build uses an
ASCII package identity while its desktop template retains the branded Unicode name.

A dependent Linux job extracts the AppImage on Ubuntu 22.04 and installs the deb and rpm
in clean Debian 13 and Fedora 43 containers, including dynamic-link closure checks. The
metadata job accepts only the exact four path-free reports for one release and signature
policy, then creates a hashed aggregate. It streams SHA-256 over every distributable
package, rejects duplicate release filenames, and publishes an exact manifest, npm and
Cargo CycloneDX 1.5 SBOMs, plus JSON/CSV licence declarations derived from locked
dependency metadata. Timestamps and SBOM identities are stable when `SOURCE_DATE_EPOCH`
is set to the tagged commit time. This evidence verifies existing platform signatures
and notarisation; it does not create them or replace representative installation,
Windows reputation review, or real signed-update promotion evidence.

The macOS application bundle includes `tufekci-paperworks-scanner` as a Tauri external
binary. Source remains in `src-tauri/native/macos-scanner`; generated architecture
binaries are never committed. CI compiles arm64 and x86_64 variants with the Apple SDK,
creates a universal binary with `lipo`, and rejects a helper missing either architecture.

The Apple mobile workflow runs separately on macOS 15. It generates the ignored Xcode
project, compiles the arm64 simulator target with signing disabled, identifies the main
application by bundle identifier, verifies iPhone and iPad device families, iOS 16,
simulator platform, document and indirect-input settings, and a non-empty executable,
then retains a ZIP plus executable/archive SHA-256 report. It does not create a signed
IPA or replace device, TestFlight, accessibility, privacy, and App Store review.
