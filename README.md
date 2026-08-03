<div align="center">
  <img src="src-tauri/icons/128x128.png" alt="Tüfekci Paperworks icon" width="96" height="96">
  <h1>Tüfekci Paperworks</h1>
  <p>A local-first, cross-platform PDF, scanning, OCR, and signing workbench.</p>
</div>

> [!IMPORTANT]
> Tüfekci Paperworks is currently pre-alpha. Real local PDF rendering and search,
> non-destructive page organisation and verified export, paper-sized image-to-PDF
> scanning, draw/image/type visual signatures and initials, an encrypted local
> visual-mark library with typed path-free native outcomes,
> QPDF-backed password protection, and the
> workflow interface are implemented. A bounded local Print workflow now prepares the
> live organiser order at 150 or 300 dpi, carries printable annotations, current form
> values and optional visual marks, and opens the operating system print dialogue;
> retained physical-printer and PDF-target evidence on all three platforms remains a
> release gate. Searchable OCR now performs strict local
> engine and language-pack preflight, page-level text verification, and selected-page
> confidence review on systems with OCRmyPDF and Tesseract. The dedicated Recognise
> Text workspace also creates a cancellable, verified searchable copy of an existing
> PDF with optional deskew and AES-256 output. A generated public
> English, Turkish, rotated, and noisy corpus now runs in all three CI environments,
> while tagged drafts require engine-backed Ubuntu, macOS, and pinned native Windows
> evidence. Named visual marks can be dragged onto any page, moved, proportionally
> resized, rotated, duplicated, deleted, locked, reused, undone, and redone. Export
> embeds each image once, flattens every placement into ordinary page content, reopens
> the result, and verifies the exact per-page mark count. Ordered PDF merging,
> selected-range import, and range-based splitting are implemented. A verified
> privacy-clean export is also implemented
> for selected metadata, scripts, attachments, annotations, form fields, and page
> thumbnails. Local autosaved recovery drafts now cover PDF page plans, image
> scan sessions, and standalone Merge and Split plans without storing passwords or
> signature images. The bounded pyHanko
> certificate-signing and validation adapter and its four-locale responsive interface
> are implemented. A generated disposable-identity gate now passes visible,
> incremental, encrypted-input, save-and-reopen, trusted, and intact-but-untrusted
> validation on Windows x64 without retaining key material. Tagged releases still
> require the same path-free evidence on Windows, macOS, and Linux with RFC 3161
> timestamping enabled. Page import into the active organiser is implemented
> with a cancellable fingerprinted source review through the shared scheduler, and
> scan/OCR export runs as a cancellable native job with visible stage progress and
> engine-level OCR page percentages, content-free retained failures, and a final
> source-image fingerprint gate. Optional
> QPDF AES-256 output is applied only after the image or OCR candidate passes; the
> protected copy is decrypted and put through the same page, image, and searchable-text
> coverage checks before publication.
> Standalone merge and split also run through the bounded native job queue, with
> progress, cancellation before publication, retrying status checks, and same-process
> reattachment. Merge now has translated typed live stages, action failures and bounded
> warning templates, content-free retained failures, final source-fingerprint
> checks, and optional AES-256 output whose decrypted candidate repeats the page-tree
> verification. Split applies the same optional AES-256 passwords to every prepared
> part, decrypts and verifies the complete protected set, and rechecks its source before
> publishing any part. Both standalone planners have bounded 100-step undo and redo:
> Merge covers source additions, removals, ordering, and page ranges, while Split covers
> page-group edits. Merge history deliberately strips source passwords.
> App-owned temporary PDFs, batch workspaces, certificate input workspaces and
> passfiles, pyHanko password bridges, OCR hint files, and scan-normalisation rasters
> now use a private lock-backed
> lease registry. A new
> application process removes only unlocked, strictly validated stale artefacts left by
> a crash; live work is skipped and recoverable scanner captures keep their separate
> seven-day retention policy.
> Long-running shared jobs now have a separate private, lock-backed active-job
> journal. After a crash or application restart, unlocked entries are restored once as
> explicit interrupted terminal states. The journal stores only an opaque entry ID,
> workflow kind, and start time: requests, paths, filenames, passwords, OCR hints,
> signature data, document content, progress, errors, and results are never persisted
> or replayed. Publication workflows still ask the user to check the chosen destination
> before a fresh retry; read-only checks simply run again from the current source.
> The desktop Activity panel now keeps a bounded local audit of terminal shared
> jobs. It records only an opaque entry ID, workflow, succeeded/failed/cancelled
> outcome, and timing; filenames, paths, passwords, document content, error text,
> stages, and typed job results are excluded. The panel supports outcome filtering,
> refresh, create-new JSON export, and confirmed clearing.
> The main organiser export now uses that queue too and binds its page
> plan to the opening size and modification time of every primary and imported PDF,
> checking those fingerprints again immediately before publication.
> Permanent-redaction review and publication use distinct shared jobs, with native
> progress, cancellation, same-process reattachment, content-free retained failures,
> and exact source checks before report delivery and create-new publication.
> AES-256 password addition and removal now use that queue as well, with cancellable
> QPDF execution, bounded diagnostics, and the same start/final source-fingerprint gate.
> Connected capture is implemented for Windows WIA, macOS Image Capture, and Linux
> SANE. Acquisition now runs through the shared typed scheduler with staged progress,
> queued or running cancellation, process-tree termination, safe retry, same-process
> reattachment, content-free retained failures, and final captured-page size and
> modification-time checks. Device identifiers and settings are excluded from queued and interrupted
> snapshots; successful page paths remain volatile and are opened sequentially for scan
> review. Local two-document text, geometry, and selected-page visual comparison is
> implemented. Verified bookmark-tree editing, review-only heading-based navigation
> generation, and optional linked printed contents pages are implemented. Bookmark review and publication now use distinct shared
> jobs with cancellation, reattachment, final source checks, content-free retained
> failures, and optional verified AES-256 publication protection. A bounded graphical annotation workspace now exports
> editable text boxes, highlights, stamps, freehand ink, rectangles, ellipses, lines,
> and images with generated appearances and verified create-new output. Annotation
> publication now runs through the shared queue with cancellation, reattachment,
> content-free retained errors, start/final source checks, and optional verified
> AES-256 output protection. A bounded
> AcroForm workspace now fills text, checkbox, radio, and choice fields, generates
> explicit appearances, and can flatten supported fields into verified static page
> content. Form review and publication now use distinct shared jobs with cancellation,
> reattachment, content-free retained errors, final source checks, and optional verified
> AES-256 publication protection. A graphical page-finishing workspace now crops visible page boxes, fits
> selected pages to paper, and adds watermarks, headers, footers, and Bates numbering
> through verified create-new output. Page Finish review and publication now use
> distinct shared jobs with cancellation, reattachment, content-free retained errors,
> final source checks, and optional verified AES-256 publication protection. A
> graphical permanent-redaction workspace now
> supports reviewed manual regions plus bounded text, email-address, and wildcard
> suggestions. Marked pages are replaced with lossless image-only pages, private and
> interactive document structures are stripped, and the create-new output is reopened
> and verified before publication. Redaction review and publication now use distinct
> shared jobs with cancellation, reattachment, content-free retained errors, and final
> source checks. Reusable local batch recipes now inspect and fingerprint up to fifty
> PDFs through one cancellable, recoverable, request-free shared review job, then apply
> reviewed privacy-clean and/or compression
> settings through the shared cancellable queue, and prepare the complete output set
> before publication. Batch outputs can share optional AES-256 opening and administrator
> passwords; every protected workspace copy repeats its delegated privacy or compression
> verification before the final source checks and all-or-nothing publication. Compression
> publication now retains only content-free job
> failures, rechecks the source immediately before publication, and can apply optional
> AES-256 output protection after the smaller prepared copy passes. The protected
> candidate is decrypted and put through the same page, form, and bookmark checks.
> The application shell now has an automated keyboard baseline: a skip link, visible
> focus, a roving workflow chooser, and shared focus containment and return across all
> modal workspaces. The packaged keyboard-only and assistive-technology matrix remains
> a release gate. A typed local interface-locale runtime now provides explicit
> `en-GB`, `en-US`, `tr-TR`, and `de-DE` catalogues, with British English as the
> default and fallback. The persisted selector, workflow rail, Merge workspace,
> shared document opening, drag-and-drop, loading, recovery and edit-safety shell,
> complete page organiser, shared protection and job controls, complete visual-signature
> workflow, full-page and thumbnail reader states, locale-aware progressive document
> search, standalone searchable OCR, OCR confidence review, image-scan processing,
> connected-scanner controls, complete Print, Split, password-protection, Compression,
> local Activity, signed Updates, Document Health, Privacy Cleaner, PDF Comparison, and
> Page Finish, Annotation, Forms, Page Content, Permanent Redaction, PDF Standards,
> Batch Recipes, Bookmarks, Certificate Signatures, and the shared protected-PDF
> opening dialogue are migrated. Stable organiser, OCR,
> scan, scanner, split, protection, compression, health, privacy, page-finishing,
> annotation, form, content-editing, redaction, archive, batch, batch-inspection,
> bookmarks, bookmark-inspection, certificate, and certificate-validation
> stage codes, mapped organiser/split/compression/privacy/page-finishing/annotation/
> form/content-editing/redaction/archive/batch warnings and findings, and generic
> content-free failures keep native detail out of translated application messages.
> PDF opening now reduces cancellation, changed-file, malformed-file, password, and
> read failures to stable path-free outcomes before they reach the interface. PDF text
> extraction likewise collapses to one path-free outcome and evicts failed cache entries
> so a later search can retry.
> Native GUI evidence switches Turkish and German through Merge, OCR, scan, page
> organisation, Split, Protect, Compression, Health, Privacy, Comparison, and Page
> Finish, Annotation, Forms, Page Content, Permanent Redaction, PDF Standards, Batch
> Recipes, Bookmarks, and Certificate Signatures, opens translated Activity and signed-Update dialogs, checks translated
> editor, thumbnail, and live page-canvas accessibility names, and proves locale-aware document-size formatting after a live
> switch. It also rejects and then accepts a password for a generated AES-256 PDF in
> Turkish, reopens its German prompt, and cancels without exposing the secret or a
> native diagnostic. Remaining native stages and errors, installers, accessibility
> names, release metadata, and user guides are not yet fully localised; see
> [localisation](docs/LOCALISATION.md), and do not describe
> any locale as release-complete yet. Official release builds now have an explicit, user-triggered signed
> updater with alpha, beta, and stable channels; the first credential-backed
> three-platform update, promotion, and rollback evidence remains a release gate.
> Tagged packages now fail closed unless Windows installers carry the configured
> Authenticode identity and trusted timestamp, while macOS builds carry the configured
> Developer ID identity, secure timestamp, successful Gatekeeper assessment, and
> stapled notarisation ticket. The first real certificate-backed package, reputation,
> and representative installation evidence remains a release gate. The
> documented physical-scanner matrix still needs to pass on release hardware.

> [!NOTE]
> Read the [feature status and security boundaries](docs/FEATURE_STATUS.md) before
> installing, packaging, or describing a release. It distinguishes complete,
> experimental, and unavailable workflows and lists every gate that still prevents a
> stable release.

## Platform Support

| Platform | Current status | Capability boundary |
| --- | --- | --- |
| Windows 10/11 | Desktop alpha | Full local Rust core plus optional QPDF, OCR, PDF/A, certificate-signing, and WIA scanner engines |
| macOS 12+ | Desktop alpha | Full local Rust core plus optional command-line engines and the packaged Image Capture scanner bridge |
| Linux | Desktop alpha | Full local Rust core plus optional command-line engines and SANE connected scanning |
| iPhone and iPad, iOS/iPadOS 16+ | Experimental build foundation | Local viewing, page organisation, pure-Rust editing and export, image-to-PDF, annotations, forms, page finishing, redaction, visual signatures, and the encrypted visual-mark vault; desktop subprocess engines, connected scanners, camera capture, certificate signing, PDF/A conversion, document password changes, and direct application updates are unavailable |

Apple mobile builds require macOS and Xcode. The repository includes an unsigned
arm64 simulator compile-and-bundle gate for both iPhone and iPad families; a signed
device archive, TestFlight run, App Store review, and retained mobile interaction and
accessibility evidence remain open release gates. iOS updates are managed by the App
Store rather than the desktop Tauri updater.

## Principles

- Process documents locally by default.
- Never overwrite a source document in place.
- Use proven PDF, OCR, and signing engines instead of inventing fragile parsers.
- Distinguish visual signatures and permission locks from certificate-backed,
  cryptographically verifiable signatures.
- Keep common document workflows understandable before exposing advanced options.

## Current Interface

- PDF workflow workspace for organising, merging, splitting, OCR, signing, printing,
  and protection.
- A connected Recognise Text workspace for existing scanned PDFs, with local engine
  readiness, installed-language selection, optional deskew, encrypted-input support,
  certificate review, cancellation, searchable-page verification, and optional
  AES-256 output.
- PDF.js page rendering with real page counts, lazy thumbnails, high-DPI zoom,
  searchable document text, local password prompts, and offline rendering assets.
- Local all/current/custom-range printing from the current organiser plan, including
  reordered, rotated, duplicated, imported and blank pages, printable annotations,
  current form values, and optional flattened visual signatures or initials. Preparation
  is cancellable and bounded to 100 pages, 50 megapixels per page, and 120 megapixels per
  request before the operating system dialogue handles printer-specific settings.
- Bounded desktop PDF loading through a 64 KiB initial sample and cancellable local
  range requests, with source size and modification checks while a file is open.
  Browser file intake remains memory-backed, and non-optimised PDFs may still require
  PDF.js to request every range.
- Display-only PDF annotation and AcroForm appearance layers with local icon assets.
  Embedded scripts, external links, and rendered form controls stay inert in the main
  preview; editing occurs only after the user opens the reviewed form workspace.
- Graphical annotation workspace with drag-to-create and drag-to-move tools for text
  boxes, highlights, stamps, freehand ink, rectangles, ellipses, lines, and embedded
  images. It includes page navigation, selection, duplication, deletion, colour and
  opacity controls, shape fills, line and font sizing, and a 100-step undo/redo history.
  Native source inspection is a distinct cancellable shared job with page progress,
  safe retry, interrupted-state recovery, content-free failures, and a final source
  fingerprint before the workspace receives its report.
- Annotation export writes standard `FreeText`, `Highlight`, `Stamp`, `Ink`, `Square`,
  `Circle`, and `Line` objects with generated appearance streams, rotation-aware page
  coordinates, bounded image and stroke data, source-fingerprint checks, signed-source
  acknowledgement, form preservation, shared-job progress and cancellation, and exact
  marker verification after reopening. Optional QPDF AES-256 output is decrypted and
  checked again for page count, form preservation, annotation counts, generated markers,
  subtypes, appearance streams, and embedded-image resources.
  Self-contained existing `FreeText`, single-quad `Highlight`, `Stamp`, single-stroke
  `Ink`, `Square`, `Circle`, and plain `Line` annotations enter the same graphical
  history and can be moved, restyled, duplicated, deleted, undone, and redone. Stable
  source identities separate additions, updates, and removals; export rejects stale or
  invented identities, preserves unsupported annotations, and verifies exact per-page
  counts plus every replacement marker. Direct-object, linked, rich, multi-part,
  structurally complex, and over-limit annotations remain visible and read-only. Full
  Unicode text remains in annotation contents; characters outside the built-in Windows
  Latin appearance font are visibly substituted and reported.
- Graphical AcroForm workspace with a searchable field list, page-linked widget overlay,
  typed editors for text, password, checkbox, radio, fixed-choice, editable-choice, and
  multi-select fields, required-field feedback, filtering, reset controls, and a bounded
  100-step undo/redo history. Push buttons, signature fields, read-only fields, and
  unsupported field types are preserved but are not edited.
- Form export stores exact Unicode values, generates verified appearances, fingerprints
  the reviewed source at worker start and immediately before publication, requires
  certificate-signature acknowledgement, and always
  publishes a reopened create-new copy. Optional flattening turns supported fields with
  complete page geometry into static content while preserving signatures, push buttons,
  unsupported fields, and incomplete widgets. XFA forms are detected and rejected;
  optional QPDF AES-256 output is decrypted and checked again using stable field names,
  exact values, field counts, appearances, and flattened markers.
- Graphical Page Finish workspace with all/current/range selection, visual-edge crop
  controls, A3/A4/A5, US Letter, US Legal, and custom paper sizing, portrait or landscape
  fit, live source placement, and output-dimension preview. Rotation-aware transforms
  keep page content upright and carry standard annotation and form-widget coordinates.
- Local watermarks above or below content, token-aware headers and footers using
  `{page}`, `{pages}`, and `{file}`, and sequential Bates labels with configurable
  prefix, suffix, start, padding, position, size, margin, colour, and selected-page scope.
  Export fingerprints the reviewed source, requires signed-source acknowledgement,
  preserves forms, bookmarks, and annotation counts, and verifies page boxes, operation
  markers, and every generated mark layer after reopening. Publication offers shared-job
  progress and cancellation; optional QPDF AES-256 output is decrypted and checked
  again before publication. Cropping is explicitly non-redactive.
- Reversible single- or multi-page reordering, rotation, deletion, duplication, and
  blank-page insertion with A4, US Letter, business card, ID card, and driving licence
  sizes. Explicit selection toggles, modifier ranges, Select All, active-page focus, and
  keyboard alternatives feed one stable page-ID history operation; dragging a selected
  thumbnail moves the complete ordered group.
- Selected-range import from another PDF into the active thumbnail plan, with a
  cancellable fingerprinted source review, progress, safe retry, imported-page previews,
  source-aware text search, single- or multi-page drag-and-drop ordering, rotation,
  duplication, deletion,
  undo, redo, passwords, and certificate-risk review.
- Two-document page transfer keeps the selected source pages and reviewed destination
  visible together. Drag a single page or ordered selection to an exact destination
  boundary, or use the numeric insertion control for very large PDFs. Copy publishes a
  verified create-new destination without changing the source plan; Move removes the
  pages only after destination reopening succeeds and records that removal as one
  undoable source operation. Rotations, blanks, imported sources and visual marks are
  preserved, with password, certificate-risk and optional AES-256 output handling.
- Native-reviewed page-content editing for exact existing text and image objects.
  The graphical workspace selects reviewed objects on the rendered page, replaces text
  only when the original embedded font can encode it exactly, and can move, resize,
  replace, or remove unambiguous page images. Pointer and keyboard controls share a
  bounded 100-step undo/redo history. Complex operators and nested or shared content
  remain read-only.
- Page-content publication binds edits to a full SHA-256 source fingerprint, never
  overwrites the original, preserves unrelated structures, and reopens the candidate to
  verify edited stream markers, untouched stream digests, replacement-image pixels,
  page count, forms, bookmarks, and annotations. Optional AES-256 output is decrypted
  and put through the same checks before create-new publication.
- Ordered multi-PDF merge and selected-page import with per-source ranges and
  passwords, reverse ranges, odd/even selection, repeated pages, drag or button source
  ordering, verified output, and cancellable source/page progress through the shared
  native queue. An enabled-by-default control preserves resolved source bookmarks whose
  destinations survive the selected ranges, promotes retained descendants, maps
  repeated-page links to the first copy, and reports omitted entries. Merge rechecks
  every opening source fingerprint immediately before publication and can apply
  optional QPDF AES-256 protection only after the prepared page and bookmark trees
  pass; the protected copy is decrypted and checked again. Live stages, failures, and
  warnings use four-locale typed outcomes and never display raw native exception text.
  AcroForm catalogues are
  reported but not merged. A bounded 100-step history covers source additions,
  removals, ordering, and page-range edits without retaining source passwords.
- Multi-part split and single-range extraction using semicolon-separated page groups.
  Every part is prepared and verified before publication, with clean-up if the set
  cannot be completed, visible per-part progress, final source-fingerprint validation,
  content-free retained failures, and cancellation before any part is published.
  Optional QPDF AES-256 protection is prepared and decrypted-verification checked for
  every part before the all-or-nothing publication stage. Page-group editing has its
  own bounded 100-step undo/redo history, reset whenever a new source is selected.
- Local document health checks for encryption, certificate signatures, forms, XFA,
  JavaScript, automatic and launch actions, attachments, metadata, bookmarks,
  unusual page geometry, oversized images, dangling object references, strict page-stream
  and nested Form XObject parsing, missing named resources, nested font embedding and
  Unicode maps, output intents, bounded binary ICC header/tag validation, unmanaged
  Device CMYK, cycle detection, and disclosed inspection limits. Health inspection uses
  the shared native queue with staged progress, cancellation throughout bounded
  traversals, same-process reattachment, one-time interrupted-state recovery,
  content-free retained failures, and an exact size/modification-time recheck before
  returning the report.
- Accessibility preflight for document title display, default language, tagged-PDF
  declarations, structure elements, page structure links, Figure alternative text,
  and interactive tab-order signals, with explicit manual reading-order guidance.
- Lightweight edit-safety preflights for the active workspace and standalone Merge,
  Split, Protect, Privacy Cleaner, Compress PDF, and Batch Recipes sources. One
  debounced aggregate read-only job checks up to 250 sources with staged progress,
  cancellation, retry, stale-selection rejection, path-free ordered results,
  content-free per-source failures, and a final size/modification-time fingerprint.
  Rewriting actions wait for the check, and certificate-signed inputs require explicit
  acknowledgement enforced again by the Rust command before any output is published.
- Review-only detection of likely blank and duplicate pages; findings never alter
  the source document automatically.
- Fingerprinted local privacy inspection for ordinary metadata, scripts, attachments,
  forms, thumbnails, optional-content groups and default-hidden layer use, invisible
  text rendering, zero-opacity drawing, hidden annotations, cropped-away artwork,
  Web Capture provenance, embedded PDX indexes, and declared private extensions.
  Findings link to safe cleaner switches, while content-dependent signals remain
  review-only and explicitly disclose their page-level inspection limits.
- Local privacy-clean export with explicit switches for document information, XMP,
  file identifiers, Web Capture URL/digital-ID trees, private application history,
  JavaScript, automatic and launch actions, attachments, annotations, form fields,
  and embedded page thumbnails. Cleaning requires the inspected source size and
  modification time; the prepared copy is reopened and checked before publication,
  and the original is never changed. Optional AES-256 protection is applied only after
  the selected removal categories pass, then the protected candidate is decrypted and
  checked again before a final source-fingerprint gate and publication.
- Graphical permanent redaction with manual draw-and-move regions, black or white
  fills, page navigation, 100-step undo/redo, selectable text and name matching,
  email-address discovery, and bounded `*`, `?`, and `#` wildcard patterns. Search
  matches remain review-only suggestions until selected explicitly.
- Redaction export renders only marked pages to clean bounded lossless PNG artwork and
  sends separately reviewed normalised geometry to native code. Rust validates every
  region, expands it by one raster pixel, burns the black or white masks into the decoded
  raster in reviewed order, and rebuilds each selected PDF page with one verified image
  and no searchable text or interactive resources. It fingerprints the
  reviewed source, checks that fingerprint when native work starts and again after
  output verification, supports progress, cancellation, and same-process reattachment,
  requires certificate-signature acknowledgement, never overwrites,
  strips metadata, actions, attachments, annotations, forms, bookmarks, named
  destinations, thumbnails, and tagged-document structures throughout the new copy,
  prunes unreachable objects, and verifies all page, image, marker, privacy, and
  searchable-text conditions after reopening, including the SHA-256 digest of the exact
  native-masked RGB pixels. Optional AES-256 output protection is
  applied only after that prepared copy passes; the protected candidate is decrypted
  and put through the same redaction and privacy checks before publication. Unmarked
  page artwork is preserved.
- Local compression preview with a 40-95 compatible-image quality control, decoded
  source-versus-candidate sample, exact dry-run rewrite size, saving estimate, and
  explicit counts for reduced and preserved images. Preview calculation uses the
  shared cancellable read-only queue, supports reattachment and safe retry, excludes
  its source path and password from public lifecycle records, and rechecks the source
  immediately before returning its volatile sample report. Export uses the same bounded
  transformation, preserves text, vectors, links, forms, page geometry, and OCR text
  layers, then reopens the smaller create-new copy before publication. Optional QPDF
  AES-256 protection is applied only after this prepared check; the protected candidate
  must decrypt and repeat the page-count, form, and bookmark checks, remain smaller
  than the source, and pass a final source-fingerprint gate before publication.
- PDF Standards with standalone conversion to PDF/A-1b, PDF/A-2b, or PDF/A-3b and
  validation-only reporting through the local veraPDF CLI. Conversion uses OCRmyPDF
  and Ghostscript, can add a searchable English or other installed Tesseract layer,
  reopens the unencrypted candidate, checks page count and searchable-page coverage,
  requires an independent matching-profile conformance verdict, rechecks the source,
  and only then publishes a create-new copy. Reports retain bounded generic failed-rule
  summaries rather than document content or local paths. Protected originals are
  validated through a private unlocked copy but are still reported non-conforming
  because PDF/A forbids encryption; conversion also warns that certificate signatures
  cannot survive the structural rewrite.
  The same shared job formally validates PDF/UA-1 or PDF/UA-2 through veraPDF's exact
  `ua1` or `ua2` flavour. It also provides clearly labelled built-in structural
  preflight for PDF/X-1a:2001, PDF/X-3:2002, and PDF/X-4, covering declarations,
  trapping, encryption, output intents, bounded ICC integrity, embedded fonts, object
  integrity, page boxes, scripts, forms, attachments, external content, non-printing
  media, and transfer curves. PDF/X preflight never claims ISO certification,
  colourimetric proofing, or print-service approval.
- Reusable Batch Recipes for up to fifty inspected PDFs and 20 GiB of source data,
  with per-source passwords, certificate-signature review, built-in smaller/private/
  privacy-clean/searchable-archive recipes, custom settings-only local recipes,
  collision-free filenames, sequential preparation, progress, cancellation, and
  itemised results. Searchable recipes use a selected installed Tesseract language,
  optionally deskew scanned pages, preserve page count, and report pages without a
  verified text layer. OCR runs before privacy cleaning and compression. Protected OCR
  sources are unlocked only inside the isolated workspace when QPDF is available, and
  optional shared AES-256 output protection is applied once to each final candidate
  unless a PDF/A profile is selected. A built-in PDF/A-2b archive recipe and custom
  PDF/A-1b, PDF/A-2b, or PDF/A-3b settings run after OCR, privacy cleaning, and
  compression; each archival candidate must pass veraPDF before publication. PDF/A
  recipes cannot be combined with encryption.
  Protected non-archival candidates are decrypted in memory to repeat page-count and
  searchable text-layer checks before publication.
  Documents, passwords, findings, and output folders are never persisted in saved
  recipes. A successful reviewed scan PDF, including one captured through WIA, Image
  Capture, or SANE, can be handed directly to Batch Recipes from its completion banner.
  The source row records only a session-only origin label and verified path; protected
  scans require their opening password to be entered again.
- Local two-document comparison using the bounded 64 KiB PDF range loader, with
  progressive page-by-page selectable-text and geometry analysis, added and removed
  page detection, changed-page filtering, bounded first-difference excerpts, and a
  cancellable selected-page pixel map. Visual comparison is limited to two million
  pixels and its page rasters, extracted text, and passwords are never persisted.
- Bookmark-tree inspection and editing with Unicode titles, page destinations, up to
  seven hierarchy levels, branch-aware move/indent/delete controls, bold and italic
  styling, text colour, expansion state, and a selected-page preview. Export rejects a
  source changed after review or during processing, requires signed-source
  acknowledgement, and reopens the create-new copy to verify the complete bookmark tree,
  page count, and preserved form structure. The shared job supports progress,
  cancellation, same-process reattachment, and optional QPDF AES-256 output passwords.
- Optional linked A4 contents pages generated from selected bookmark levels, with a
  bounded live preview, physical output page numbers, an embedded Liberation Sans
  Unicode font, clickable page destinations, optional top-level sidebar bookmark, and
  reopening checks for every page marker, link, destination, text stream, and embedded
  font structure. Source pages and edited bookmark destinations move forward together.
  Generated contents pages are not tagged and therefore do not establish PDF/UA
  conformance.
- Review-only heading-based navigation generation using bounded streamed PDF.js text,
  font-size tiers, numbered-heading depth, repeated-header filtering, confidence
  scores, and explicit suggestion selection. It creates a draft PDF bookmark outline;
  printed contents are generated only when the separate option is explicitly enabled.
- Versioned local recovery drafts for PDF page plans and ordered image-scan sessions,
  including workflow, selected page, zoom, and scan settings. The app rotates three
  create-new snapshots, falls back from an incomplete newest write, and offers
  Continue or Discard on the next launch.
- Standalone Merge source order and page ranges and Split source and page-group text
  use the same debounced recovery store. Recovery checks that each source PDF is still
  present before restoring the plan; passwords, certificate acknowledgements, output
  protection, undo history, and complete job requests are not restored.
- Unencrypted imported PDF sources and their source-aware page identities are included
  in recovery drafts. Password-protected imported sources deliberately disable draft
  saving because their passwords are never persisted.
- Recovery drafts contain local source paths and document names, but never passwords,
  prepared signature images, document text, or PDF/image bytes. Closing a document
  from the workspace clears its recovery snapshots.
- Self-contained Rust PDF export with native destination dialogues, source and
  destination overwrite protection, temporary output, storage flushing, structural
  verification, and user-facing warnings for signatures, forms, and bookmarks.
- Crash-safe temporary-workspace leases for app-owned PDF candidates, batch folders,
  certificate passfiles, pyHanko password bridges, OCR recognition hints, and scan
  rasters. Start-up clean-up is
  bounded, rejects links and unexpected names or file types, skips locks held by live
  operations, and reports aggregate counts without exposing document paths.
- Secret-free process-restart recovery for all thirty-three scheduled workflows:
  seventeen publication workflows; fifteen read-only workflows covering Document Health,
  edit-safety inspection, certificate validation, compression preview, Privacy
  Inspection, OCR confidence review, scan clean-up preview, annotation review,
  bookmark review, Page Finish review, form review, redaction review, page import
  review, Batch Recipe source review, and page-content review; plus connected-scanner
  acquisition. A bounded private journal uses one
  exclusive operating-system lock per active job, skips jobs owned by another live
  application instance, rejects malformed,
  oversized, linked, reparse-point, unknown-field, and future-dated records, and
  restores stale entries exactly once as non-resumable interrupted states. Normal
  completion or cancellation retires the entry. Publication users must check the
  destination before retrying because publication may have completed just before
  interruption; all fifteen read-only workflows rerun from their current inputs. An
  interrupted scanner acquisition is started afresh after the device and feeder are
  reviewed; app-owned pages created before the interruption may remain under the
  separate seven-day recovery policy.
- One generic typed scheduler boundary for scan PDF creation, including start, status,
  cancellation, same-process reattachment, interrupted-state recovery, connection
  diagnostics, and fresh retry. The WebView has no scan-specific compatibility IPC
  commands.
- A privacy-preserving local operation history for all thirty-three scheduled
  workflows. Terminal transitions are recorded exactly once in a cross-process-locked,
  three-generation store capped at 500 entries and 512 KiB. The Activity panel filters
  outcomes, exports path-free JSON to a new file, and removes older generations after
  explicit two-step clearing.
- Multi-source organiser export that combines primary, imported, duplicated, rotated,
  and blank pages into one verified copy while leaving every source unchanged. The
  shared job reports source/page/signature/verification progress, supports cancellation
  before publication and reattachment within the same process, and rejects any source
  whose opening size or modification time changed before or during export.
- Multi-image scan intake and verified, non-overwriting PDF export using embedded
  PNG, JPEG, TIFF, WebP, BMP, GIF, and portable-anymap codecs, with ImageMagick
  fallback for other formats where it is installed.
- A4, US Letter, business card, ID card, and UK driving licence page presets, with
  auto-orientation, 150/300/600 DPI output, margins, image quality, and fitted pages
  that preserve source framing unless reviewed automatic clean-up is enabled.
- Colour, greyscale, and monochrome scan processing, plus optional local searchable
  OCR, bounded engine readiness diagnostics, installed-language discovery, fail-fast
  language validation, OCRmyPDF deskewing controls, and live engine percentages mapped
  monotonically inside the bounded overall scan stage.
- Standalone local searchable OCR for existing PDFs through the same verified engine
  path. The workflow accepts protected input, skips pages that already contain text,
  supports deskew and certificate acknowledgement, reports searchable-page coverage,
  and optionally decrypts and rechecks an AES-256 output before publication.
- Selected-page OCR confidence review using the cleaned export raster, with bounded
  Tesseract TSV parsing, low-confidence overlays, side-by-side word correction, and
  explicitly queued vocabulary hints for the final OCR pass. Hints guide recognition
  but are not presented as guaranteed text-layer replacements. Review uses its own
  shared read-only job with progress, process-tree cancellation, reattachment,
  path-free lifecycle snapshots, content-free failures, and final source-image
  revalidation; recognised words remain volatile and are excluded from diagnostics and
  Activity history.
- Page-by-page searchable-text verification after OCR, with verified-page counts and
  warnings that identify blank or image-only pages needing review.
- Optional scan/OCR output protection with distinct opening and administrator
  passwords. AES-256 is applied to a separate already-verified candidate, which is
  decrypted and checked again for pages, embedded images, and unchanged searchable-text
  coverage before publication.
- Local automatic page-edge detection, confidence-gated cropping, projective camera
  correction, uneven-lighting and shadow balancing, and a selected-page before/after
  preview that uses the same bounded pipeline as export. Preview work has its own
  shared read-only job with progress, cancellation, safe retry, live reattachment,
  source/settings-free lifecycle snapshots, content-free failures, and a final exact
  source fingerprint. JPEG preview bytes remain volatile and are excluded from
  recovery, diagnostics, and Activity history. Source images are unchanged.
- One shared bounded FIFO job queue for scan/OCR creation, standalone searchable OCR,
  compression, privacy
  cleaning, batch recipes and source review, merge, split, organiser export, bookmark publication and
  review, annotation publication and review, page-content publication and review,
  Page Finish publication and review,
  form publication and review, PDF/A archival, certificate
  signing, permanent redaction publication and review, password protection, Document Health, certificate
  validation, compression preview, Privacy Inspection, OCR confidence review, scan
  clean-up preview, and connected-scanner acquisition. At most two workers and sixteen
  non-terminal jobs
  use typed secret-free request snapshots, monotonic stage progress, cancellation
  checkpoints through page, image, object, or validation work, automatic status retry,
  same-process live reattachment, and process-restart interrupted-state recovery.
  Scheduler-backed certificate validation, compression preview, Document Health, scan
  preview, OCR review, and scanner capture have no parallel direct Tauri command.
  Every native structural publisher and validator is now scheduler-only, guarded by an
  exact registered-command allow-list test. Direct IPC is reserved for bounded PDF/image
  byte transport, readiness and capability probes, recovery, Activity history, signature
  vault operations, scanner discovery, signed application updates, and aggregate status.
  PDF.js rendering, search,
  previews, and comparison remain in the WebView over bounded range reads; their loading,
  text-stream, page, and render tasks are cancelled and cleaned up when stale or closed.
  Queued passwords and OCR hints remain only in transient native memory and are
  dropped on cancellation or worker hand-off. Failed or cancelled jobs offer a fresh
  setup retry and a selectable, copyable diagnostic built only from the sanitised
  public snapshot; secret requests and result payloads are never retained for replay.
  Publication results appear only after create-new publication and verification;
  Document Health returns its report only after rechecking that the source is unchanged.
  Certificate validation also rechecks the PDF and every selected trust root before
  returning a path-scrubbed integrity and trust report. Compression preview rechecks its
  source before returning bounded source/candidate image samples and size estimates.
  Privacy Inspection honours cancellation throughout its bounded structure and page
  analysis, then rechecks the source before returning a path-free report.
  Batch Recipe source review maps that inspection lifecycle across up to fifty unique
  sources in one job and returns only ordered path-free reports or content-free errors.
  Page Finish review reports bounded page-geometry and annotation progress, supports
  cancellation, and rechecks the source before returning its typed workspace model.
  Redaction review applies the same lifecycle to page-geometry and annotation traversal
  before returning the destructive-workspace safety model.
  OCR confidence review terminates its local ImageMagick or Tesseract process tree on
  cancellation and rechecks the source image before returning volatile recognised words.
  Scan clean-up preview cancels obsolete settings, rechecks the exact source image
  before delivery, and returns only a bounded volatile JPEG to the current interface.
- A fail-closed signed update client for official builds. Checks happen only after an
  explicit user action; update manifests are strictly constrained to reviewed release
  assets; downloaded packages must pass Tauri's embedded Minisign verification; progress
  is bounded; and restart is explicit. Development and browser builds remain
  unconfigured and offline. Protected signing and promotion
  environments, strict immutable-manifest evidence, byte-verified channel publication,
  and documented withdrawal and forward-version rollback keep release operations
  separate from document processing.
- A shared connected-scanner contract with app-owned, bounded capture storage,
  Windows WIA, macOS Image Capture, and Linux SANE discovery and acquisition, plus
  device-aware flatbed, feeder, duplex, page-limit, resolution, paper-size, and colour
  controls. Capture uses the shared job lifecycle with progress, process-tree
  cancellation, retry, same-process reattachment, one-time interrupted recovery,
  content-free failures, and final output size and modification-time checks. Partial
  work is removed after an ordinary failure or cancellation; successful pages are
  opened sequentially and remain in the app-owned seven-day recovery area. The macOS
  bridge is built as a Tauri sidecar for Intel, Apple Silicon, and universal application
  bundles.
- Signature Studio for named signatures and initials created by freehand drawing,
  typed script/classic/modern text, or PNG, JPEG, WebP, BMP, and TIFF images. Image
  intake removes light backgrounds locally, crops transparent artwork, and supports
  original, black, or blue ink. Session marks can be dragged or placed on any page,
  moved, proportionally resized, rotated, duplicated, deleted, reused, locked, and
  changed through a bounded undo/redo history. Placements follow stable page identities
  through reordering. Export embeds reused PNG assets once, maps geometry through all
  page rotations, flattens marks into ordinary page content, and reopens the result to
  verify the exact number of generated resources on every marked page. Prepared marks
  can be stored in a local AES-256-GCM library protected by an Argon2id passphrase,
  unlocked only for the current session, and deleted through explicit confirmation.
  Version-two vault records retain mark kind and creation method while legacy records
  remain readable as image signatures; names and source details stay encrypted at rest.
- Certificate Studio with PKCS#12 visible or invisible incremental signing, rotated-page
  field placement, optional RFC 3161 HTTPS timestamps, PAdES validation information,
  additional trust roots, and existing-signature inspection through pyHanko. Certificate
  passphrases use a private one-use passfile rather than a process argument. Standard
  password-protected inputs can be signed or validated directly: their PDF password is
  sent through a bounded private standard-input bridge, never a command argument, and is
  cleared from the interface after submission. Every signed copy is reopened with that
  password, required to preserve the source encryption state, checked for a new complete
  byte range, and cryptographically validated before publication; intact but untrusted
  signatures are reported separately from trusted signatures. Signing uses the shared
  cancellable job queue, kills and awaits pyHanko on cancellation, rejects a source changed
  during processing, supports same-process reattachment, and retains only sanitised
  terminal diagnostics.
- Optional signed-copy locking with distinct opening and administrator passwords,
  AES-256 encryption, and change restrictions through QPDF. Reader permissions are
  advisory and are clearly distinguished from certificate-backed signatures.
- AES-256 PDF password protection and password removal through QPDF, with native
  source and destination dialogues, separate administrator passwords, printing,
  copying, and editing permissions, verified output, and source-file protection.
  The shared job reports encryption and verification stages, supports queued or
  running cancellation and same-process reattachment, bounds QPDF diagnostics, and
  rejects a source changed after review or during processing.
- Exhaustive protection-policy coverage for every shared job. Annotation, Batch Recipe,
  bookmark, compression, form, merge, organiser, page-content editing, Page Finish,
  privacy, redaction, scan, standalone searchable OCR, and split publication can add
  optional AES-256 output. Certificate signing preserves
  an encrypted source, PDF/A accepts a protected source but intentionally publishes an
  unencrypted archival copy, and copied job diagnostics identify the applicable rule.
- Local engine detection for QPDF, ImageMagick, img2pdf, OCRmyPDF, Ghostscript,
  Tesseract, veraPDF, and pyHanko.

## Planned Backend Work

- PDF/UA remediation and conversion, PDF/X conversion and independent certification,
  profile-specific colour and transparency semantics, required ICC tag-type checks,
  colourimetric proofing, and retained three-platform standards corpora beyond the
  current formal PDF/A and PDF/UA reports and bounded PDF/X structural preflight.
- Finer engine-level progress for non-OCR tools and execution of the documented
  English, Turkish, rotated, and noisy corpus on every platform.
- Verify connected capture against the documented representative flatbed, feeder,
  duplex, paper-size, DPI, colour-mode, and driver-failure corpus on every platform.
- Run the private visible/invisible PKCS#12, PAdES, HTTPS timestamp, and trust-chain
  corpus against pyHanko on Windows, macOS, and Linux, then package the validated engine.
- Embedded Unicode shaping and font support for CJK, RTL, and other annotation
  appearances outside the current Windows Latin font coverage.

See [feature status](docs/FEATURE_STATUS.md), [the roadmap](docs/ROADMAP.md),
[release programme](docs/RELEASE_PLAN.md), [architecture notes](docs/ARCHITECTURE.md),
[accessibility testing](docs/ACCESSIBILITY_TESTING.md), [localisation](docs/LOCALISATION.md), and
[release metadata guide](docs/RELEASE_METADATA.md) for the implementation and
publication sequence.

## Technology

- Tauri 2 and Rust for the desktop and Apple mobile application shell and trusted
  command layer.
- React, TypeScript, and Vite for the user interface.
- Mozilla PDF.js for local page rendering, thumbnails, text extraction, and search.
- lopdf for self-contained structural page export, privacy cleaning, AcroForm filling
  and flattening, page-box finishing and visual mark layers, image-only permanent
  redaction, and output verification, including standard annotation dictionaries and
  generated appearance streams.
- Rust `image` codecs for self-contained scan processing and PDF creation.
- QPDF for AES-256 encryption, decryption, and document permissions.
- OCRmyPDF and Ghostscript for explicit PDF/A conversion, with veraPDF for independent
  PDF/A and PDF/UA conformance validation.
- Replaceable adapters for PDF, image, OCR, connected-scanner, and signing engines.

## Development

Install Node.js 22.13 or newer, npm 10 or newer, Rust stable, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your operating system.

```bash
npm ci
npm run desktop:dev
```

Run the checks used by CI:

```bash
npm run release:source-check
npm run check
npm run test:frontend
npm run qa:rendering-corpus
npm run build
npm run e2e
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Create native bundles for the current operating system:

```bash
npm run desktop:build
```

On macOS with Xcode and the Apple Rust targets installed, validate and build the
unsigned iPhone/iPad simulator application:

```bash
npm run release:apple-mobile-check
npm run mobile:ios:init
npm run mobile:ios:build-simulator
```

The signed App Store Connect build requires an Apple development team, certificates,
provisioning, and App Store Connect access. See the iPhone and iPad section in the
[build notes](docs/BUILD.md) before attempting it.

Platform tools and packaging details are listed in [the build notes](docs/BUILD.md).
The generated and engine-backed OCR release gates are described in
[OCR corpus testing](docs/OCR_TESTING.md).
The PDF/A conversion, validation, and release-evidence gate is described in
[PDF/A corpus testing](docs/PDFA_TESTING.md).
The generated signing, encryption-preservation, trust-separation, and tagged timestamp
gate is described in [certificate corpus testing](docs/CERTIFICATE_TESTING.md).
The private hardware gate is described in
[connected-scanner testing](docs/SCANNER_TESTING.md).
The keyboard-only and assistive-technology release matrix is described in
[accessibility testing](docs/ACCESSIBILITY_TESTING.md).
The physical-printer, PDF-target, fidelity, cancellation, and privacy matrix is
described in [printing testing](docs/PRINTING_TESTING.md).
The native Tauri/WebDriver boundary, drag-and-drop coverage, signature acceptance
journey, and path-free three-platform evidence contract are described in
[native end-to-end testing](docs/E2E_TESTING.md).
Signed updater setup, promotion, rollback, and key recovery are described in
[signed application updates](docs/UPDATES.md).

## Releases

The desktop CI workflow checks Windows, macOS, and Linux. A separate macOS 15 workflow
generates and compiles the unsigned arm64 iOS simulator application, verifies its
iPhone/iPad bundle metadata, and retains a hashed simulator archive. It does not sign,
upload, or claim an App Store release. Pushing the exact project-version tag,
such as `v0.1.0-alpha.1`, starts the release workflow and creates a draft GitHub pre-release with platform
bundles, including one universal macOS build for Intel and Apple Silicon. The protected
release environment must provide the expected Windows publisher certificate, timestamp
service, Apple Developer ID certificate, and App Store Connect notarisation credentials;
missing or malformed credentials fail before packaging. CI audits the complete
distributable npm graph and applies an exact reviewed policy to every RustSec warning in
the locked Rust graph; vulnerabilities, new warnings, changed package versions, and stale
exceptions fail closed. Current Node 24 GitHub Action majors are tracked by Dependabot.
CI then audits the complete source
tree before building and
rejects generated or private paths, unsupported file types, unsafe encodings, credential
signatures, and personal home paths. Each native build also verifies its exact package
formats, release identity, architecture, container structure, expected publisher
identity, trusted timestamp, macOS Gatekeeper and stapled-ticket state, and hashes.
Linux builds use an ASCII package-manager identity while retaining the branded
desktop name, then test AppImage extraction on Ubuntu 22.04, deb installation on Debian
13, and rpm installation on Fedora 43. Tagged releases attach those path-free reports,
a three-platform native end-to-end, OCR, PDF/A, and certificate evidence set, a deterministic source ZIP, exact
per-file manifest, and SHA-256 checksum. Current release readiness and
unavailable workflows are recorded in
[feature status](docs/FEATURE_STATUS.md). Official tagged builds require the protected
`updater-signing` environment, create platform and updater signing configuration without
writing private key material to the source tree, remove temporary credentials after the
matching build, and retain validated package and `latest.json` hash reports. A
published immutable release reaches `updates-alpha`, `updates-beta`, or `updates-stable`
only through the separately approved, byte-verified promotion workflow. See
[signed application updates](docs/UPDATES.md).

## Security

Treat every PDF and image as untrusted input. Do not attach personal or signed
documents to public issues. See [SECURITY.md](SECURITY.md) for reporting guidance.

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) before
opening a pull request.

## Licence

Tüfekci Paperworks is licensed under `AGPL-3.0-or-later`. See [LICENSE](LICENSE).
