# Roadmap

## Milestone 0: Project Foundation

- Create the Tauri, React, and Rust scaffold.
- Add project docs, build notes, and contribution guidelines.
- Add local tool probing for QPDF, ImageMagick, img2pdf, OCRmyPDF, Tesseract, and pyHanko.
- Define the typed job model for PDF and scan operations. Completed with one shared
  bounded FIFO queue for scan/OCR, scan clean-up preview, connected-scanner acquisition,
  compression, privacy cleaning, batch source review and publication, page-content
  review and publication,
  merge, split, organiser export, page-import review, bookmark publication and review, annotation publication and review, Page
  Finish publication and review, form publication and review, certificate publication,
  permanent redaction publication and review, password protection,
  PDF/A archival, Document Health, certificate validation, compression preview,
  Privacy Inspection, edit-safety inspection, and OCR confidence review, with typed
  public snapshots, progress, cancellation, retry, same-process live reattachment, and
  process-restart interrupted-state recovery.
- Reduce the direct native IPC surface. Completed for every native structural publisher
  and validator. An exact handler allow-list regression permits only the four generic
  scheduler commands plus bounded document transport, readiness, recovery, audit,
  signature-vault, scanner-discovery, and status services. PDF.js rendering and
  comparison keep a separate bounded, progressive, cancellable WebView lifecycle.
- Add safe process-restart handling for scheduled jobs. Completed with a private
  lock-backed, secret-free active-job journal, live-instance exclusion, strict bounded
  records, one-time interrupted terminal snapshots, and fresh retries that never retain
  or replay an old request. Because interruption can follow publication, publication
  users are told to check the chosen destination first; read-only checks run again from
  current inputs.
- Contain long-running external tool descendants. Completed with kill-on-close Windows
  Job Objects and separate macOS/Linux process groups across PDF, OCR, image,
  certificate, and scanner adapters, including explicit termination and a
  parent-plus-grandchild regression test.
- Add crash-safe temporary-workspace ownership. Completed with a private, bounded
  lock-backed lease registry for app-owned PDF candidates, batch folders, certificate
  passfiles, OCR hints, and scan rasters. Start-up clean-up validates canonical parents,
  exact names, ordinary file types, and batch ownership tokens while preserving live
  leases and the scanner-capture seven-day recovery policy.
- Add a local operation audit. Completed for all thirty-three scheduled
  workflows with exactly-once terminal records, a sparse path-free schema,
  cross-process write locking, three-generation recovery, strict 500-entry/512-KiB
  bounds, outcome filtering, create-new JSON export, and confirmed clearing.

## Milestone 1: Page Operations MVP

- Open a PDF from disk. Completed for browser and desktop file intake.
- Render page previews and thumbnails. Completed with PDF.js, lazy thumbnails,
  password prompts, zoom, real page counts, locale-aware document text search with
  retryable page caches and content-free failures, and safe translated display-only
  annotation and AcroForm appearance layers. Desktop primary, imported, and recovered
  PDFs now use bounded local range loading with source-change checks.
- Merge multiple PDFs. Completed with drag-and-drop and keyboard-accessible source
  ordering, selected ranges, per-source passwords, resolved selected-page bookmark
  preservation and exact remapping, verified output, shared-job progress, cancellation,
  same-process reattachment, content-free retained failures, final multi-source
  fingerprint validation, optional AES-256 output with decrypted page-tree and
  bookmark-tree checks, typed four-locale progress/failure/warning outcomes, and bounded
  undo/redo for source additions, removals, ordering, and ranges. History snapshots omit
  source passwords.
- Split by explicit page ranges. Completed with semicolon-separated output groups,
  prepare-before-publish verification, partial-output clean-up, shared-job progress,
  content-free retained failures, final source validation, and cancellation before
  publication. Optional AES-256 output prepares and decrypts every protected part for
  repeated structural verification before any part is published. Page-group planning
  has a bounded history that resets for each newly selected source.
- Delete, duplicate, and rotate selected pages. Completed for stable-ID single or
  multi-selections as one reversible page-plan operation.
- Reorder pages with drag and drop. Completed for one page or an ordered multi-selection,
  with explicit selection toggles, modifier ranges, Select All, keyboard-modified Space,
  and group-aware Move Earlier and Move Later controls as keyboard alternatives.
- Insert blank pages. Completed for A4, US Letter, business card, ID card, and
  driving licence dimensions.
- Import selected pages from another PDF. Completed both through the merge/import
  studio and direct source-aware insertion into the active thumbnail plan, including
  previews, search, passwords, certificate review, and undo/redo. Direct insertion now
  begins with a distinct cancellable shared review job, keeps its complete request out
  of public and interrupted snapshots, and rechecks the source fingerprint before the
  selected-page report is returned.
- Copy or move selected pages between documents. Completed through a bounded
  two-document workspace with explicit group drag or numeric insertion, password-aware
  destination review, stable source deduplication, rotations, blank geometry, imported
  provenance, visual marks, certificate acknowledgement, optional AES-256 output and
  verified create-new publication. Copy leaves the source plan unchanged. Move removes
  pages only after destination reopening succeeds, and the source removal is one
  undoable operation; publishing the remaining source remains a separate explicit export.
- Save to a new output PDF. Completed with native source selection, atomic
  non-overwriting publication, structural verification, output reopening, shared-job
  progress and cancellation, same-process reattachment, and start/final fingerprints
  for every primary and imported source.
- Print the current workspace. Implemented with all/current/custom page selection,
  reordered and imported sources, rotations, blank pages, printable annotations,
  current form values, optional flattened visual marks, bounded 150/300 dpi local
  preparation, progress, cancellation, volatile previews, keyboard access, and the
  system print dialogue. Retained physical-printer and PDF-target output evidence on
  Windows, macOS, and Linux remains a release gate.
- Edit reviewed existing page text and images. Completed for exact, unshared page-level
  `Tj` text runs whose original font round-trips replacement characters, and for
  unambiguous page-level image paint blocks. The graphical workspace supports pointer
  and keyboard selection, image movement, resizing, replacement and removal, text
  replacement, and 100-step undo/redo. Separate inspection and publication jobs bind
  the draft to a full source SHA-256 and verify edited and untouched streams, image
  pixels, forms, bookmarks, annotations, and optional decrypted AES-256 output.
  Complex operators, shared or nested content, arbitrary vectors, and full reflow
  remain read-only.
- Recover recent page-plan sessions. Completed with versioned local autosave,
  manual Save Draft, startup Continue/Discard controls, and interrupted-write fallback.
- Recover standalone Merge and Split planning. Completed for bounded Merge source
  identities, order, paths, and ranges and Split source and page-group text, with
  source-presence checks and no persisted passwords, certificate acknowledgements,
  output protection, undo history, or job requests.

## Milestone 2: Scan and OCR MVP

- Import one or many image files. Completed with native paths and live previews.
- Support common image formats such as PNG, JPEG, TIFF, WebP, BMP, AVIF, HEIC,
  and HEIF where platform tools allow it. Completed with embedded codecs for PNG,
  JPEG, TIFF, WebP, BMP, and GIF, plus an optional ImageMagick fallback.
- Place imported images onto A4, US Letter, business card, ID card, and driving
  licence layouts. Completed with verified, non-overwriting export.
- Clean photographed pages before export. Completed with confidence-gated automatic
  cropping, projective perspective correction, lighting and shadow balancing, applied
  page reports, and a before/after preview that shares the export pipeline through a
  cancellable read-only job with progress, retry, reattachment, final source
  fingerprinting, secret-free lifecycle snapshots, and volatile preview bytes.
- Detect scanned or image-heavy PDFs.
- Select installed OCR language packs, starting with English. Completed through
  bounded Tesseract discovery, a local language selector, and fail-fast validation
  against every selected pack.
- Run OCR locally. The OCRmyPDF adapter is connected with cancellation-safe verified
  publication and bounded engine-level page percentages. A generated public
  English/Turkish/rotated/noisy corpus runs in three-platform CI, and tagged drafts
  require engine-backed Ubuntu, macOS, and pinned native Windows reports. The Windows
  x64 corpus passes locally; the first complete tagged evidence set remains outstanding.
- Run OCR directly on an existing scanned PDF. Completed with a dedicated graphical
  Recognise Text workspace backed by the same verified OCR engine path as batch work:
  installed-language selection, optional deskew, protected input, certificate review,
  progress, cancellation, retry, searchable-page coverage, create-new publication,
  and optional AES-256 output with decrypted coverage checks.
- Preserve original images while adding a text layer. Completed through separate
  temporary image-only and OCR outputs; sources are never rewritten.
- Protect image-only or searchable scan output. Completed with optional QPDF AES-256
  after prepared verification, decrypted page, image, and text-coverage checks, a final
  source-image fingerprint gate, and session-only passwords.
- Review uncertain OCR words. Completed for the selected cleaned page with bounded
  confidence parsing, clickable overlays, side-by-side corrections, and temporary
  recognition hints whose non-guaranteed behaviour is stated in the interface. Review
  uses a distinct shared read-only job with staged progress, ImageMagick and Tesseract
  process-tree cancellation, reattachment, source/settings-free lifecycle snapshots,
  content-free failures, final source-image fingerprinting, and recognised words kept
  only in volatile result state.
- Display scan and OCR job progress. Completed with typed native job snapshots,
  monotonic stage progress, OCRmyPDF engine percentages mapped into the reserved 76–90%
  scan interval, cancellation, automatic status retry, and same-process frontend
  reattachment through the same bounded FIFO manager used by compression, privacy
  cleaning, and batch recipes. Retained failures are content-free and optional protected
  output uses the same cancellable job.
- Capture from connected scanners. The shared contract, Windows WIA and Linux SANE
  adapters, packaged macOS Image Capture bridge, safe capture storage, and
  capability-aware controls are complete. Adapter output and runtime are bounded and
  each operation owns its descendant process tree. Acquisition now uses the shared
  typed scheduler with progress, queued or running cancellation, retry, same-process
  reattachment, one-time interrupted recovery, content-free failures, private queued
  snapshots, final output size and modification-time checks, and sequential scan-review
  loading. The documented cross-platform physical-device corpus remains outstanding.
- Search text after OCR completes. The native exporter now verifies searchable text
  page by page, and opening the result uses the existing PDF.js search workflow.
- Recover ordered image-scan sessions and their paper, colour, DPI, margin, quality,
  deskew, and OCR settings. Completed without storing image bytes or passwords.

## Milestone 3: Protection and Signing MVP

- Add and remove AES-256 PDF password protection through QPDF. Completed through the
  shared native job queue with visible progress, cancellation, and reattachment.
- Support separate opening and administrator passwords. Completed with 127-byte bounds.
- Configure printing, copying, page-assembly, form, annotation, and editing permissions.
  Completed with explicit advisory-permission wording.
- Keep passwords out of logs and process command lines. Completed with standard-input
  argument delivery, secret-free job snapshots, and bounded redacted diagnostics.
- Verify protected output before publishing the destination file. Completed with
  QPDF reopening, start/final source fingerprints, and create-new publication.
- Keep every structural workflow inside an exhaustive encryption contract. Completed
  for all thirty-three shared job kinds, with fourteen optional AES-256 publishers, preserved
  certificate-source encryption, password-aware unencrypted PDF/A output, explicit
  protection management, and a native-registry parity regression.
- Inspect existing signatures. Completed with local structural field discovery and
  bounded pyHanko validation that reports integrity and trust separately. Validation
  now uses the shared read-only scheduler with progress, traversal and process-tree
  cancellation, reattachment, path-free snapshots, content-free failures, and exact
  final PDF and trust-root fingerprints.
- Create named visual signatures and initials. Completed for freehand drawing, typed
  script/classic/modern artwork, and PNG, JPEG, WebP, BMP, or TIFF image intake.
- Remove light image backgrounds locally, crop to detected ink, and choose original,
  black, or blue artwork. Completed.
- Store prepared visual marks locally. Completed with per-entry Argon2id and
  AES-256-GCM authenticated encryption, encrypted kind/method/source metadata,
  backward-compatible legacy unlock, session-only plaintext, and explicit two-step
  deletion. Native commands expose only eight stable path-free outcome codes; acceptance
  covers wrong-passphrase retry and deletion without retaining the passphrase.
- Place and edit transparent visual marks. Completed with page drag/drop and button
  placement, pointer and keyboard movement, proportional resize, arbitrary rotation,
  duplication, deletion, reuse, per-placement locking, bounded undo/redo, and stable
  page identities through reordering.
- Flatten visual marks and optionally restrict document changes during export.
  Completed with deduplicated image embedding, all page rotations, exact per-page
  reopened resource counts, optional QPDF AES-256 passwords, and repeated verification
  after decryption; reader permissions remain advisory.
- Add a signature field. Completed for new visible fields with rotation-aware page
  placement and for named invisible fields through pyHanko.
- Sign with a local certificate. The bounded PKCS#12 adapter, private passfile,
  incremental create-new publication, post-signing verification, and interface are
  complete. Signing now uses the shared cancellable scheduler, snapshots the PDF,
  identity, and trust roots into a private workspace after SHA-256 review, rechecks the
  originals before publication, supports same-process reattachment, and retains
  sanitised terminal state. Password-protected input is supplied through a private
  standard-input bridge and the signed copy must preserve source encryption. A
  generated disposable-identity gate passes on Windows x64; tagged three-platform
  timestamp evidence remains outstanding.
- Support visible and invisible signatures. Completed at adapter and interface level;
  visible, incremental invisible, reopened, and encrypted Windows x64 engine evidence
  now passes; macOS/Linux and timestamp-service evidence remains outstanding.
- Add RFC 3161 HTTPS timestamps, optional PAdES validation information, additional
  trust roots, and multiple-signature validation. Implemented with a generated,
  key-discarding release gate and strict path-free report; tagged releases require the
  gate with timestamping on every target platform.
- Warn before edits invalidate existing signatures. Completed for opened workspace
  PDFs and standalone Merge, Split, Protect, Privacy Cleaner, Compress PDF, and Batch
  Recipes sources through lightweight preflights, gated controls, explicit acknowledgement, and backend
  enforcement before publication.

## Milestone 4: Editing Tools

- Add a local document health preflight. Completed for security actions,
  attachments, encryption, signatures, forms, metadata, page geometry, oversized
  images, likely blank or duplicate pages, and static accessibility signals for
  title, language, tags, structure, Figure alternatives, and tab order. Technical
  integrity checks now also cover bounded object references and nesting, strict page
  and nested Form XObject streams, missing named resources, nested font embedding and
  Unicode maps, output intents, bounded binary ICC header and tag-table validation,
  unmanaged Device CMYK, cycle-safe 32-level/100,000-context resource traversal, and
  explicit finding truncation. The check now uses the shared scheduler for staged
  progress, cancellation inside bounded traversals, reattachment, content-free
  failures, interrupted-state recovery, and a final exact source fingerprint check.
  PDF/A-1b/2b/3b conversion and independent conformance validation now live in the
  separate PDF Standards workflow. The same workflow formally validates PDF/UA-1 and
  PDF/UA-2 through veraPDF and provides explicitly non-certifying bounded structural
  preflight for PDF/X-1a:2001, PDF/X-3:2002, and PDF/X-4. PDF/UA remediation,
  standards conversion beyond PDF/A, independent PDF/X certification, additional
  required-tag semantics, and colourimetric conformance remain separate release work.
- Add a verified privacy-clean export. Completed for selected metadata and
  identifiers, Web Capture provenance, scripts and automatic actions, attachments,
  annotations and form fields, page thumbnails, source-fingerprint enforcement, full
  rewriting, reopening verification, and unreachable-object pruning. A bounded privacy
  inspection now also reports optional and default-hidden layers, invisible text,
  zero-opacity painting, hidden annotations, cropped-away artwork, embedded PDX indexes,
  and declared private-extension containers without auto-deleting ambiguous content.
  Inspection now uses the shared read-only scheduler with bounded staged progress,
  cancellation through object and page traversal, reattachment, source/password-free
  lifecycle snapshots, content-free failures, and a final exact source fingerprint.
  Publication supports optional AES-256 output with decrypted category re-verification
  and a final source-fingerprint gate.
- Add compression preview and verified export. Completed for exact dry-run size
  calculation, representative source-versus-candidate image review, bounded
  DeviceRGB/DeviceGray JPEG recompression, structural stream optimisation, explicit
  preserved-image reporting, create-new publication, reopening checks, content-free
  retained failures, final source revalidation, and optional AES-256 output whose
  decrypted candidate repeats page, form, and bookmark checks. Preview analysis now
  uses the shared read-only scheduler with progress, cancellation, reattachment,
  source/password-free lifecycle snapshots, and a final exact source fingerprint.
- Run long compression, privacy-clean, merge, split, organiser, bookmark, form,
  certificate-signing, permanent-redaction, and password-protection exports through
  shared native jobs. Completed
  with a two-worker FIFO queue, bounded pending and retained state, secret-free public
  snapshots, monotonic image/object progress, cancellation before publication, status
  retry, frontend reattachment within the same application process, and one-time
  interrupted-state restoration after a process restart. Scan/OCR and connected-scanner
  acquisition now use this scheduler too; scan creation calls the generic typed job
  lifecycle directly and has no compatibility IPC layer. Merge, split, organiser
  export, permanent redaction, bookmark and form publication, and password protection
  use typed scheduler variants directly and expose only secret-free public snapshots. Every
  scheduled workflow now offers a fresh-setup retry after failure or cancellation and
  a copyable allow-listed diagnostic that excludes request and result payloads.
- Add reusable local batch recipes. Completed for up to fifty unique PDFs and 20 GiB
  of source data through one cancellable shared source-review job, followed by
  searchable OCR, optional deskew, privacy-clean and
  compression steps, with built-in and settings-only custom recipes, per-source
  passwords, signature acknowledgement, isolated prepare-all-before-publish output,
  cancellation, searchable-page coverage, and itemised results. Protected OCR sources
  are unlocked only inside the workspace, and optional shared AES-256 passwords are
  applied once to each final non-archival candidate before the complete set is
  published. PDF/A-1b/2b/3b conversion now runs as a final, independently validated,
  unencrypted recipe step with a built-in PDF/A-2b recipe. Connected-scanner and local
  image batches can now enter through a password-free session hand-off after verified
  scan-PDF publication, while saved recipes remain settings-only.
- Add PDF/A conversion and validation. Completed for PDF/A-1b, PDF/A-2b, and PDF/A-3b
  with optional OCR, protected-source private unlock, signature-risk acknowledgement,
  shared progress and cancellation, bounded veraPDF JSON reports, source revalidation,
  atomic publication, and a three-platform release-evidence gate. Validation-only
  PDF/UA-1 and PDF/UA-2 now use exact veraPDF flavours. PDF/X-1a:2001, PDF/X-3:2002,
  and PDF/X-4 now have bounded structural preflight reports that never claim ISO
  conformance; their independent corpus and certification boundary remain open.
- Add local visual and textual PDF comparison. Completed for progressive bounded text
  and geometry analysis, added and removed pages, changed-page filtering, independent
  passwords, and a cancellable selected-page side-by-side pixel difference map.
- Add bookmark editing and heading-based navigation generation. Completed for bounded
  existing-tree inspection, branch editing, Unicode titles and styles, changed-source
  rejection, verified create-new export, review-only streamed heading suggestions,
  distinct shared inspection and publication jobs with cancellation and reattachment,
  content-free retained failures, optional linked A4 contents pages with physical page
  numbers and an embedded Unicode font, exact generated-link reopening checks, and
  optional verified AES-256 output protection.
- Add text, image, stamp, highlight, and shape annotations. Completed with a bounded
  graphical workspace for text boxes, highlights, stamps, freehand ink, rectangles,
  ellipses, lines, and images; standard PDF objects and generated appearances;
  rotated-page mapping; local history; source and signature safety gates; and exact
  create-new reopening verification. Publication has shared-job progress, cancellation,
  same-process reattachment, content-free retained errors, final source validation, and
  optional verified AES-256 output. Source inspection now uses a separate cancellable,
  fingerprinted shared job with secret-free lifecycle snapshots. Self-contained existing
  FreeText, single-quad highlight, stamp, single-stroke ink, square, circle, and plain
  line annotations enter the same undoable graphical history. Export applies explicit
  add, update, and remove sets, rejects stale identities, preserves unsupported items,
  and verifies exact per-page counts plus replacement markers. Direct-object, linked,
  rich, multi-part, structurally complex, and over-limit source annotations remain
  visible and read-only.
- Fill and flatten forms. Completed for bounded ordinary AcroForms with hierarchical
  field inspection, typed text, checkbox, radio, and choice editing, generated
  appearances, exact Unicode values, optional safe flattening, XFA rejection,
  source and certificate guards, distinct shared inspection and publication jobs with
  cancellation and reattachment, content-free retained errors, optional AES-256 output,
  and verified create-new publication.
- Rotate, crop, and resize pages. Completed with organiser rotation plus visual-edge
  cropping and proportional paper fitting that preserve page rotation, transform standard
  annotations and form widgets, and verify selected page boxes after reopening. Page
  Finish inspection and publication use distinct shared jobs with progress,
  cancellation, same-process reattachment, content-free retained errors, final source
  validation, and optional verified AES-256 publication output.
- Add watermarks, headers, footers, and Bates numbering. Completed with rotation-aware
  above/below-content layers, page and filename tokens, selected-range sequences,
  graphical previews, source and signature guards, and verified create-new publication.
- Implement real redaction with content removal. Completed with reviewed manual and
  search-assisted regions, lossless raster replacement of selected pages, global
  privacy-structure stripping, source and certificate guards, create-new publication,
  unreachable-object pruning, image-only reopening verification, distinct shared
  inspection and publication jobs, cancellation, same-process reattachment, final
  source-fingerprint gates, and optional AES-256 output whose decrypted candidate
  repeats every redaction and privacy check.

## Milestone 5: Open-Source Release

- Pick final licence. Completed with AGPL-3.0-or-later.
- Add the four-locale interface contract. Completed for the typed British English,
  American English, Turkish, and German runtime, explicit persisted selector, workflow
  navigation, shared open/drop/loading/recovery/edit-safety shell, translated reader
  canvases and locale-aware document search, complete page
  organiser, Merge, complete visual-signature workflow, searchable OCR, OCR confidence
  review, image-scan processing, connected-scanner controls, Print, Split, password
  protection, the shared protected-PDF opening dialogue, Compression, local Activity,
  signed Updates, Document Health, Privacy
  Cleaner, PDF Comparison, Page Finish, Annotation, Forms, Page Content, and Permanent
  Redaction, PDF Standards, Batch Recipes, Bookmarks with heading generation and
  printed contents, and Certificate Signatures. British English is the canonical default
  and fallback; organiser/OCR/scan/scanner/split/protection/compression/health/privacy/
  finish/annotation/form/content/redaction/archive/batch/bookmark/certificate native
  status codes and path-free PDF-opening outcomes are stable;
  and native acceptance switches Turkish and German through organiser, Merge, OCR,
  scan, Split, Protect, Compression, Health, Privacy,
  Comparison, Page Finish, Annotation, Forms, Page Content, Permanent Redaction, PDF
  Standards, Batch Recipes, Bookmarks, Certificate Signatures, Activity, and Update
  screens while checking accessibility names, live page-canvas names, and number formatting. It rejects and
  accepts the AES-256 fixture in Turkish, then reopens and cancels its German password
  dialogue.
- Complete localisation of every remaining native stage and error,
  installer, accessibility label, and user guide, then retain packaged and human
  linguistic review for all four locales. Outstanding; see `LOCALISATION.md`.
- Add CI for Windows, macOS, and Linux. Completed.
- Add an Apple mobile foundation. Completed for the shared Tauri mobile entry point,
  iOS/iPadOS 16 configuration, iPhone/iPad bundle families and orientations, safe areas,
  dynamic viewport sizing, touch targets, indirect input, runtime capability gating,
  backend rejection of desktop-only jobs, App Store-managed update presentation, and a
  credential-free macOS 15 arm64-simulator compile and bundle-evidence workflow. Hosted
  run `30766986118` passed and retained its bundle evidence. Simulator and
  physical-device UI automation, Files picker and document hand-off acceptance, camera
  scanning, native mobile OCR, signed
  device archive, TestFlight, privacy declarations, accessibility review, and App Store
  review remain outstanding.
- Add installer builds. Draft Windows, Linux, and universal macOS packaging is present.
  Native package structure, identity, architecture, hashes, exact publisher identity,
  timestamp, Gatekeeper, and notarisation state now have strict path-free evidence;
  Linux also has clean Ubuntu 22.04, Debian 13, and Fedora 43 extraction or installation
  gates. The first real credential-backed tagged evidence set, Windows reputation
  review, and representative user-machine installation remain release gates.
- Add contributor guide and issue templates. Completed.
- Publish checksums, an exact manifest, SBOMs, and dependency-licence evidence.
  Completed for draft release artefacts.
- Gate PDF.js rendering on generated encrypted, signature-structure, image-only,
  multilingual, unusual-size, 320-page, and malformed fixtures across Windows, macOS,
  and Linux. Completed with retained path-free per-platform reports.
- Gate PDF/A conversion and reusable archival recipes on the public image-only OCR
  fixture across Windows, macOS, and Linux. Implemented with hash-pinned veraPDF and
  Windows Ghostscript acquisition plus path-free per-platform reports; the first tagged
  evidence set remains required.
- Gate certificate signing and validation with a generated disposable identity,
  visible and incremental signatures, AES-256 input, save-and-reopen checks, configured
  trust, and intact-but-untrusted separation. Implemented with a closed path-free report
  and a passing Windows x64 run; the first tagged timestamp-enabled Windows, macOS, and
  Linux evidence set remains required.
- Publish signed releases. Protected Windows certificate import and macOS Developer ID
  plus App Store Connect notarisation are implemented with exact signer, timestamp,
  Gatekeeper, stapled-ticket, cleanup, and path-free evidence gates. The first real
  certificate-backed packages, Windows reputation review, and representative
  installations remain outstanding.
- Add signed application updates. Implemented as a user-triggered, fail-closed Tauri
  updater with explicit alpha/beta/stable derivation, protected credential-backed
  builds, strict immutable-manifest evidence, separately approved byte-verified channel
  promotion, and public rollback and key-recovery procedures. The first real signed
  three-platform promotion and rollback rehearsal remains a release gate. iOS excludes
  the desktop updater and uses App Store-managed updates.
- Publish an honest capability matrix. Completed with complete, experimental,
  unavailable, dependency, security-boundary, and release-gate sections in
  [FEATURE_STATUS.md](FEATURE_STATUS.md).
- Establish the application accessibility baseline. Completed for skip navigation,
  visible keyboard focus, a roving workflow chooser, and shared focus containment,
  Escape, and opener-focus return across all modal workspaces, with automated contracts
  and a public [manual test matrix](ACCESSIBILITY_TESTING.md). Packaged keyboard-only and
  assistive-technology evidence remains a release gate.

The detailed release gates and differentiators are tracked in
[RELEASE_PLAN.md](RELEASE_PLAN.md).
