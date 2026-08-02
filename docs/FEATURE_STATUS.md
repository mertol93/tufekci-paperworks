# Feature Status

Tüfekci Paperworks is a local-first pre-alpha project. The first public
release candidate is `0.1.0-alpha.1`; no build should be described as stable until the
remaining release gates below have passed.

The status terms in this document are deliberately narrow:

- **Complete** means the workflow is implemented and covered by repository tests. It
  may still require an optional local engine and does not imply that every possible PDF
  has been tested.
- **Experimental** means the end-to-end workflow is implemented, but representative
  hardware, engine, document-corpus, signing, or platform evidence is still incomplete.
- **Unavailable** means there is no supported production workflow in the current
  source tree.

## Complete Workflows

| Area | Available now | Important limits |
| --- | --- | --- |
| PDF viewing | Local PDF.js rendering, real page counts, lazy thumbnails, zoom, password prompts, selectable text, locale-aware progressive search with retryable page-text caching and content-free failures, translated full-page/thumbnail rendering and display-only annotation states, a generated cross-platform malformed/encrypted/signature-structure/scanned/multilingual/size/large rendering corpus, and native-shell pixel/search plus Turkish/German canvas-name acceptance | The first retained three-platform native matrix and assistive-technology smoke tests remain release gates |
| Page organisation | Stable-ID single and multi-selection through explicit toggles, modifier ranges, Select All and keyboard controls; ordered group or single-page drag; group-aware move, rotate, delete and duplicate; paper-sized blank pages; cancellable reviewed page import; and a two-document drag workspace for verified copy or deferred-removal move | Cross-document transfer preserves rotation, blank geometry, imported provenance and visual marks; inputs are fingerprinted and originals are never overwritten. A move publishes and reopens the destination before one undoable source-plan removal; the remaining source plan is published separately through normal export |
| Page-content editing | Cancellable native review; graphical and keyboard selection; original-font text replacement; image movement, resizing, replacement, and removal; 100-step undo and redo; exact SHA-256 source binding; verified create-new export; and optional AES-256 output | Only exact, unshared, page-level content with supported font encodings and unambiguous image-painting blocks is editable; complex text operators, nested forms, arbitrary vectors, and layout reflow remain read-only; replaced image resources may remain unreferenced until Privacy Cleaner is used |
| Merge and split | Drag or button source ordering, password-aware selected ranges, resolved selected-page bookmark preservation and remapping, extraction groups, verification, optional protected output, and typed four-locale Merge progress, failures, and bounded warnings | Merge omits unresolved and unselected bookmark destinations with exact counts; repeated-page links target the first copy; AcroForm catalogues are reported but not merged |
| Image-to-PDF | Multi-image intake, paper and card presets, orientation, crop, perspective correction, shadow balancing, colour modes, and verified output | HEIC, HEIF, and AVIF need a compatible local ImageMagick installation |
| Visual signatures | Named signatures and initials from freehand drawing, typed styles, or dropped images; local background removal and ink colour; drag/place, move, proportional resize, rotate, duplicate, delete, reuse, lock, undo, and redo; stable-page placement; versioned encrypted local storage with typed path-free outcomes and wrong-passphrase retry acceptance; exact-count flattened export and save/reopen native acceptance | A visual mark is ordinary page artwork, not cryptographic proof of identity; the expanded native case passes on Windows x64, while the first retained macOS/Linux matrix remains a release gate |
| Encryption | QPDF-backed AES-256 opening and administrator passwords, password removal, printing, copying, assembly, form, annotation, and editing permissions, plus exhaustive protection-policy coverage for every structural publisher | QPDF is optional and must be installed locally; reader permissions are advisory; PDF/A output cannot be encrypted |
| Annotations | Cancellable fingerprinted source inspection; editing of self-contained existing FreeText, single-quad highlight, stamp, single-stroke ink, square, circle, and plain line annotations; new text boxes, highlights, stamps, ink, shapes, lines, and images; history; and verified add/update/remove export | Direct-object, linked, rich, multi-part, structurally complex, and over-limit annotations remain preserved read-only; complex-script appearance shaping is unavailable |
| AcroForms | Cancellable fingerprinted field-tree inspection; text, checkbox, radio, and choice filling; generated appearances; validation; history; and supported-field flattening | XFA, signature fields, push buttons, read-only fields, and unsupported field types are not edited |
| Page finishing | Cancellable fingerprinted page-geometry and annotation inspection; crop, fit to paper, watermarks, headers, footers, and Bates numbering | Cropping hides page area; it is not redaction |
| Permanent redaction | Cancellable fingerprinted page-geometry and annotation inspection; reviewed manual and search-assisted regions; native-validated, one-pixel-expanded mask application; image-only replacement; exact reopened pixel-digest checks; and verified removal of selected-page searchable content | Marked pages lose selectable text, vectors, links, forms, and accessibility structure |
| Document checks | Security, structure, privacy, likely blank or duplicate pages, and static accessibility preflight | Findings are guidance, not malware, standards, WCAG, or assistive-technology certification; use the separate PDF Standards workflow for its narrower reports |
| Privacy cleaner | Verified removal of selected metadata, scripts, actions, attachments, annotations, forms, thumbnails, and private structures | It is not redaction, antivirus scanning, or proof that no concealed artwork exists |
| Compression | Quality preview, size estimate, compatible-image recompression, structure preservation, and verified export | Specialist colour spaces, masks, and unsupported image streams are preserved rather than recompressed |
| Comparison | Local text, page-geometry, and selected-page visual comparison | It is a review aid, not a legal or cryptographic equivalence test |
| Navigation | Cancellable fingerprinted bookmark-tree inspection and editing, review-only heading suggestions, and optional linked A4 contents pages with an embedded Unicode font and shifted destinations | Printed contents use physical output page numbers and are not tagged; unsupported font glyphs are visibly substituted without changing bookmark titles |
| Batch recipes | Cancellable fingerprinted source review for up to fifty unique local PDFs, verified scanner and image-scan hand-off, searchable OCR with optional deskew, reviewed privacy-clean and compression steps, PDF/A-1b/2b/3b archival output, settings-only saved recipes, and optional shared non-archival output protection | PDF/A needs local OCRmyPDF, Ghostscript, and veraPDF and cannot be encrypted; protected inputs also need QPDF; scan intake starts only after verified scan-PDF publication |
| Reliability | Shared queued jobs, progress, cancellation, same-process reattachment, interrupted-state recovery, aggregate edit-safety review for up to 250 sources, exact scheduler-only structural IPC enforcement, bounded cancellable PDF.js work, autosaved plans, and privacy-preserving Activity history | Interrupted publication is never replayed; users must check the chosen destination before retrying |
| Application accessibility | Document-editor skip navigation with explicit target focus, high-contrast visible focus, a roving keyboard workflow chooser, labelled landmarks and state, shared focus containment, safe Escape, focus return for all thirteen modal workspaces, and native-shell acceptance | Automated contracts are not WCAG certification; the first retained native matrix and packaged Windows, macOS, and Linux keyboard and assistive-technology matrix remain release gates |

## Experimental Workflows

| Area | Implemented | Evidence still required |
| --- | --- | --- |
| Printing | Local all/current/custom-range preparation from the live organiser plan; reordered, rotated, duplicated, imported, and blank pages; printable PDF annotations and current form values; optional flattened visual signatures and initials; 150 or 300 dpi output; per-page physical CSS print boxes; progress, cancellation, volatile previews, memory limits, `Ctrl+P`/`Command+P`, and the operating system print dialogue | Prepared pages are intentionally rasterised and do not preserve vector output; the native automated case simulates the final dialogue request, so retained physical and PDF-target output evidence on Windows, macOS, and Linux is still required |
| Apple mobile application | Tauri iOS/iPadOS 16+ configuration; iPhone and iPad bundle families; portrait, landscape, multitasking, safe-area, dynamic-viewport, touch-target, keyboard, and pointer foundations; a typed runtime capability contract; backend rejection of unsupported jobs; store-managed update presentation; and a macOS 15 arm64-simulator compile, metadata, archive, and hash gate. Hosted workflow run `30766986118` passed the complete gate and retained its simulator evidence | Signed device and App Store archives need Apple credentials. Device/simulator UI acceptance, Files picker testing, document hand-off, camera capture, TestFlight, App Review, and mobile accessibility evidence remain open. Only the local pure-Rust/PDF.js core is enabled; subprocess engines are unavailable |
| Interface localisation | Exact typed `en-GB`, `en-US`, `tr-TR`, and `de-DE` catalogues with British English default/fallback; persisted local selection; root-language metadata; locale-aware numbers, dates, lists, live document sizes, and document-search casing; translated workflow navigation, shared open/drop/loading/recovery/edit-safety shell, editor, thumbnail, page-canvas, display-only annotation, rendering, and search accessibility states, complete page organiser and cross-document transfer, Merge, output protection, shared job controls, visual signatures and vault, standalone searchable OCR, OCR confidence review, image-scan processing, connected-scanner controls, Print, Split, password protection, the shared protected-PDF opening dialogue, Compression, local Activity, signed Updates, Document Health, Privacy Cleaner, PDF Comparison, Page Finish, Annotation, Forms, Page Content, Permanent Redaction, PDF Standards, Batch Recipes, Bookmarks with heading generation and printed contents, and Certificate Signatures; stable PDF-opening, PDF-search, organiser, discovery, OCR, scan, scanner, split, protection, compression, health, privacy, finishing, annotation, form, content-editing, redaction, archive, batch, batch-inspection, bookmarks, bookmark-inspection, certificate, and certificate-validation codes; mapped content-free organiser, split, compression, privacy, health, comparison, page-finishing, annotation, form, content-editing, redaction, standards, batch, bookmark, and certificate outcomes; catalogue parity plus native Turkish/German canvas-name and AES-256 password evidence | Migrate the remaining native outcomes, installers, accessibility names, release metadata, and user guides; then pass expansion, restart, screen-reader, packaged three-platform and fluent human linguistic review in all four locales |
| Searchable OCR | A dedicated existing-PDF Recognise Text workspace plus image-scan and batch OCR; OCRmyPDF and Tesseract readiness; installed-language selection; deskew; protected input; certificate acknowledgement; optional AES-256 output; searchable layers including OCRmyPDF 17 Form XObjects; page-level verification; confidence review; cancellation; retry; a generated public English/Turkish/rotated/noisy corpus; and mandatory Windows/macOS/Linux engine evidence for tagged drafts | The first tagged three-platform run and representative user-machine testing remain required before removing the experimental label |
| PDF/A archival | Standalone PDF/A-1b, PDF/A-2b, and PDF/A-3b conversion; validation-only reports; optional searchable OCR; independent veraPDF verdicts; bounded failed-rule summaries; protected-input handling; shared-job progress, cancellation, and reattachment; atomic create-new publication; and matching reusable batch recipes | The first tagged Windows/macOS/Linux engine-evidence set and representative institutional archive-policy testing remain required |
| PDF/UA validation | Validation-only PDF/UA-1 and PDF/UA-2 reports through exact-profile veraPDF flavours, bounded generic failed-rule summaries, shared-job progress, cancellation, and source fingerprinting | The first tagged three-platform PDF/UA corpus and hands-on assistive-technology matrix remain required; protected files must first be saved as exact unprotected copies; validation does not repair tags or prove usable reading order |
| PDF/X structural preflight | Validation-only PDF/X-1a:2001, PDF/X-3:2002, and PDF/X-4 checks for profile identifiers, trapping, encryption, GTS_PDFX output intents, bounded ICC structures, embedded fonts, object integrity, page boxes, JavaScript, forms, attachments, external content, non-printing media, and transfer curves | This built-in report is a bounded preflight, not ISO 15930 certification, colourimetric proofing, or print-service approval; profile-specific colour semantics, transparency rules, required ICC tag types, and an independent PDF/X corpus remain open |
| Connected scanners | Windows WIA, macOS Image Capture, Linux SANE, flatbed, feeder, duplex, DPI, colour, paper, progress, cancellation, retry, and recovery | Pass the private physical-device matrix on representative drivers and devices |
| Certificate signatures | Visible and invisible PKCS#12 signing, password-protected input with preserved encryption, RFC 3161 HTTPS timestamps, PAdES information, trust roots, existing-signature validation, cancellation, SHA-256 input snapshots, bounded field metadata, stable localised outcomes, and integrity/trust separation; a disposable Windows x64 engine gate passes visible, incremental, encrypted, trusted, and intact-but-untrusted scenarios | Retain the generated engine report on Windows, macOS, and Linux with timestamping enabled, then add live tampered, revoked, expired, and timestamp-service failure evidence before removing the experimental label |
| Distribution packages | Windows MSI and NSIS, universal macOS DMG, Linux AppImage/deb/rpm, native package identity and structure checks, credential-gated publisher setup, exact signer and timestamp evidence, macOS Gatekeeper and stapled-ticket verification, an Ubuntu 22.04/Debian 13/Fedora 43 installation gate, checksums, manifest, SBOMs, and dependency-licence inventory | The unsigned local Windows package gate passes; the first real credential-backed Windows/macOS tagged evidence, Windows reputation review, first tagged macOS/Linux evidence set, and representative user-machine installation remain required |
| Signed application updates | User-triggered checks, mandatory package-signature verification, explicit alpha/beta/stable channels, download progress, explicit restart, credential-gated tagged builds, strict immutable-manifest evidence, approved byte-verified promotion, and documented withdrawal, rollback, and key recovery | The first real credential-backed three-platform build, channel promotion, packaged update/restart matrix, and withdrawal plus forward-version rollback rehearsal remain required |

## Unavailable Workflows

- Full layout reflow, complex text shaping, nested Form XObject editing, and arbitrary
  vector-object editing.
- XFA form editing.
- Full embedded-font shaping for CJK, RTL, and other complex-script annotation
  appearances.
- PDF/UA and PDF/X conversion, PDF/UA remediation or tag editing, and independent
  PDF/X conformance certification.
- Connected-scanner steps in reusable batch recipes.
- iPhone/iPad camera capture, connected scanners, searchable OCR, PDF/A conversion,
  certificate signing or validation, and QPDF-backed document password changes.
- iOS system-wide document hand-off and external Open In registration.
- Cloud processing, cloud storage, accounts, collaboration, and remote document sync.

## Security Boundaries

- Documents are processed locally by default. Optional QPDF, ImageMagick, OCRmyPDF,
  Ghostscript, Tesseract, veraPDF, pyHanko, WIA, Image Capture, and SANE work also runs
  on the local machine.
- The backend publishes one typed runtime capability contract. On mobile it rejects
  every queued workflow that would need a desktop subprocess or connected scanner,
  even if a stale or modified frontend attempts to request one. Plain image-to-PDF and
  the self-contained local PDF core remain available.
- A source PDF or image is never deliberately overwritten. Publication creates a new
  destination and reopens it for workflow-specific verification.
- Page-content editing exposes only source objects rediscovered by native inspection.
  Full source hashes, exact stream markers, untouched-stream digests, replacement-image
  pixel digests, and final source revalidation protect publication. Unsupported or
  ambiguous page content remains unchanged and read-only.
- AES-256 encryption protects document opening when QPDF is available. Printing,
  copying, and editing permissions depend on PDF readers honouring them.
- A flattened visual signature does not authenticate a signer and is not
  cryptographically tamper-evident. The same is true of flattened initials. A placement
  lock only prevents accidental editor movement. Certificate integrity and
  certificate-chain trust are reported separately.
- OCR text can contain recognition errors. Confidence review and searchable-page checks
  reduce risk but do not certify transcription accuracy.
- Printing renders selected workspace pages into temporary local PNG images before the
  system dialogue opens. A 100-page, 50-megapixel-per-page, and 120-megapixel-per-job
  boundary prevents unbounded canvas allocation. The images are raster output, and
  temporary object URLs are revoked when the document, settings, or workflow changes.
- Permanent redaction removes the underlying selected-page content by rebuilding marked
  pages as lossless image-only pages. Rust validates and applies the reviewed masks and
  verifies the exact resulting pixel digest; this intentionally sacrifices semantic and
  accessibility information on those pages.
- Recovery drafts can contain local source paths and document names. They exclude
  passwords, visual-mark assets and placement history, document text, and document or
  image bytes. Encrypted vault entries are stored separately and require their own
  passphrase.
- Shared-job recovery and Activity history exclude requests, passwords, document
  content, result payloads, and local paths. Successful connected-scanner page paths
  remain volatile in the live process; app-owned capture files expire after seven days.
- Ordinary local and development packages may be unsigned and are not proof of publisher
  identity. Tagged builds fail closed unless Windows installers match the configured
  Authenticode signer and carry a timestamp, and macOS builds match the configured
  Developer ID team, carry a secure timestamp, pass Gatekeeper, and contain a valid
  stapled notarisation ticket. The first real credential-backed and reputation evidence
  remains an open release gate.
- Official release builds accept only updater artefacts signed by their embedded public
  key. This does not replace Windows publisher signing or macOS signing and notarisation.
  Development builds contain no updater endpoint or key and do not check for updates.
  The updater dependency is excluded from iOS and Android builds; iPhone and iPad
  updates are presented as App Store-managed.

## Optional Local Engines

| Engine or service | Used for |
| --- | --- |
| QPDF 11 or newer | AES-256 encryption, password removal, and PDF reader permissions |
| OCRmyPDF and Tesseract language packs | Searchable OCR, deskew, language selection, and confidence review |
| Ghostscript | Explicit PDF/A-1b, PDF/A-2b, and PDF/A-3b conversion through OCRmyPDF |
| veraPDF 1.30.2 or compatible | Independent PDF/A and PDF/UA conformance validation and bounded rule reports |
| ImageMagick | Optional fallback decoding and normalisation for formats such as HEIC, HEIF, and AVIF |
| pyHanko 0.36.2 and pyhanko-cli 0.4.2, or a subsequently reviewed compatible pair | Certificate signing, timestamps, PAdES information, and validation |
| Windows WIA | Connected scanners on Windows |
| macOS Image Capture | Connected scanners on macOS |
| SANE and `scanimage` | Connected scanners on Linux |

Desktop builds probe optional command-line engines and disable or explain dependent
controls when they are unavailable. Mobile builds do not probe or launch these engines.
No engine download is silently performed by the document workflows.

## Release Gates

Before removing the alpha label:

1. Run the automated rendering gate and the engine-backed OCR, PDF/A, PDF/UA, certificate, and
   physical-scanner corpora on every supported operating system.
2. Complete Windows signing and reputation checks, macOS signing and notarisation, and
   retain the first successful tagged Linux package-installation evidence set. Retain
   a signed Apple device archive, TestFlight
   installation, and App Store review evidence before describing iOS/iPadOS as released.
3. Run keyboard-only, accessibility-API, screen-reader, malformed-input, encrypted,
   signed, scanned, and very-large-document end-to-end checks.
4. Publish exact checksums, the release manifest, both CycloneDX SBOMs, and the
   dependency-licence inventory for the final immutable packages.
5. Pass the source-tree audit against the exact staged repository state used for the
   public source archive.
6. Record every exception honestly in the release notes and keep unfinished controls
   disabled or labelled experimental.

See [the release programme](RELEASE_PLAN.md), [build notes](BUILD.md),
[OCR corpus testing](OCR_TESTING.md), [PDF/A corpus testing](PDFA_TESTING.md),
[certificate corpus testing](CERTIFICATE_TESTING.md), and
[connected-scanner testing](SCANNER_TESTING.md) for the engine and hardware checklists.
See [printing testing](PRINTING_TESTING.md) for the physical-printer and PDF-target
matrix.
See [accessibility testing](ACCESSIBILITY_TESTING.md) for the packaged keyboard-only and
assistive-technology matrix, and [signed application updates](UPDATES.md) for channel,
promotion, rollback, and signing-key operations.
