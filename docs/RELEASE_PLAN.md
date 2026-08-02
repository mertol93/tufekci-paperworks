# Release Programme

This is the living delivery checklist for Tüfekci Paperworks. A checked item is
implemented and verified in the repository. Partially implemented work remains
unchecked until the full user workflow is dependable.

## Release-Critical Foundations

- [x] Local PDF.js worker and real page counts.
- [x] High-DPI page canvases and lazy thumbnails.
- [x] Password-protected PDF opening with local, non-persistent password prompts.
- [x] Existing-text search with progressive page scanning and cancellation.
- [x] Selectable text layer aligned with rendered PDF pages.
- [x] Display-only annotation and AcroForm appearance layers with local assets,
  cancelled stale renders, disabled scripts and links, and inert controls.
- [x] Bounded desktop PDF.js range loading with a 64 KiB initial sample, cancellable
  local reads, and source size and modification checks. Browser files remain
  memory-backed, and PDF.js may request all ranges for non-optimised documents.
- [x] Generated annotation/form and 320-page range-loading fixtures for local QA.
- [x] Generated, redistributable PDF.js rendering corpus covering malformed,
  AES-256 encrypted, certificate-structure, scanned image-only, CJK, RTL,
  unusually sized, and 320-page documents. Windows, macOS, and Linux CI validate
  password handling, page/operator decoding, text direction, annotations, geometry,
  bounded representative-page pixels, malformed rejection, and retained path-free
  evidence.

## Page Operations And Export

- [x] Typed, non-destructive page operation model.
- [x] Stable-ID single and multi-page selection with explicit toggles, modifier ranges,
  Select All, keyboard controls, ordered group drag, and group-aware Move Earlier/Later.
- [x] Single- or multi-page rotation, deletion and duplication as one undoable operation,
  plus paper-sized blank-page insertion.
- [x] Import selected or repeated page ranges from other PDFs through the verified
  merge/import workflow.
- [x] Insert password-aware selected ranges directly into the active thumbnail plan,
  with source-aware previews, text search, operation history, and certificate review.
- [x] Copy or move an ordered single- or multi-page selection into a concurrently visible,
  password-aware destination using drag insertion or an exact page boundary. Destination
  publication is create-new, fingerprinted and reopened; Move removes source pages only
  afterwards as one undoable operation. Rotation, blanks, imported provenance and visual
  marks are preserved, with certificate acknowledgement and optional AES-256 output.
- [x] Merge multiple PDFs with drag-and-drop or keyboard-accessible source ordering,
  per-source passwords, selected-page bookmark preservation with bounded hierarchy
  promotion and exact destination remapping, shared-job progress, cancellation,
  same-process reattachment, content-free retained failures, final source-fingerprint
  validation, and optional AES-256 output with decrypted second page-tree, bookmark-tree,
  and catalogue checks.
- [x] Split and extract by validated page groups, preparing and verifying every part
  before publication, cleaning up partial output sets on failure, and supporting
  progress, content-free retained failures, final source-fingerprint validation, and
  cancellation before publication. Optional AES-256 output prepares, decrypts, and
  verifies every protected part before any part is published.
- [x] Undo and redo for page reordering, rotation, deletion, duplication, and insertion.
- [x] Undo and redo direct imported-page insertion as one reversible plan operation.
- [x] Bounded undo and redo for standalone Merge source, order, and range planning and
  Split page-group planning, with password-free history snapshots.
- [x] Atomic export to a new file followed by structural verification and reopen.
- [x] Main organiser export through the shared typed scheduler with source/page/signature
  progress, cancellation before publication, same-process reattachment, secret-free
  snapshots, and start/final fingerprints for primary and imported PDFs.
- [x] Autosaved and manual local recovery drafts for PDF page plans and ordered image
  scans, with rotating create-new snapshots, startup Continue/Discard controls, and
  fallback when the newest snapshot is incomplete.
- [x] Recover unencrypted imported PDF sources and source-aware page identities without
  storing PDF bytes or passwords; suppress drafts that need an imported password.
- [x] Recover bounded standalone Merge source order and ranges and Split source and
  page-group plans after checking that each source remains present, without passwords,
  certificate acknowledgements, output protection, undo stacks, or job requests.
- [x] Add secret-free process-restart handling for long-running OCR, scan, signing,
  connected-scanner acquisition, and export jobs. A private per-job lock journal
  restores stale work once as an
  explicit non-resumable interrupted state, excludes every request, path, password,
  OCR hint, signature, document payload, progress value, error, and result. Publication
  users check the destination before a fresh retry, read-only work reruns from its
  current inputs, and scanner acquisition starts afresh after reviewing the device and
  feeder.

## Printing

- [x] Route `Ctrl+P` and `Command+P` to explicit print settings without starting a job.
- [x] Prepare all, current, or validated custom page ranges from the live organiser
  order, including imported, duplicated, rotated, and blank pages.
- [x] Use PDF.js print intent with printable annotations and current AcroForm values,
  and optionally flatten placed visual signatures and initials into each page image.
- [x] Bound preparation to 100 pages, 50 megapixels per page, and 120 megapixels per
  job; expose 150 and 300 dpi choices, progress, cancellation, retry, volatile preview
  URLs, and no document upload or persistent print cache.
- [x] Open the operating system dialogue for printer, copies, paper, colour,
  orientation, scaling, and duplex selection, with exact British, American, Turkish,
  and German interface catalogues.
- [ ] Retain Windows WebView2, macOS WKWebView, and Linux WebKitGTK evidence from both a
  real printer and a PDF target, covering mixed page sizes, portrait/landscape,
  duplex, colour/greyscale, copies, encrypted input, form values, annotations, visual
  marks, cancellation, and an unavailable print service. Keep printing experimental
  and disclose that prepared output is raster rather than vector until this passes.

## Scan And OCR

- [x] Multi-image intake and common paper/card presets.
- [x] Verified paper-sized image-to-PDF export without mutating source images.
- [x] EXIF auto-orientation, fitted placement, bounded decoding, selectable DPI,
  margins, JPEG quality, greyscale, and thresholded monochrome output.
- [x] Confidence-gated automatic cropping, projective perspective correction, shadow
  and uneven-lighting removal, cancellable processing, applied-page reports, and a
  selected-page before/after quality preview using the export pipeline and a distinct
  shared read-only job.
- [ ] Dependable searchable image-to-PDF export across the supported platforms.
- [x] Bounded OCR engine readiness, installed language-pack discovery, local selection,
  and fail-fast validation against the selected pack before image processing.
- [x] OCRmyPDF adapter for local text layers, optional deskewing, output reopening,
  page-by-page searchable-text verification, and page-specific review warnings.
- [x] Connect the standalone Recognise Text workflow for existing PDFs. The graphical
  workspace provides source and password selection, installed-language choice, deskew,
  engine readiness, certificate acknowledgement, optional AES-256 output, progress,
  cancellation, retry, and verified searchable-page coverage. Its strict shared job
  reuses the batch OCR engine path, keeps queued paths and passwords private, rechecks
  the source before create-new publication, and retains content-free failures.
- [x] Generated, redistributable English, Turkish, physically rotated, and noisy OCR
  fixtures using the bundled Liberation Sans font, with strict manifests, hashes,
  dimensions, UTF-8/LF expected text, UK English and Turkish character checks, bounded
  PNG decoding, and non-blank pixel evidence on Windows, macOS, and Linux CI.
- [ ] Engine-backed OCR corpus on all supported platforms. The Windows x64 gate passes
  locally with 100% observed recall in all four cases. Tagged drafts now require
  Windows, macOS, and Ubuntu reports containing engine versions, required language
  data, searchable-page coverage, bounded progress completion, and observed recall
  without recognised text or local paths. Keep this item open until the first tagged
  three-runner evidence set succeeds.
- [x] Cancellable scan/OCR export jobs with monotonic stage progress, bounded
  diagnostics, content-free retained failures, final source-image fingerprint checks,
  output verification, and cancellation before publication.
- [x] Optional QPDF AES-256 scan/OCR output after prepared-candidate verification, with
  decryption and repeated page, embedded-image, and searchable-text coverage checks.
- [x] Selected-page OCR confidence overlay and side-by-side correction view with
  bounded TSV parsing and temporary recognition hints. Hints are explicitly not
  presented as guaranteed text-layer replacements. Review uses a distinct shared
  read-only job with staged image preparation, ImageMagick and Tesseract process-tree
  cancellation, reattachment, one-time interrupted state, source/settings-free
  lifecycle snapshots, content-free failures, final source-image fingerprinting, and
  volatile recognised words excluded from persistence and diagnostics.
- [x] Shared typed connected-scanner contract with validated requests, app-owned
  capture storage, output confinement, size bounds, and stale-session clean-up.
- [x] Connected-scanner capture through the shared scheduler with staged progress,
  queued or running cancellation, complete adapter process-tree termination,
  same-process reattachment, fresh-setup retry, one-time interrupted recovery,
  device/settings-free queued snapshots, content-free failures, final page size and
  modification-time checks, sequential scan-review loading, and explicit discard after
  an opening failure.
- [x] Windows WIA and Linux SANE discovery and acquisition adapters with unit-tested
  parsing, bounded diagnostics, and explicit availability reports.
- [x] Capability-aware flatbed, feeder, duplex, page-limit, DPI, page-size, and colour
  controls for adapters and devices that report them.
- [x] Package the macOS Image Capture native bridge as an Intel, Apple Silicon, and
  universal Tauri sidecar, with Apple-SDK compilation and architecture checks in CI.
- [x] Add an ignored, private connected-scanner hardware harness with typed discovery,
  bounded acquisition, output confinement, image decoding, and a documented matrix.
- [ ] Physical-scanner corpus covering representative Windows WIA, macOS Image Capture,
  and Linux SANE flatbeds, feeders, duplex devices, driver failures, paper sizes, DPI,
  colour modes, page limits, disconnects, and empty feeders.

## Signatures, Encryption, And Trust

- [x] Named signatures and initials from freehand drawing, typed script/classic/modern
  artwork, or PNG/JPEG/WebP/BMP/TIFF images, with local background removal, transparent
  cropping, ink recolouring, and a bounded reusable session library.
- [x] Drag or button placement on any page, pointer and keyboard movement, proportional
  resize, arbitrary rotation, duplication, deletion, reuse, per-placement locking, and
  bounded undo/redo. Stable page identities keep placements attached through reordering.
- [x] QPDF AES-256 opening and administrator passwords.
- [x] Printing, copying, assembly, form, annotation, and editing permissions.
- [x] Password removal, verified output, error redaction, and source-overwrite prevention.
- [x] Password addition and removal through the shared typed scheduler, with bounded
  secret-bearing requests, secret-free snapshots, start/final source fingerprints,
  cancellable QPDF execution, bounded diagnostics, timeout enforcement, status retry,
  and same-process reattachment.
- [x] Flattened transparent visual-mark export with deduplicated image embedding,
  arbitrary mark angles across every page rotation, source protection, output reopening,
  and exact per-page resource-count verification before and after optional protection.
- [x] Optional signed-copy AES-256 passwords and no-change reader permissions using
  distinct opening and administrator passwords through QPDF.
- [x] Encrypted local visual-mark library with explicit deletion controls, version-two
  signature/initials and draw/image/type metadata, and backward-compatible version-one
  image-signature unlock. Passphrases, assets, and placement history remain outside
  recovery and public job snapshots.
- [ ] Certificate-backed visible and invisible signing through pyHanko. The bounded
  adapter, responsive interface, private passfile, incremental output, structural
  verification, post-signing integrity check, shared-job progress, cancellation,
  same-process reattachment, SHA-256 source/identity/trust-root snapshots and final
  rechecks, password-protected input, encryption-parity verification, bounded metadata,
  stable localised outcomes, and privacy-safe terminal snapshots are implemented. The
  generated disposable-identity corpus passes on Windows x64; the matching tagged
  Windows, macOS, and Linux evidence set is still required.
- [ ] Timestamping, trust-chain validation, and existing-signature inspection. HTTPS
  RFC 3161, PAdES validation data, optional trust roots, structural inspection, and
  typed pyHanko reports are implemented. Existing-signature inspection now has shared
  read-only job progress, cancellation, reattachment, path-free snapshots, content-free
  failures, bounded field reporting, and final PDF/trust-root SHA-256 rechecks. Trusted
  and intact-but-untrusted Windows x64 evidence passes; live modified, revoked, expired,
  TSA-failure, and timestamp-enabled three-platform evidence remains outstanding.
- [x] Generated certificate engine gate for visible and invisible incremental signing,
  encrypted-input signing with preserved protection, save-and-reopen verification, two
  signatures, trusted validation, and integrity/trust separation. It creates and removes
  a disposable identity at runtime, retains only a closed path-free report, and makes
  RFC 3161 plus PAdES mandatory in tagged release jobs without committing a private
  identity or passphrase.
- [x] Lightweight post-open certificate-signature warning before page-plan edits or
  rewritten export of the active workspace PDF, with edit controls gated until the
  check completes.
- [x] Explicit certificate acknowledgement and trusted-command enforcement for active
  organiser export and directly imported PDF sources.
- [x] Mandatory password-aware certificate-signature checks for standalone Merge,
  Split, Protect, and Privacy Cleaner sources, with explicit acknowledgement and a
  trusted-command guard before publication.
- [x] Mandatory password-aware certificate-signature checks for Batch Recipes, with a
  bounded shared preflight, explicit acknowledgement, and trusted-command enforcement.

## Editing And Safety

- [x] Native-reviewed existing page-content editing for exact unshared `Tj` text runs
  and unambiguous page-level image paint blocks. The graphical workspace provides page
  overlays, keyboard and pointer selection, image dragging and percentage geometry,
  replacement or removal, original-font text validation, and a bounded 100-step
  undo/redo history. Inspection and publication use separate shared jobs, full SHA-256
  source binding, certificate acknowledgement, content-free retained errors,
  cancellation, optional AES-256 output, create-new publication, and reopened checks
  for edited and untouched streams, image pixels, forms, bookmarks, and annotations.
  Complex text operators, shared or nested content, arbitrary vectors, and layout
  reflow remain preserved read-only.
- [x] Bounded graphical text, image, stamp, highlight, freehand, rectangle, ellipse,
  and line annotations with drag creation and movement, selection, properties,
  duplication, deletion, 100-step undo/redo, standard PDF dictionaries, generated
  appearances, rotated-page mapping, existing-annotation and form preservation,
  changed-source rejection, certificate acknowledgement, create-new publication,
  shared-job progress and cancellation, content-free retained errors, optional AES-256
  output, and exact marker verification after reopening. Self-contained existing
  FreeText, single-quad highlight, stamp, single-stroke ink, square, circle, and plain
  line annotations support move, property edit, duplicate, delete, undo, and redo.
  Publication separates additions, updates, and removals, validates reviewed source
  identities, preserves every unsupported item, and verifies exact per-page totals.
  Direct-object, linked, rich, multi-part, structurally complex, and over-limit
  annotations remain read-only; appearance text uses Windows Latin while full Unicode
  remains in annotation contents and unsupported appearance glyphs are reported.
- [x] Bounded AcroForm filling for text, checkbox, radio, and choice fields with
  hierarchical inspection, searchable page-linked graphical controls, exact UTF-16
  values, generated appearances, required-field validation, 100-step undo/redo,
  optional safe flattening, XFA rejection, signed and changed-source guards, preserved
  unsupported fields and annotations, shared-job progress and cancellation, content-free
  retained errors, optional AES-256 output, and verified create-new reopening checks.
- [x] Bounded visual-edge crop, proportional paper resize, watermark, token-aware header
  and footer, and selected-range Bates-number workflows with rotation-aware graphical
  previews, A3/A4/A5/Letter/Legal/custom paper, annotation and form-widget transforms,
  non-redaction warnings, signed and changed-source guards, preserved forms/bookmarks,
  shared-job progress and cancellation, content-free retained errors, optional AES-256
  output, and verified page boxes, annotation counts, operation markers, and mark layers.
- [x] AES-256 export and encrypted-input handling across every structural workflow.
  Scan/OCR, Batch Recipes, organiser, signed-copy, merge, split, privacy-clean,
  compression, bookmark, annotation, page-content, permanent-redaction, Page Finish, and AcroForm
  workflows now support protected output; certificate signing and validation now accept
  password-protected input while preserving its encryption; scan/OCR, Batch Recipes,
  merge, split, privacy-clean, compression, bookmark, annotation, page-content, permanent-redaction,
  Page Finish, and form publication perform a second decrypted structural,
  content-coverage, or privacy verification. An exhaustive shared-job contract covers
  all thirty-three job kinds and prevents a future publisher from omitting its protection
  policy. PDF/A is the explicit standards-driven exception: it accepts protected input
  through a private temporary unlock but publishes only unencrypted archival output.
- [x] Real redaction through lossless raster replacement of every marked page. The
  interface supplies a clean bounded page raster and reviewed normalised regions; Rust
  rejects malformed or excessive geometry, applies one-pixel-expanded black or white
  masks in reviewed order, and records the exact flattened RGB SHA-256 digest. Global
  metadata, actions, attachments, annotations, forms, bookmarks, named destinations,
  thumbnails, and tagged-document structures are removed; unreachable objects are
  pruned; and exact image-only page, marker, page-count, pixel-digest, privacy, and
  searchable-text checks run after reopening and after decrypting optional AES-256
  output.
- [x] Permanent-redaction publication through the shared typed scheduler, with bounded
  secret-bearing request memory, secret-free snapshots, page/pixel/verification progress,
  cancellation before publication, same-process reattachment, and worker-start/final
  source-fingerprint checks, content-free retained errors, and optional AES-256 output
  with decrypted image-only and privacy re-verification.
- [x] Search-assisted redaction for text and names, email addresses, and bounded
  user-defined wildcard patterns. Results remain selectable review suggestions and are
  never committed automatically.

## Product Differentiators

- [x] Local document health preflight for encryption, signatures, forms, XFA,
  scripts, actions, attachments, metadata, bookmarks, oversized images, unusual
  page geometry, and bounded page-stream inspection.
- [x] Bounded technical-integrity diagnostics for dangling objects, excessive nesting,
  strict page-content parsing, missing resources, font embedding and Unicode maps,
  structural output intents and ICC profiles, unmanaged Device CMYK, grouped findings,
  and explicit report truncation.
- [x] Bounded binary ICC-profile validation for header signature, declared size,
  version, class, data-space/component agreement, PCS, date, rendering intent, reserved
  bytes, and a 4,096-entry in-range tag table, with a 16 MiB decompression cap.
- [x] Cycle-safe recursive Form XObject auditing for content, named resources, fonts,
  colour spaces, images, ICC profiles, malformed streams, and inherited resources,
  bounded to 32 levels and 100,000 page-specific contexts with disclosed limits.
- [x] PDF/A-1b, PDF/A-2b, and PDF/A-3b conversion and validation reports using explicit
  OCRmyPDF/Ghostscript output profiles, independent matching-profile veraPDF verdicts,
  bounded generic failure summaries, unencrypted/page-count reopening checks, source
  revalidation, and create-new publication.
- [x] Validation-only PDF/UA-1 and PDF/UA-2 reports through the pinned veraPDF adapter,
  with exact `ua1` or `ua2` flavour selection, bounded generic rule summaries,
  cancellation, source fingerprinting, and an explicit exact-unprotected-source rule.
- [x] Bounded built-in PDF/X-1a:2001, PDF/X-3:2002, and PDF/X-4 structural preflight
  reports for profile IDs, trapping, encryption, GTS_PDFX output intents, binary ICC
  integrity, embedded fonts, object integrity, page boxes, scripts, forms, attachments,
  external content, non-printing media, and transfer curves. Reports identify themselves
  as preflight and never claim ISO 15930 conformance.
- [ ] PDF/UA remediation, PDF/UA and PDF/X conversion, independent PDF/X certification,
  profile-specific colour and transparency semantics, required ICC tag sets and tag-type
  semantics, colourimetric proofing, and retained three-platform standards corpora.
- [x] Verified privacy cleaner for document information, XMP, identifiers, private
  application and Web Capture history, JavaScript, automatic and launch actions,
  attachments, annotations, form fields, and page thumbnails, with inspected-source
  fingerprinting, full rewrite, reopening verification, unreachable-object pruning,
  final source revalidation, and optional AES-256 output with decrypted category checks.
- [x] Bounded deeper privacy inspection for optional and default-hidden layers,
  invisible text, zero-opacity drawing, hidden annotations, cropped-away artwork,
  Web Capture provenance, embedded PDX indexes, and declared private-extension
  containers, with finding-linked safe cleaner switches and explicit review-only limits.
  Inspection uses a distinct shared read-only job with bounded staged traversal,
  cancellation, reattachment, one-time interrupted state, source/password-free
  lifecycle snapshots, path-free reports, content-free failures, and a final exact
  source fingerprint.
- [x] Duplicate-page and blank-page detection with review before removal.
- [x] Preservation-first compression preview with a 40-95 image-quality control,
  representative decoded source/candidate comparison, exact dry-run output size,
  bounded compatible-image processing, explicit non-recompressed-image reporting, and
  verified create-new export that preserves text, vectors, links, forms, and OCR layers.
  Preview analysis has shared read-only job progress, cancellation, reattachment,
  source/password-free lifecycle snapshots, content-free failures, path-free report
  metadata, and a final exact source fingerprint. Publication has content-free retained
  errors, a final source-fingerprint gate, and optional AES-256 protection whose
  decrypted candidate repeats page, form, and bookmark checks and must remain smaller
  than the source.
- [x] Local visual and textual PDF comparison with independent encrypted-input
  passwords, progressive bounded text and geometry checks, added and removed page
  detection, changed-page filtering, and a cancellable selected-page pixel map.
- [x] Bookmark editing and heading-based navigation generation with bounded Unicode
  tree inspection, branch-aware hierarchy controls, page preview, style preservation,
  reviewed-source fingerprinting at worker start and immediately before publication,
  verified create-new export, optional AES-256 output passwords, and explicit selection
  of streamed font-size and numbered-heading suggestions. Optional printed contents
  insert bounded linked A4 pages with an embedded Unicode font, shift source and outline
  destinations together, optionally add a top-level contents bookmark, and verify every
  generated page, text stream, font, link destination, and protected candidate before
  publication.
- [x] Reusable local batch recipes for up to fifty inspected PDFs using searchable OCR
  with optional deskew, privacy-clean and compression steps, with built-in and
  settings-only custom recipes, per-source passwords, signature review, safe output
  naming, prepare-all-before-publish execution, cancellation, content-free retained
  errors, itemised searchable-page results, and optional shared AES-256 output applied
  once after the final verified workspace copy.
- [x] Engine-backed batch OCR and deskew with language selection, version-one saved
  recipe migration, protected-source temporary unlock, page-count preservation,
  searchable-layer coverage warnings, and direct public-corpus evidence.
- [x] Extend batch recipes to PDF/A-1b, PDF/A-2b, and PDF/A-3b conversion after OCR,
  privacy cleaning, and compression, with validation-before-publication, protected-input
  temporary unlock, encryption exclusion, built-in recipe, and saved-recipe migration.
- [x] Extend batch recipes to scanner intake and other reviewed multi-step archives.
  Connected-scanner and local-image batches first complete the reviewed scan clean-up,
  optional OCR, create-new publication, and reopening checks, then expose a direct
  completion-banner hand-off to Batch Recipes. The transient seed contains only the
  verified PDF path and a generic origin label; scanner details, image bytes, settings,
  OCR hints, and passwords are excluded, and protected scans must be unlocked again.
- [x] Static accessibility preflight for title display, language, tags, structure-tree
  consistency, page structure links, RoleMap-aware Figure alternative text, and
  interactive tab-order signals, with reading-order correctness reserved for manual
  assistive-technology review.
- [x] PDF/A conversion and validation reports with a mandatory three-platform tagged
  release evidence gate.

## Long-Running Work

- [x] One shared bounded FIFO manager for scan/OCR creation, standalone searchable OCR,
  compression, privacy
  cleaning, connected-scanner acquisition, batch source review and publication, merge, split, organiser
  export, page-import review, bookmark publication and review, annotation publication and review, Page Finish
  publication and review, form publication and review,
  certificate signing, permanent redaction publication and review, password protection, PDF standards reporting,
  Document Health, edit-safety inspection, certificate validation, compression preview,
  Privacy Inspection, OCR confidence review, and scan clean-up preview, with two
  workers, sixteen non-terminal jobs, thirty-two retained typed
  secret-free request snapshots, monotonic progress, cancellation before publication
  or report delivery, retrying polls, generic typed lifecycle commands, and same-process
  reattachment.
- [x] Complete the long-running execution boundary. Every native structural publisher
  and validator uses the thirty-three-kind shared scheduler with process-restart interrupted
  recovery. An exact Tauri handler allow-list prevents parallel direct wrappers; direct
  commands are limited to bounded transport and reviewed support services. PDF.js
  rendering, search, previews, and comparison use a separate bounded WebView lifecycle
  with loading-task destruction, stream and render cancellation, page clean-up, stale-run
  rejection, and progressive comparison reporting.
- [x] Batch Recipe source review through one distinct shared read-only job for up to
  fifty unique canonical PDFs, with mapped per-file Privacy Inspection progress,
  immediate cancellation, safe retry, same-process reattachment, one-time interrupted
  recovery, source/password-free snapshots, ordered path-free results, content-free
  per-file failures, and final source fingerprinting. The direct Privacy Inspection IPC
  command is removed.
- [x] Remove obsolete direct Tauri wrappers for scheduler-backed scan export,
  certificate validation, compression preview, Document Health, scan clean-up preview,
  OCR confidence review, scanner capture, and edit-safety inspection. Scan creation now
  uses the generic typed lifecycle directly. Keep synchronous helpers internal to
  backend enforcement or native tests and guard the registered handler with a static
  regression.
- [x] Shared edit-safety review through one aggregate read-only job for up to 250 PDFs,
  with debounced starts, exact stale-job cancellation, mapped per-source progress,
  bounded passwords and structural parsing, cancellation, safe retry, one-time
  interrupted-state recovery, source/password-free snapshots, ordered path-free
  results, content-free per-source failures, and a final source fingerprint before
  every successful result is delivered.
- [x] Selected-page import review through a distinct shared read-only job with bounded
  path, password, range expression, expanded selection, and certificate-structure
  traversal; staged progress; queued or running cancellation; safe retry; same-process
  reattachment; one-time interrupted-state recovery; source/password/range-free
  snapshots; content-free failures; and an exact final source fingerprint before the
  typed selected-page report is delivered.
- [x] Redaction review through a distinct shared read-only job with bounded request,
  page-geometry, and annotation traversal; staged progress; queued or running
  cancellation; safe retry; same-process reattachment; one-time interrupted-state
  recovery; source/password-free snapshots; content-free failures; and an exact final
  source fingerprint before the typed destructive-workspace model is delivered.
- [x] Page Finish review through a distinct shared read-only job with bounded request,
  page-geometry, and annotation traversal; staged progress; queued or running
  cancellation; safe retry; same-process reattachment; one-time interrupted-state
  recovery; source/password-free snapshots; content-free failures; and an exact final
  source fingerprint before the typed workspace model is delivered.
- [x] Form review through a distinct shared read-only job with bounded request,
  page/widget, option, and recursive field-tree traversal; staged progress; queued or
  running cancellation; safe retry; same-process reattachment; one-time interrupted
  recovery; source/password-free snapshots; content-free failures; and an exact final
  source fingerprint before the typed field model is delivered.
- [x] Bookmark review through a distinct shared read-only job with bounded request,
  named-destination and outline traversal; staged progress; queued or running
  cancellation; safe retry; same-process reattachment; one-time interrupted-state
  recovery; source/password-free snapshots; content-free failures; and an exact final
  source fingerprint before the typed tree is delivered.
- [x] Annotation review through a distinct shared read-only job with bounded request
  validation, page-level progress, a 2,000-entry overall and 500-entry per-page editable
  extraction ceiling, stable indirect-object source and PDF.js viewer identities,
  read-only accounting, queued or running cancellation, safe retry,
  same-process reattachment, one-time interrupted-state recovery, source/password-free
  queued snapshots, content-free failures, and an exact final source fingerprint before
  the typed report is delivered.
- [x] Document Health through the shared typed scheduler with staged object, font,
  colour, accessibility, page, content-operator, and nested Form progress; queued or
  running cancellation; same-process reattachment; one-time process-restart
  interrupted state; content-free retained failures; request-path and password
  exclusion from public queued snapshots; and exact final source size/modification-time
  validation before report delivery.
- [x] Certificate validation through a distinct shared read-only job with staged
  structural, engine, integrity/trust, and final-input progress; bounded recursive
  traversal and pyHanko process-tree cancellation; same-process reattachment; one-time
  interrupted state; path-free queued and successful snapshots; content-free retained
  failures; and exact final PDF and trust-root fingerprints.
- [x] Compression preview through a distinct shared read-only job with staged
  compatible-image, stream-size, sample-encoding, and final-source progress; queued or
  running cancellation; same-process reattachment; one-time interrupted state;
  source/password-free lifecycle snapshots; path-free retained report metadata;
  content-free failures; and an exact final source fingerprint before volatile sample
  delivery.
- [x] Privacy Inspection through a distinct shared read-only job with staged
  direct-object, optional-content, resource, annotation, and page-stream traversal;
  queued or running cancellation; same-process reattachment; one-time interrupted
  state; source/password-free lifecycle snapshots; path-free retained reports;
  content-free failures; and an exact final source fingerprint before report delivery.
- [x] OCR confidence review through a distinct shared read-only job with staged image
  preparation; cancellable ImageMagick and Tesseract process trees; same-process
  reattachment; one-time interrupted state; source/settings-free lifecycle snapshots;
  content-free failures; exact final source-image fingerprinting; and recognised words
  retained only in volatile typed result state.
- [x] Scan clean-up preview through a distinct shared read-only job with debounced
  automatic starts; stale-configuration cancellation; staged bounded decoding,
  clean-up, and JPEG encoding; same-process reattachment; one-time interrupted state;
  source/settings-free lifecycle snapshots; content-free failures; exact final
  source-image fingerprinting; and preview bytes retained only in volatile typed
  result state.
- [x] Stage progress, automatic status retry, final source-image checks, content-free
  retained errors, cancellation, and optional AES-256 output for scan/OCR export,
  including decrypted page, image, and searchable-text coverage verification.
- [x] Stage progress, automatic status retry, and cancellation for compression and
  privacy-clean export, including checkpoints inside image and PDF-object loops.
  Both have a final source gate, content-free retained errors, and optional AES-256
  output with decrypted re-verification of their respective safety checks.
- [x] Stage progress, automatic status retry, and cancellation for standalone merge
  and split, including source/page/part checkpoints, content-free retained errors,
  final source-fingerprint checks, and a publication gate. Merge also supports optional
  AES-256 output with decrypted page-tree re-verification; Split verifies a complete
  protected set before publishing any part.
- [x] Stage progress, automatic status retry, source-fingerprint rechecks, and
  cancellation for the main organiser export.
- [x] Stage progress, automatic status retry, source-fingerprint rechecks, and
  cancellation for permanent redaction, including checkpoints inside page and pixel
  work, content-free retained errors, optional AES-256 output with decrypted redaction
  and privacy re-verification, and a final publication gate.
- [x] Stage progress, automatic status retry, source-fingerprint rechecks, and
  cancellation for AES-256 password addition and removal, including child-process
  termination, bounded diagnostics, timeout enforcement, and a final publication gate.
- [x] Stage progress, automatic status retry, source-fingerprint rechecks, queued or
  running cancellation, same-process reattachment, and optional AES-256 output for
  bookmark publication, including a post-encryption decrypted structure check.
- [x] Stage progress, automatic status retry, source-fingerprint rechecks, queued or
  running cancellation, same-process reattachment, content-free retained errors, and
  optional AES-256 output for annotation publication, including stable-marker and
  embedded-image verification after decrypting the protected candidate.
- [x] Stage progress, automatic status retry, source-fingerprint rechecks, queued or
  running cancellation, same-process reattachment, content-free retained errors, and
  optional AES-256 output for Page Finish publication, including page-box, structure,
  operation-marker, and mark-layer verification after decrypting the protected candidate.
- [x] Stage progress, automatic status retry, source-fingerprint rechecks, queued or
  running cancellation, same-process reattachment, content-free retained errors, and
  optional AES-256 output for form publication, including stable-name verification
  after decrypting the protected candidate.
- [x] Stage progress, automatic status retry, final source-fingerprint validation,
  controlled pyHanko termination, queued or running cancellation, same-process
  reattachment, stable localised stage/failure codes, and sanitised retained results for
  certificate signing and validation.
- [x] Fresh-setup retry controls and selectable, clipboard-assisted diagnostics for all
  scheduled workflows, without retaining requests or including typed result payloads.
- [x] Engine-level OCR percentage in addition to the bounded overall scan stage. A
  one-use OCRmyPDF plug-in emits content-free machine records, a 16 KiB streaming parser
  also accepts guarded Rich/tqdm OCR output, and monotonic engine 0–100% maps to overall
  scan 76–90%. Malformed, unrelated, duplicate, decreasing, and oversized records are
  ignored; process cancellation and the ignored cross-platform corpus progress contract
  are tested.
- [x] Cross-platform descendant-process containment for long-running QPDF, pyHanko,
  OCRmyPDF, Tesseract, ImageMagick, WIA, Image Capture, and SANE work. Windows uses a
  suspended launch assigned to a kill-on-close Job Object before resume; macOS and Linux
  use an isolated process group. Cancellation, timeout, monitor failure, parent exit,
  and wrapper drop terminate the whole tree, with a real parent-plus-grandchild
  regression test.
- [x] Isolated temporary workspaces with reliable clean-up after success, failure, or
  crash. A private lock-backed lease registry covers temporary PDF candidates, batch
  directories, certificate input workspaces and passfiles, pyHanko password bridges,
  OCR hints, OCR progress plug-ins, and scan rasters;
  start-up clean-up is bounded and path-confined, skips live locks, validates directory
  ownership tokens, rejects links and unexpected types, and excludes seven-day scanner
  captures.
- [x] Operation audit history that excludes passwords and document content. All thirty-three
  scheduled workflows record terminal outcomes exactly once in a
  cross-process-locked, three-generation local store capped at 500 entries and 512 KiB.
  Entries exclude job IDs, stages, errors, warnings, filenames, paths, passwords,
  document content, and typed results. The Activity panel provides outcome filtering,
  refresh, create-new path-free JSON export, and confirmed physical clearing.

## Release Engineering

- [x] AGPL-3.0-or-later licence, contributor guide, security policy, and issue templates.
- [x] Windows, macOS, and Linux CI checks.
- [x] Checked-in iPhone/iPad build foundation: iOS/iPadOS 16 configuration, shared
  mobile Rust entry point, typed runtime capabilities, backend rejection of
  desktop-only jobs, App Store-managed update state, safe-area/dynamic-viewport/touch
  interaction contracts, and a credential-free macOS 15 arm64-simulator workflow with
  bundle-identity, device-family, executable, archive, and SHA-256 evidence checks.
- [x] Retain the first hosted Apple mobile workflow result. Run `30766986118` passed
  the arm64 simulator compile, bundle metadata, iPhone/iPad family, archive, hash, and
  evidence-upload gates.
- [ ] Run the complete compact/regular-width, portrait/landscape, Files picker,
  keyboard/pointer, VoiceOver,
  Dynamic Type, reduced-motion, low-memory, document-integrity, and disabled-feature
  matrix on representative iPhone and iPad simulators and physical devices.
- [ ] Create a credential-backed iOS distribution archive and retain installation,
  TestFlight, privacy-manifest and store-metadata review, screenshots, crash diagnostics,
  accessibility evidence, and App Store review. Apple account, team, certificate,
  provisioning, and App Store Connect material must remain outside the repository.
- [x] Tagged draft-release workflow, Windows MSI/NSIS bundles, and a universal macOS
  Intel/Apple Silicon target.
- [x] One explicit `0.1.0-alpha.1` version across npm, both npm lockfile roots, Cargo,
  and Tauri, with exact `v<version>` tag validation before platform builds and a
  derived GitHub pre-release flag. WiX uses the separately validated numeric package
  version `0.1.0.1`, mapped from the final prerelease sequence.
- [ ] Signed Windows installers and reputation testing. Protected non-exportable PFX
  import, exact signer-thumbprint binding, SHA-256 signing, trusted timestamps, cleanup,
  and fail-closed package evidence are implemented; the first real certificate-backed
  run, reputation review, and representative installations remain required.
- [ ] Signed and notarised macOS Intel and Apple Silicon builds. Ephemeral Developer ID
  import, exact team binding, secure timestamps, App Store Connect submission, cleanup,
  Gatekeeper assessment, and stapled-ticket verification are implemented; the first real
  credential-backed universal build and representative installations remain required.
- [ ] Linux AppImage, deb, and rpm packages tested on supported distributions. Tagged
  drafts now build on Ubuntu 22.04, structurally inspect all three formats, require one
  byte-identical x64 ELF payload, extract the AppImage on Ubuntu 22.04, install and link
  the deb on Debian 13, and install and link the rpm on Fedora 43. Keep this item open
  until the first retained tagged evidence set passes and representative user-machine
  installation is reviewed.
- [x] Strict native package evidence before release metadata: exact format inventories,
  release identity, architecture, container signatures, package-manager metadata,
  bounded filenames and sizes, streamed SHA-256, Windows/macOS publisher-signature
  state, exact expected signer identity, timestamps, macOS Gatekeeper and stapled-ticket
  state, path-free reports, and one cross-platform aggregate. Tagged builds require
  `signed-required` with the generated public signing contract; unsigned local evidence
  remains diagnostic only.
- [x] Tagged-release SHA-256 checksums, exact artefact manifest, npm and Cargo
  CycloneDX 1.5 SBOMs, and a combined dependency-licence declaration report with
  explicit manual-review flags and retained workflow evidence.
- [x] Fail-closed signed-update infrastructure with explicit alpha, beta, and stable
  channels; user-triggered checks; mandatory signature verification; protected signing
  and promotion environments; strict immutable-manifest evidence; byte-verified channel
  promotion; and documented withdrawal, forward-version rollback, manual recovery, and
  key rotation in [UPDATES.md](UPDATES.md).
- [ ] First real credential-backed signed updater evidence across Windows, universal
  macOS, and Linux, followed by approved channel promotion, packaged update/restart
  testing, and a retained withdrawal plus higher-version rollback rehearsal.
- [ ] End-to-end tests across the supported operating-system matrix. An eleven-case native
  Tauri/WebDriver suite now covers shell readiness, keyboard and modal focus, real PDF
  loading, page drag reordering plus structural operations, PDF.js pixels and search,
  native-reviewed page-text replacement with undo/redo and verified publication, and
  linked printed-contents preview and verified publication, merge-source drag ordering
  with selected-page bookmark preservation, plus image/typed/freehand visual-mark
  creation, drag placement, movement, resize, rotation, duplicate, undo/redo, lock,
  exact-count flattening, export and reopen, and explicit Turkish/German interface
  switching across organiser, Split, Protect, Compression, Activity, and signed Updates
  with persisted locale and root-language checks. Windows x64 passes
  locally; CI and tagged-release matrices now run Linux, macOS, and Windows and retain
  strict path-free evidence. Keep this item open until the first complete hosted matrix
  has passed and its retained reports have been reviewed.
- [x] Automated application accessibility baseline: document-editor skip navigation,
  high-contrast visible focus, a roving workflow tab set, labelled landmarks, and shared
  initial focus, Tab containment, safe Escape, and focus return for all thirteen modal
  workspaces, with source contracts and a public manual test matrix.
- [ ] Accessibility audit, keyboard-only pass, and assistive-technology smoke tests on
  the final packaged Windows, macOS, and Linux release candidates, retaining the
  non-sensitive evidence required by [ACCESSIBILITY_TESTING.md](ACCESSIBILITY_TESTING.md).
- [x] Typed local interface-locale foundation with exact `en-GB`, `en-US`, `tr-TR`, and
  `de-DE` catalogues; British English default and fallback; persisted explicit choice;
  root `lang`; placeholder and formatting contracts; and native switching acceptance
  for workflow navigation, Merge, output protection, trust warnings, and shared jobs.
  The full visual-signature and encrypted-vault interface, Split, password protection,
  Compression, local Activity, signed Updates, their accessible controls, stable artwork
  errors, and stable split/protection/compression native stages are migrated in all four
  catalogues. Document Health, Privacy Cleaner, PDF Comparison, and Page Finish now add
  complete translated controls and outcomes, stable health/privacy/finishing stages and
  failures, closed finding and warning mappings, locale-aware diagnostics, and Turkish
  and German native GUI acceptance. Annotation and Forms now also provide complete
  translated workspaces, locale-aware validation and results, stable inspection and
  publication codes, and closed warning mappings with native GUI switching coverage.
  Page Content and Permanent Redaction now provide the same complete four-locale
  controls, accessibility names, validation, search and export results, stable
  inspection/publication codes, closed warning mappings, and Turkish/German native GUI
  switching coverage. PDF Standards and Batch Recipes now also provide complete
  four-locale controls, engine and source readiness, profile and recipe choices,
  validation and per-file results, stable archive/batch/inspection codes, closed
  outcome mappings, locale-aware counts and sizes, and Turkish/German native GUI
  switching coverage. Bookmarks now provides the same complete coverage for outline
  editing, heading review, generated printed contents, stable inspection/export codes,
  closed warning mappings, locale-aware results, and Turkish/German native GUI
  switching. Certificate signing and validation now provide the same complete coverage
  for signing settings, trust selection, bounded field review, stable codes, closed
  warning mappings, locale-aware results, and Turkish/German native GUI switching.
- [ ] Complete localisation of every remaining interface, native stage and error,
  installer, accessibility name, and user guide, followed by expansion,
  restart, screen-reader, packaged three-platform, and fluent linguistic review under
  [LOCALISATION.md](LOCALISATION.md).
- [x] Public feature-status documentation that distinguishes complete, experimental,
  and unavailable capabilities; records optional local engines and platform evidence;
  states signature, permission, redaction, privacy, OCR, recovery, and publisher-signing
  security boundaries; and retains an explicit alpha release-gate checklist.
- [x] Mandatory source-tree audit in pull-request, main-branch, and tagged-release
  preflight, covering required public files, generated/private path exclusion, bounded
  source size, reviewed file types, strict UTF-8/LF text, credential signatures, and
  personal absolute home paths.
- [x] Deterministic, create-new source release from the exact audited Git index, with
  post-write ZIP path and byte verification, an exact per-file manifest, SHA-256
  checksum, retained workflow evidence, and attachment to the draft GitHub release.

## Release Line

The first public release candidate remains `0.1.0-alpha.1`. It becomes eligible when real
rendering, core page operations, image-to-PDF scanning, English OCR, visual
signature flattening, and dependable verified export are complete. The certificate,
connected-scanner, and AcroForm workflows are implemented but retain
their documented alpha release gates. Permanent redaction is implemented with its
documented destructive raster and accessibility trade-offs. Advanced automation may
ship in later milestones, and incomplete interfaces must remain clearly marked until
their end-to-end workflows are operational. The iPhone/iPad target is an experimental
build foundation within this release line, not an App Store release: only the
self-contained local core is enabled until the mobile evidence and distribution gates
above have passed.
