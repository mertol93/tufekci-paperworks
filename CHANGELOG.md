# Changelog

All notable changes to Tüfekci Paperworks will be documented in this file.

The project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- A fail-closed Rust dependency advisory policy. CI and tagged release preflight now run
  pinned `cargo-audit` directly, reject every vulnerability and every new or changed
  informational warning, and require temporary exemptions to match an exact reviewed
  advisory, category, package, and locked version. Stale or 92-day-expired exemptions
  fail too. GitHub workflows now use current Node 24 action majors and Dependabot tracks
  action updates.
- An experimental iPhone and iPad build foundation for iOS/iPadOS 16 and newer. The
  shared Tauri mobile entry point now exposes a typed runtime capability contract;
  React gates desktop-only controls; Rust rejects unsupported OCR, PDF/A, certificate,
  protection, batch, and connected-scanner jobs; and iOS excludes the desktop updater
  in favour of an App Store-managed state. Checked-in mobile configuration covers both
  device families, orientations, iPad multitasking, safe areas, dynamic viewport
  sizing, coarse-pointer targets, and indirect input. A credential-free macOS 15 CI job
  generates the ignored Xcode project, compiles the unsigned arm64 simulator app,
  verifies bundle identity and metadata, and retains hashed simulator evidence. Signed
  device/TestFlight/App Store distribution, camera capture, document hand-off, native
  mobile OCR, and device-level interaction and accessibility evidence remain open gates.
- Narrow test-tool security overrides for `serialize-javascript` 7.0.5 and
  `brace-expansion` 1.1.17, with a clean locked install and zero-advisory npm audit.
- Typed, four-locale Merge and encrypted visual-mark vault outcomes. Merge now exposes
  stable checking, preparation, protection, verification, publication, and failure
  codes; action failures and result warnings use bounded catalogue templates; unknown
  backend prose cannot enter display state; and transient job-history/status failures
  are content-free codes. Vault commands now return only an allow-listed native error
  code, keeping filesystem and cryptography detail behind the IPC boundary while
  preserving the deliberately ambiguous wrong-passphrase-or-altered-copy result.
  Existing results re-render after a locale change, passphrases remain retryable and
  memory-only, and native acceptance encrypts a mark, rejects a wrong passphrase,
  unlocks it on retry, and deletes the fixture.
- Complete four-locale PDF reader and text-search presentation. Full-page canvases,
  thumbnails, rendering progress, display-only annotation layers, inert control titles,
  and render failures now use exact typed catalogue keys with locale-aware page numbers
  and restrained screen-reader status or alert semantics. Progressive search now applies
  the selected locale during compatibility normalisation and lowercasing, including
  Turkish dotted and dotless I, stores only a stable content-free extraction outcome,
  and evicts rejected page-text promises so a later query can retry. Adversarial tests
  prove raw parser paths cannot be retained, and Windows native acceptance verifies the
  live page-canvas name after Turkish and German switching.
- Complete four-locale protected-PDF opening boundary. The shared password dialogue now
  translates its labels, privacy guidance, accessible incorrect-password announcement,
  cancellation, and actions in British English, American English, Turkish, and German;
  keeps focus in place while PDF.js checks a retry; marks the field as autocomplete-off;
  and rejects line separators, null bytes, and values over 1,024 UTF-8 bytes. Whole-file
  and range failures collapse to stable cancelled, changed, invalid, password,
  unreadable, or unknown codes before React retains them, so native paths and exception
  prose cannot reach the interface. Windows native acceptance rejects and then accepts
  the generated AES-256 fixture in Turkish, reopens its German prompt, cancels safely,
  and verifies the restored document journey.
- Complete four-locale Certificate Signatures workspace and a reproducible real-engine
  gate. PKCS#12 signing, visible-field placement, RFC 3161 and PAdES options, trust-root
  selection, validation states, bounded field metadata, warnings, results, accessibility
  names, and cancellation now use typed catalogues and stable native codes. Signing and
  validation run pyHanko against SHA-256-verified private snapshots, recheck the original
  PDF, identity, and trust roots, cap reported fields and text, and never expose raw
  engine diagnostics in the interface. A generated disposable identity proves visible
  signing, a second incremental signature, encrypted-input preservation, save/reopen,
  configured trust, and intact-but-untrusted separation on Windows x64, then removes all
  key material and retains only a closed path-free report. The run also corrected
  pyHanko integer field geometry and incremental byte-range verification. Tagged release
  jobs require the same evidence on all three platforms with timestamping enabled.
- Complete four-locale Bookmarks and printed-contents workspace. Source selection,
  outline editing, hierarchy actions, heading analysis and review, generated A4
  contents, validation, locale-aware counts and sizes, accessibility names, output
  protection, verified results, and warnings now use the typed catalogues. Stable
  bookmark inspection and export stage and failure codes replace native progress
  prose; exact warning mappings retain only safe counts; unknown backend text becomes
  one translated generic notice; visible paths are reduced to basenames; and native
  acceptance visits the workspace in Turkish and German.
- Complete four-locale PDF Standards and Batch Recipes workspaces. Controls,
  profile and recipe descriptions, engine readiness, validation summaries, source
  inspection, scanner hand-offs, locale-aware counts and sizes, recipe storage,
  per-file steps, notes, warnings, and verified results now use the typed catalogues.
  Stable archive, batch, and batch-inspection stage and failure codes replace native
  progress prose; validator evidence and backend outcomes pass through closed
  allow-lists; unknown text becomes one translated generic notice; visible paths are
  reduced to safe basenames; and native acceptance visits both surfaces in Turkish
  and German.
- Complete four-locale Page Content and Permanent Redaction workspaces. Controls,
  validation, search assistance, accessibility names, locale-aware counts and sizes,
  destructive-action guidance, and verified results now use the typed catalogues.
  Stable content/redaction inspection and publication codes replace native progress and
  failure prose; exact warning allow-lists collapse unknown backend text to translated
  generic notices; source paths and edit-safety failures stay out of visible messages;
  and native acceptance visits both surfaces in Turkish and German.
- Complete four-locale Annotation and Forms workspaces. Controls, field and annotation
  types, validation, search, accessibility names, locale-aware counts and sizes, image
  preparation, XFA and flattening states, and verified export results now use the typed
  catalogues. Stable annotation/form inspection and publication codes replace native
  progress and failure prose; exact warning allow-lists collapse unknown backend text
  to translated generic notices; and native acceptance visits both surfaces in Turkish
  and German.
- Complete four-locale Document Health, Privacy Cleaner, PDF Comparison, and Page
  Finish interfaces. Stable health, privacy, privacy-inspection, finishing, and
  finishing-inspection job codes replace native progress and failure prose; health and
  privacy findings, privacy and page-finishing warnings, comparison errors, paper names,
  ranges, validation, accessibility labels, previews, and locale-aware results use typed
  catalogues with generic handling for unknown native text. Native acceptance visits all
  four surfaces in Turkish and German, while focused tests prove private paths cannot
  enter translated messages.
- Complete four-locale Split, password-protection, Compression, local Activity, and
  signed-Update interfaces. Locale-aware counts, dates, durations, percentages, and file
  sizes replace fixed English formatting; stable split/protection/compression native
  stage codes cover live progress and recovery; exact split and compression warning
  allow-lists prevent private native detail from entering translated messages; and
  native acceptance visits all five surfaces in Turkish and German.
- A reviewed two-document page-transfer workspace. One or many selected source pages
  remain visible beside a password-aware destination PDF and can be dragged to an exact
  insertion marker or placed by page number. Copy preserves the source plan; Move removes
  pages only after the create-new destination has been written, reopened and structurally
  verified, and that source-plan change is one undoable operation. Rotations, blank-page
  geometry, imported source provenance and visual-signature placements follow the pages;
  repeated source PDFs are deduplicated; certificate signatures require explicit
  acknowledgement; a distinct secret-free `page-transfer` scheduler identity prevents
  reattachment to the ordinary organiser surface; optional AES-256 protection uses the
  existing QPDF boundary; large destinations render a bounded thumbnail window; and the
  complete workflow is available in all four release locales. Native Windows acceptance
  proves group drag, copy, move,
  verified destination output, unchanged copy source, deferred move removal and undo.
- Complete four-locale catalogue coverage for the direct Import Pages dialogue,
  including source selection, page ranges, passwords, certificate acknowledgement,
  progress, cancellation, review results and content-free failures.
- First-class multi-page organisation. Thumbnail toggle controls, Ctrl/Command clicks,
  Shift ranges, Ctrl/Command+A, and keyboard-modified Space maintain an ordered selection
  with one clearly active preview page. Dragging any selected thumbnail moves the whole
  selection without scrambling it; Move Earlier/Later, Rotate, Duplicate, and Delete act
  on the selected set as one reversible history operation. Stable page identities retain
  imported-page provenance and visual-signature placement, selection states and controls
  are catalogued in all four release locales, and native acceptance proves group drag,
  step movement, rotation, duplication and deletion with one-step undo before continuing
  through ordinary page editing.
- A local Print workflow for the active organiser state. All, current, and validated
  custom ranges preserve reordered, rotated, duplicated, imported, and blank pages;
  PDF.js print intent includes printable annotations and current form values; and users
  can include or exclude flattened visual signatures and initials. Standard 150 dpi and
  High 300 dpi preparation has visible progress, cancellation, volatile previews,
  per-page physical print boxes, a 100-page/50-megapixel-page/120-megapixel-job bound,
  `Ctrl+P` and `Command+P`, and a real operating-system dialogue for printer-specific
  settings. The complete interface is catalogued in all four release locales. Native
  WebView2 acceptance proves non-blank prepared pixels and the isolated dialogue request;
  retained physical-printer and PDF-target output on all three platforms remains open.
- A complete local visual-signature and initials workflow. Users can create named marks
  by drawing, typing in three styles, or importing PNG/JPEG/WebP/BMP/TIFF artwork with
  local background removal and ink recolouring; drag or place them on any page; move,
  proportionally resize, rotate, duplicate, delete, reuse, lock, undo, and redo. Native
  export embeds reused images once, maps arbitrary angles through rotated pages,
  flattens every placement, and requires exact per-page resource counts after reopening
  both ordinary and optionally protected output. The Argon2id/AES-256-GCM vault now
  encrypts mark kind and creation method in a backward-compatible version-two payload,
  while recovery and public job snapshots retain no artwork or placement state. The
  complete signature interface and stable artwork failures are catalogued in British
  English, American English, Turkish, and German. Windows native acceptance creates
  image, typed, and freehand marks and proves editing, history, locking, export, and
  save/reopen fidelity; macOS/Linux retained evidence remains a release gate.
- A typed, local-only interface localisation foundation for British English, American
  English, Turkish, and German. British English is the canonical default and fallback;
  the explicit selector stores only its locale identifier and updates root language
  metadata. Workflow navigation, editor tools, Merge, shared document opening,
  drag-and-drop, loading, recovery and edit-safety states, the complete page organiser,
  output protection, shared PDF-job controls, the complete visual-signature workflow,
  standalone searchable OCR, OCR confidence review, image-scan processing, and
  connected-scanner controls now use exact-key catalogues with placeholder-parity and
  locale-aware number, date, list and live document-size tests. Organiser, OCR review,
  searchable OCR, scan export, scan preview, and scanner capture expose stable
  translated stages; organiser warnings are mapped without exposing unknown native
  text, and discovery uses a stable status. Native evidence switches Turkish and
  German, verifies translated organiser, Merge, OCR, scan, scanner and accessibility
  controls plus persistence, captures German organiser evidence, proves locale-aware
  size formatting, and restores British English. Remaining native stage/error,
  installer, accessibility, release-metadata, and documentation migration stays an
  explicit release gate.
- Selected-page bookmark preservation for Merge. The graphical plan now enables
  preservation by default, supports drag handles alongside keyboard-accessible move
  controls, hides Windows transport prefixes, and reports preserved and omitted counts.
  Native publication resolves and filters each source tree, promotes retained
  descendants, maps repeated-page links to the first copy, enforces a 2,000-entry bound,
  and reopens prepared and AES-256 candidates to compare the exact rebuilt tree.
- Optional linked printed contents in the bookmark workspace. A graphical level filter,
  title control, sidebar-bookmark choice, A4 page estimate, and shifted-destination
  preview feed the existing cancellable publication job. Native export prepends bounded
  pages, embeds Liberation Sans with a ToUnicode map, creates direct Fit links, shifts
  the edited outline, and reopens prepared and AES-256 candidates to verify page markers,
  text streams, entry counts, font files, links, forms, and the resulting bookmark tree.
  Generated pages are explicitly untagged and use physical output page numbers.
- A connected standalone Recognise Text workflow for existing scanned PDFs. It uses
  the same verified local OCRmyPDF and Tesseract path as Batch Recipes, with graphical
  source and password selection, installed-language choice, optional deskew,
  certificate acknowledgement, progress, cancellation, retry, page-level searchable
  coverage, and optional AES-256 output. Its strict thirty-third scheduler kind keeps
  queued paths and passwords transient, publishes only a create-new verified copy, and
  retains content-free failures.
- Native-reviewed editing of exact existing page text and images. A full-screen
  graphical workspace provides page overlays, object navigation, keyboard and pointer
  selection, image movement, percentage resizing, replacement and removal,
  original-font text replacement, and 100-step undo/redo. Shared inspection and
  publication jobs use opaque source identities, full-document stream-ownership
  reference counts, and full SHA-256 source binding;
  preserve unsupported, shared, nested, or ambiguous content read-only; and verify
  edited and untouched stream hashes, replacement-image pixels, page count, forms,
  bookmarks, and annotations after create-new export and optional AES-256 decryption.
- Native-enforced permanent-redaction masks. The interface now sends a clean bounded
  page raster plus reviewed normalised black or white regions instead of a claimed
  redaction count. Rust rejects unknown, empty, non-finite, undersized, out-of-bounds,
  over-count, and excessive-work inputs; expands each mask by one raster pixel; applies
  overlapping regions in reviewed order; and hashes the complete flattened RGB image.
  Prepared and decrypted AES-256 candidates must reproduce that exact SHA-256 digest,
  so changed pixels fail publication even when page dimensions, markers, and counts
  still match.
- Existing-annotation editing for a fail-closed standard subset: self-contained
  `FreeText`, single-quad `Highlight`, `Stamp`, single-stroke `Ink`, `Square`, `Circle`,
  and plain `Line` annotations now enter the graphical history with move, property edit,
  duplicate, delete, undo, and redo. Inspection returns bounded stable source and PDF.js
  viewer identities; the preview suppresses only editable source appearances while
  preserving unsupported items visibly read-only. Publication separates add, update,
  and remove sets, rejects stale or invented identities, preserves author metadata and
  every untargeted annotation, and reopens prepared and AES-256 outputs to verify exact
  per-page counts plus replacement markers.
- Zero-vulnerability production npm and RustSec release gates, plus weekly npm/Cargo
  Dependabot tracking. Development-only WebDriver advisories remain visible and
  require maintainer review until compatible upstream fixes are available.
- A test-only native Tauri/WebDriver acceptance boundary and strict path-free evidence
  contract for Windows, macOS, and Linux. Eleven workflows cover shell and engine
  readiness, the standalone Recognise Text workspace, roving keyboard and skip focus,
  modal focus containment, real Rust-backed
  PDF opening, page drag reordering plus duplicate/blank/rotate/delete/undo/redo,
  non-blank PDF.js pixels and search, local print-intent preparation plus a simulated
  system-dialogue request, native-reviewed page-text replacement with
  undo/redo and verified publication, linked printed-contents preview and verified
  publication, merge-source drag ordering with selected-page bookmark preservation,
  image, typed-initials, and freehand visual-mark creation; drag placement, movement,
  proportional resize, rotation, duplication, undo/redo, locking, exact-count flattening,
  and save/reopen fidelity; plus persisted Turkish and German interface switching with
  root-language metadata.
  Production builds exclude the optional plugins, capabilities, global
  Tauri API, one-shot chooser, and diagnostics, then scan emitted assets for leakage.
  CI retains each operating-system report and tagged metadata requires one complete
  three-platform matrix; the first hosted matrix remains a release gate.
- Signature images can now be dropped directly into Signature Studio, with explicit
  PNG, JPEG, WebP, BMP, and TIFF validation. Page thumbnails expose source-aware labels,
  and skip navigation explicitly focuses the document editor after activation.
- Credential-gated platform publisher signing for tagged releases. Windows builds import
  a validated non-exportable PFX, bind Tauri and package evidence to its exact thumbprint,
  require SHA-256 signing and a trusted timestamp, and remove the temporary certificate.
  macOS builds use an ephemeral keychain, exact Developer ID team binding, secure
  timestamps, App Store Connect notarisation, Gatekeeper assessment, stapled-ticket
  validation, and immediate credential cleanup. Secret-free ignored overlays and strict
  schema-versioned reports keep the expected public signer in evidence without exposing
  certificate bytes, passwords, or API private keys. The first real credential-backed
  packages, Windows reputation review, and representative installations remain release
  gates.
- A fail-closed signed application updater for official release builds, with explicit
  alpha, beta, and stable channels; user-triggered checks; mandatory Minisign
  verification; bounded download progress; explicit restart; and an accessible modal
  that remains offline in browser and development builds. Tagged builds require a
  protected signing environment, generate updater configuration without writing key
  material, validate the complete immutable-release manifest, and retain path-free
  evidence. A separately approved promotion workflow publishes byte-verified channel
  manifests, with documented withdrawal, forward-version rollback, manual recovery,
  and key-rotation procedures. Real credential-backed three-platform and rollback
  evidence remains a release gate.
- An application accessibility baseline with a visible-on-focus skip link to the
  document editor, a single-stop workflow tab set with Arrow, Home, and End navigation,
  consistent high-contrast focus indicators, and shared initial-focus, Tab containment,
  safe Escape, and opener-focus return behaviour across all twelve modal workspaces.
  Automated contracts cover the shell and every modal consumer; a separate public
  Windows, macOS, and Linux keyboard and screen-reader matrix remains mandatory for
  packaged release candidates.
- A verified scan-to-Batch-Recipe hand-off for connected scanners and ordinary image
  batches. The scan must first pass the existing reviewed create-new PDF export; its
  success banner can then open Batch Recipes with the output preselected and labelled by
  origin. The transient hand-off contains only the verified path and origin, carries no
  scanner identifier, image bytes, settings, or password, and asks for any opening
  password again before inspection. Saved recipes continue to store settings only.
- An exhaustive shared-job protection contract covering every native PDF job. Fourteen
  structural publishers offer optional AES-256 output, certificate signing preserves
  source encryption, PDF/A accepts protected input but must publish an unencrypted
  archival copy, and the Protection workflow adds or removes encryption. Static release
  checks bind the contract to the Rust job registry, publisher password fields,
  cancellable QPDF publication, protected-output reopening, and standard-input secret
  delivery; copied job diagnostics now state the applicable protection policy.
- An exact Tauri command allow-list regression that keeps every native structural
  publication and validation workflow behind the generic scheduler. Remaining direct
  IPC is limited to bounded document transport, readiness, recovery, audit, signature
  vault, scanner discovery, and status calls. A companion regression fixes the PDF.js
  comparison bounds, progressive reporting, loading/text/render cancellation, and page
  clean-up contract.
- Password-aware certificate signing and validation for encrypted PDFs, with the PDF
  password delivered to pyHanko through a bounded private standard-input bridge rather
  than process arguments. The bridge is an app-owned crash-cleaned temporary directory;
  signed output must reopen with the same password and preserve the source encryption
  state before publication. Certificate job snapshots, recovery, diagnostics, and
  Activity history retain neither the password nor local paths.
- Shared edit-safety review through one distinct aggregate read-only job for up to 250
  PDFs, with debounced starts, exact stale-job cancellation, staged per-source
  progress, bounded passwords, object streams, objects and pages, safe retry,
  one-time interrupted recovery, source/password-free snapshots, ordered path-free
  results, content-free per-source failures, and a final fingerprint for every
  successfully inspected source.
- A reduced Tauri IPC surface for scheduler-backed work: obsolete direct wrappers for
  scan export, certificate validation, compression preview, Document Health, scan
  clean-up preview, OCR confidence review, scanner capture, and edit-safety inspection
  are no longer registered. Scan creation now uses the generic typed start, status,
  cancellation, recovery, and diagnostics lifecycle directly. Synchronous helpers
  remain internal to backend enforcement or native tests, and one static regression
  guards the handler.
- Batch Recipe source review through one distinct shared read-only job for up to fifty
  unique PDFs, reusing bounded Privacy Inspection with per-file mapped progress,
  immediate cancellation, safe retry, same-process reattachment, one-time interrupted
  recovery, source/password-free snapshots, ordered path-free results, content-free
  per-file failures, and a final fingerprint for every successfully inspected source.
- Selected-page import inspection through a distinct shared read-only job, with bounded
  path, password, and range validation; cancellable certificate-structure traversal;
  safe retry; same-process reattachment; one-time interrupted-state recovery;
  source/password/range-free snapshots; content-free failures; and an exact final
  source fingerprint before the typed page selection is delivered.
- Permanent-redaction workspace inspection through a distinct shared read-only job,
  with bounded page-geometry and annotation traversal, progress, queued or running
  cancellation, safe retry, same-process reattachment, one-time interrupted-state
  recovery, source/password-free snapshots, content-free failures, and an exact final
  source fingerprint before the typed review model is delivered.
- Page Finish workspace inspection through a distinct shared read-only job, with
  bounded page-geometry and annotation traversal, progress, queued or running
  cancellation, safe retry, same-process reattachment, one-time interrupted-state
  recovery, source/password-free snapshots, content-free failures, and an exact final
  source fingerprint before the typed finishing model is delivered.
- AcroForm workspace inspection through a distinct shared read-only job, with bounded
  page/widget discovery and recursive field-tree progress, queued or running
  cancellation, safe retry, same-process reattachment, one-time interrupted-state
  recovery, source/password-free snapshots, content-free failures, and an exact final
  source fingerprint before the typed field model is delivered.
- Bookmark workspace inspection through a distinct shared read-only job, with bounded
  named-destination and outline traversal, progress, queued or running cancellation,
  safe retry, same-process reattachment, one-time interrupted-state recovery,
  source/password-free snapshots, content-free failures, and an exact final source
  fingerprint before the typed bookmark tree is delivered.
- Annotation workspace inspection through a distinct shared read-only job, with
  page-level progress, queued or running cancellation, safe retry, same-process
  reattachment, one-time interrupted-state recovery, source/password-free queued
  snapshots, content-free failures, and an exact final source fingerprint before the
  typed review report is delivered.
- PDF/A Archive for conversion to PDF/A-1b, PDF/A-2b, or PDF/A-3b and validation-only
  conformance reports. Conversion uses explicit OCRmyPDF/Ghostscript output profiles,
  optional searchable OCR, independent veraPDF validation, page-count and encryption
  checks, source-fingerprint revalidation, shared-job progress and cancellation, and
  atomic create-new publication. Reports retain only bounded generic rule evidence.
- PDF/A Batch Recipe support after OCR, privacy cleaning, and compression, including a
  built-in PDF/A-2b archive recipe, version-three settings migration, protected-input
  temporary unlock, conformance validation before all-or-nothing publication, and an
  explicit ban on incompatible output encryption.
- A three-platform PDF/A engine-evidence gate using the public image-only OCR fixture,
  all three supported profiles, and the real batch recipe. Release CI pins veraPDF
  1.30.2 by SHA-256, pins the Windows Ghostscript 10.07.1 installer, and retains a
  closed-schema report containing engine versions and conformance counts without paths
  or recognised document text.
- Searchable OCR and optional deskew steps for reusable Batch Recipes, with installed
  language selection, a built-in searchable archive recipe, safe migration of existing
  settings-only recipes, password-aware page-count fingerprints, protected-source
  temporary unlock, verified searchable-page coverage, step-level progress and direct
  public-corpus evidence. Privacy cleaning and compression follow OCR, while optional
  AES-256 protection is applied once to each final candidate.
- Strict native release-package verification for Windows MSI/NSIS, universal macOS DMG,
  and Linux AppImage/deb/rpm. Reports bind bounded filenames, release metadata,
  architecture, container structure, package or payload hashes, and publisher-signature
  state without local paths. The Windows x64 gate passes against the real alpha bundles.
- Tagged Linux package testing on an Ubuntu 22.04 build baseline, with AppImage
  extraction on Ubuntu 22.04, deb installation and dynamic-link checks on Debian 13,
  and rpm installation and dynamic-link checks on Fedora 43. Exact container image IDs
  are retained, and a strict aggregate blocks release metadata until all platform and
  installation reports agree.
- A Linux-specific ASCII package identity for deb and rpm portability, paired with a
  custom desktop template that preserves the visible `Tüfekci Paperworks` brand.
- A deterministic, redistributable UK English, Turkish, physically rotated, and noisy
  OCR corpus generated from the bundled Liberation Sans font, with strict path, UTF-8,
  hash, dimension, size, language, recall, and pixel validation on Windows, macOS, and
  Linux CI.
- A bounded OCR engine-evidence runner that records OCRmyPDF and Tesseract versions,
  required language data, searchable-page verification, engine progress and observed
  token recall without recognised text or local paths. Tagged drafts require Ubuntu,
  macOS, and pinned native Windows reports; the Windows x64 corpus passes locally at
  100% observed recall for all four fixtures.
- Bounded native traversal of invoked OCR Form XObjects, allowing OCRmyPDF 17's
  invisible ToUnicode text layers to be verified and measured without treating unused
  resources as searchable content. Windows child OCR now uses UTF-8 diagnostics, and
  the app-supplied progress plug-in explicitly enables content-free progress records
  when OCRmyPDF disables terminal progress for piped standard error.
- A generated, redistributable PDF.js rendering corpus for encrypted password
  challenges, certificate structures, image-only scans, CJK and RTL text, unusual page
  sizes, 320-page loading, and malformed rejection. Windows, macOS, and Linux CI render
  bounded representative pages through the native canvas backend and retain path-free
  per-platform hashes, geometry, operator, text-direction, annotation, and pixel
  evidence.
- A mandatory distributable source-tree audit for local handoff, pull requests,
  main-branch builds, and tagged-release preflight. It enforces required public files,
  bounded source size, reviewed file types, strict UTF-8/LF text, generated/private
  exclusions, and checks for credential signatures and personal absolute home paths.
  The tagged workflow creates an exact-index deterministic source ZIP, reopens and
  byte-verifies every member, and publishes its per-file manifest and SHA-256 checksum.
- Strict release-identity validation across npm, both npm lockfile roots, Cargo
  structured metadata, and Tauri configuration. Tagged builds now reject anything
  except the exact `v<version>` tag and derive GitHub's pre-release flag from the shared
  semantic version before any platform package is built. The explicit numeric WiX
  version is derived from and checked against the final pre-release sequence.
- A public feature-status and security-boundary matrix that distinguishes complete,
  experimental, and unavailable workflows; records optional local-engine dependencies;
  and names the rendering, OCR, certificate, scanner, signing, notarisation, Linux
  package, accessibility, and end-to-end evidence still required before a stable release.
- Connected-scanner acquisition through the shared typed scheduler, with staged
  progress, queued or running cancellation, WIA/Image Capture/SANE process-tree
  termination, safe retry, same-process reattachment, one-time interrupted-state
  recovery, device/settings-free queued and interrupted snapshots, content-free
  failures, final captured-page size and modification-time checks, and sequential
  opening of volatile successful page paths into scan review. Ordinary failure and
  cancellation remove partial capture work; crash remnants retain the existing
  seven-day recovery policy.
- Scan clean-up preview through a distinct shared read-only job, with debounced
  automatic starts, stale-work cancellation, staged bounded image processing,
  same-process reattachment, one-time interrupted-state recovery, source/settings-free
  lifecycle snapshots, content-free failures, final source-image fingerprinting, and
  volatile JPEG results excluded from persistence, diagnostics, and Activity history.
- OCR confidence review through a distinct shared read-only job, with staged image
  preparation, cancellable ImageMagick and Tesseract process trees, same-process
  reattachment, one-time interrupted-state recovery, source/settings-free lifecycle
  snapshots, content-free failures, final source-image fingerprinting, and explicit
  exclusion of recognised words from persistence, diagnostics, and Activity history.
- Privacy Inspection through a distinct shared read-only job, with cancellable bounded
  object, optional-content, resource, annotation, and page-stream traversal; staged
  progress; same-process reattachment; one-time interrupted-state recovery;
  source/password-free lifecycle snapshots; path-free reports; content-free failures;
  and a final exact source fingerprint before report delivery.
- Compression preview through a distinct shared read-only job, with staged bounded
  image and sample work, queued or running cancellation, same-process reattachment,
  one-time interrupted-state recovery, source/password-free lifecycle snapshots,
  path-free retained report metadata, content-free failures, and an exact final source
  fingerprint before volatile sample delivery.
- Certificate-signature validation through a distinct shared read-only job, with
  cancellable bounded structural traversal and pyHanko process-tree execution, staged
  progress, same-process reattachment, one-time interrupted-state recovery, path-free
  queued and successful snapshots, content-free retained failures, and exact final PDF
  and trust-root fingerprints.
- Document Health through the shared typed scheduler with staged progress across
  bounded object, font, colour, accessibility, page, content-operator, and nested Form
  inspection; queued and running cancellation; same-process reattachment; one-time
  interrupted-state recovery; content-free retained failures; path/password-free
  queued snapshots; and a final exact source size/modification-time gate.
- Cross-platform process-tree ownership for long-running QPDF, pyHanko, OCRmyPDF,
  Tesseract, ImageMagick, WIA, Image Capture, and SANE commands. Windows Job Objects
  and separate macOS/Linux process groups now terminate descendants on cancellation,
  timeout, monitor failure, parent exit, or wrapper drop; scanner output and runtime
  are bounded and a real parent-plus-grandchild regression test guards the contract.
- Tagged-release SHA-256 checksums, an exact package manifest, deterministic npm and
  Cargo CycloneDX 1.5 SBOMs, and combined JSON/CSV dependency-licence declarations,
  with cross-platform workflow artefact gathering and automatic draft-release upload.
- Cross-platform Tauri, React, and Rust application shell.
- PDF workflow interface for organising, merging, splitting, OCR, signing, and protection.
- Image scan intake with A4, US Letter, business card, ID card, and driving licence presets.
- Local signature image processing with background removal and transparent PNG output.
- Authenticated local signature storage using Argon2id-derived AES-256-GCM keys,
  encrypted labels and image metadata, session-only unlock, and deliberate two-step
  deletion controls.
- Page-specific signature placement that follows page reordering, editor locking,
  transparent PDF flattening, rotated-page transforms, and signed-output verification.
- Bounded pyHanko certificate adapter and responsive Certificate Studio for PKCS#12
  visible or invisible incremental signatures, rotation-aware fields, optional HTTPS
  RFC 3161 timestamps, PAdES validation data, additional trust roots, and structured
  inspection of existing signatures.
- Private one-use certificate passfiles, source-overwrite prevention, create-new
  temporary output, structural byte-range verification, post-signing cryptographic
  validation, and explicit separation of integrity from certificate trust.
- Private lock-backed temporary-workspace leases for PDF candidates, batch directories,
  certificate passfiles, OCR hints, and scan rasters, with bounded start-up recovery of
  unlocked crash remnants, strict path/type/link validation, batch ownership tokens,
  path-free aggregate status, and explicit preservation of seven-day scanner captures.
- Secret-free process-restart handling for all thirty-three scheduled workflows, using
  private create-new active-job records, per-job operating-system
  locks, live-instance exclusion, strict 8 KiB schemas, bounded one-time stale recovery,
  explicit non-resumable interrupted results, and fresh retry guidance that accounts
  for the possible post-publication crash window.
- Privacy-preserving Activity history for all thirty-three scheduled workflows,
  with exactly-once terminal records, an allow-listed path-free schema, cross-process
  locking, three-generation interrupted-write recovery, 500-entry/512-KiB bounds,
  outcome filtering, create-new JSON export, and confirmed clearing.
- Certificate signing through the shared typed scheduler with child-process
  cancellation, final source-fingerprint validation, same-process reattachment,
  frozen controls, and sanitised retained job results.
- Optional AES-256 permanent-redaction output, applied after prepared-candidate
  verification and followed by decrypted repetition of every image-only, marker,
  searchable-text, page-geometry, and privacy-structure check.
- Optional AES-256 Privacy Cleaner output with prepared and decrypted
  selected-category verification, a final inspected-source fingerprint gate, shared
  controls, encrypted-result reporting, and content-free retained failures.
- Optional AES-256 compression output with prepared and decrypted page, form, and
  bookmark verification, a final source-fingerprint gate, smaller-final-output
  enforcement, shared controls, encrypted-result reporting, and content-free retained
  failures.
- Ignored private certificate release gate covering visible and invisible incremental
  signing, trusted timestamps, PAdES data, multiple signatures, and final trusted
  validation without committing a signing identity to the repository.
- Optional signed-copy AES-256 opening and administrator passwords with QPDF-backed
  change restrictions and explicit advisory-permission warnings.
- AES-256 PDF password protection and password removal through QPDF.
- Separate opening and administrator passwords with printing, copying, and editing permissions.
- Native source and destination dialogues with verified, non-overwriting protected output.
- Password addition and removal now run as typed shared jobs with secret-free snapshots,
  visible stages, queued and running cancellation, retrying status polls, and same-process
  reattachment. QPDF child output is drained into a 1 MiB diagnostic bound, execution has
  a 30-minute safety timeout, and cancellation terminates and waits for the child.
- Protection requests carry the reviewed source size and modification time. Native work
  checks that fingerprint before QPDF starts and after output verification immediately
  before create-new publication; direct protection commands are no longer exposed to
  the WebView.
- Local PDF.js page rendering with real page counts and high-DPI zoom.
- Lazy page thumbnails, password-protected PDF opening, and progressive text search.
- Bounded desktop PDF.js range loading with a 64 KiB initial sample, cancellable local
  reads, source size and modification checks, and memory-backed browser-file fallback.
- Bundled PDF.js CMaps, standard fonts, ICC profiles, WASM, and worker assets for offline use.
- Display-only PDF.js annotation and AcroForm appearance layers with bundled icons,
  cancelled stale renders, disabled scripts and links, and inert form controls.
- Non-destructive page plans with drag-and-drop and button-based reordering, rotation,
  deletion, duplication, paper-sized blank pages, undo, and redo.
- Selected-range page import into the active organiser with password-aware cancellable
  review through the shared scheduler, final source-fingerprint validation, certificate
  acknowledgement, source-aware thumbnails and previews, text search, drag-and-drop
  ordering, and single-step undo/redo insertion.
- Verified multi-source organiser export that safely renumbers imported objects,
  preserves source files, supports repeated pages and blank pages, and reopens the
  completed PDF before publication.
- Self-contained Rust page export with native file selection, atomic non-overwriting
  publication, output reopening, page-tree verification, and document-risk warnings.
- Ordered multi-source PDF combination with per-file passwords, selected ranges,
  odd/even and reverse selection, repeated-page support, and verified output.
- Merge/import interface with source reordering, per-source ranges, password fields,
  and native non-overwriting destination selection.
- Optional AES-256 Merge output applied only after prepared page-tree verification,
  followed by decryption and a second structural check, final multi-source fingerprint
  validation, encrypted-result reporting, shared controls, and content-free retained
  failures. Resolved bookmarks for selected pages are rebuilt and verified exactly;
  AcroForm catalogues remain explicitly unmerged.
- Multi-part split and extraction with semicolon-separated groups, deterministic
  filenames, prepare-before-publish verification, final source-fingerprint validation,
  content-free retained failures, partial-output rollback, and optional AES-256 output
  that prepares, decrypts, and verifies every protected part before any publication.
- Bounded 100-step undo/redo for standalone Merge source additions, removals, ordering,
  and page ranges and Split page-group edits, with keyboard-accessible controls,
  password-stripped Merge snapshots, and per-source Split history resets.
- Local document health report covering encryption, signatures, forms, XFA,
  JavaScript, actions, attachments, metadata, page geometry, large images, and
  decompression-bounded page inspection.
- Bounded technical-integrity diagnostics for dangling indirect references, excessive
  object nesting, strict page and nested Form XObject content decoding, missing named
  resources, nested font embedding and Unicode maps, output intents, bounded binary ICC
  header/tag-table validation, unmanaged Device CMYK, Form resource cycles and inherited
  resources, with grouped findings, a compact technical summary, 32-level/100,000-context
  traversal bounds, and explicit report truncation.
- Local accessibility preflight covering Title and DisplayDocTitle, catalogue language,
  MarkInfo and StructTreeRoot consistency, semantic structure counts, RoleMap-aware
  Figure alternative text, page structure links, and interactive tab-order warnings.
- Review-only likely blank-page and duplicate-page detection using page content,
  image resources, annotations, fonts, and page geometry.
- Lightweight post-open edit-safety preflight for certificate signatures, AcroForm,
  and XFA structures, with page-plan controls held until the check completes and a
  persistent pre-edit signature-invalidating warning.
- Shared, stale-request-safe certificate preflights for Merge, Split, Protect, and
  Privacy Cleaner, with password-aware errors, explicit risk acknowledgement, and
  backend rejection before publication when acknowledgement is absent.
- Verified privacy-clean export for selected document information, XMP, identifiers,
  private application history, JavaScript, automatic and launch actions, embedded
  files, file specifications, annotations, form fields, and page thumbnails.
- Bounded, fingerprinted privacy inspection for optional-content groups and default-off
  usage, invisible text modes, zero-opacity graphics states, hidden annotations,
  non-empty cropped pages, Web Capture URL and digital-ID structures, embedded PDX
  indexes, and declared extension, page-piece, or XX-prefixed private data.
- Finding-linked cleaner controls, source size/time enforcement, bounded object-stream
  loading, and verified removal of Web Capture `SpiderInfo`, `URLS`, and `IDS` data with
  the metadata category. Ambiguous layer, extension, and concealed-artwork content
  remains review-only rather than being deleted automatically.
- Full PDF rewrite, unreachable-object pruning, category-by-category reopening
  checks, source-overwrite protection, and encryption or certificate warnings for
  privacy-clean jobs.
- Preservation-first Compress PDF workflow with a 40-95 compatible-image quality
  control, decoded representative before/after comparison, exact dry-run rewritten
  size, saving estimate, and explicit reduced, non-recompressed, and unsupported-image counts.
- Bounded DeviceRGB and DeviceGray image recompression plus structural stream and
  object optimisation, with masks and specialist colour spaces excluded from JPEG
  recompression. The smaller create-new output is reopened and checked for pages,
  forms, and bookmarks before publication, with encrypted-input and
  certificate-signature warnings.
- Shared bounded FIFO native jobs for compression and privacy cleaning, with typed
  secret-free snapshots, two workers, bounded queue/history retention, image- and
  object-level stage progress, cancellation before atomic publication, retrying
  frontend status polls, and same-process reattachment by opaque job identifier.
- Consistent failed/cancelled-job controls across scan/OCR and every typed PDF workflow,
  with fresh setup retries that never retain old requests and selectable,
  clipboard-assisted diagnostics built only from allow-listed public snapshot fields.
  Typed results, paths within them, passwords, and document-bearing request data are
  excluded from diagnostic text.
- Scan/OCR creation now uses that same scheduler through its generic typed command
  lifecycle, removing the duplicate scan-only worker manager and later compatibility
  wrappers while retaining page-level progress, cancellation, verification, recovery,
  diagnostics, and the existing graphical workflow contract.
- Standalone merge and split now run as typed shared jobs with bounded request
  validation, monotonic source/page/part progress, queued and running cancellation,
  a final cancellation gate before create-new publication, retrying status polls,
  same-process reattachment, and itemised verified output reports.
- Main organiser export now runs as a typed shared job with source/page/signature and
  reopening progress, queued and running cancellation, same-process reattachment, and
  no direct WebView export command. Primary and imported PDFs carry their opening size
  and modification time into native work and are checked at start and again after
  output verification, immediately before create-new publication.
- Reusable local Batch Recipes for up to fifty fingerprinted PDFs and 20 GiB of source
  data, with built-in privacy-clean and compression combinations, per-source passwords,
  bounded certificate preflight, settings-only custom recipe storage, safe unique output
  names, sequential progress, cancellation, and itemised graphical results.
- Native batch execution through the shared FIFO scheduler, with an isolated output-side
  workspace, prepare-all-before-publish verification, create-new destinations, rollback
  of files published by an incomplete set, source-change rejection, and no publication
  after cancellation. PDF/A is now a verified final recipe step; connected-scanner
  intake is not yet available.
- Optional shared AES-256 Batch Recipe output passwords with QPDF-gated graphical
  controls, session-only secret handling, content-free retained failures, and delegated
  decryption plus repeated privacy or compression verification for every workspace copy
  before final source checks and all-or-nothing publication.
- Local two-document comparison with independent password handling, progressive
  64 KiB range loading, page-count and geometry checks, bounded Unicode word metrics,
  first-difference excerpts, added and removed page detection, changed-page filtering,
  and cancellable analysis progress.
- Bounded selected-page visual comparison with side-by-side local renders, adjustable
  pixel tolerance, and a colour-coded map for additions, removals, and other changes.
  Comparison text, page rasters, and passwords remain memory-only and are released
  when the comparison workspace closes.
- Bounded bookmark-tree inspection and create-new export preserving Unicode titles,
  hierarchy, page targets, bold/italic flags, colour, and expansion state, with
  branch-aware editing, source-fingerprint checks, certificate acknowledgement, form
  preservation, and exact bookmark-tree reopening verification.
- Bookmark publication now runs as a typed shared job with secret-free snapshots,
  monotonic stage progress, queued or running cancellation, status retry, and
  same-process reattachment. It checks the reviewed source fingerprint at worker start
  and immediately before publication, removes the direct WebView export command, and
  can apply optional QPDF AES-256 opening and administrator passwords before a second
  decrypted bookmark/form verification pass.
- Review-only heading suggestions derived locally from streamed PDF.js text, font-size
  tiers, numbered-heading depth, repeated running-header filtering, confidence scores,
  progress and cancellation. Suggestions replace the draft only after explicit selection.
- Graphical annotation workspace for text boxes, highlights, preset stamps, freehand
  ink, rectangles, ellipses, lines, and embedded images, with drag creation and movement,
  page navigation, selection, colour, opacity, fill, width and font controls, duplication,
  deletion, keyboard actions, and a bounded 100-step undo/redo history.
- Standard PDF `FreeText`, `Highlight`, `Stamp`, `Ink`, `Square`, `Circle`, and `Line`
  export with generated appearance streams, inherited page-box and rotation mapping,
  bounded image and point data, existing annotation and form preservation, source
  fingerprinting, certificate acknowledgement, create-new publication, and exact marker
  and subtype verification after reopening.
- Annotation publication now runs as a typed shared job with content-free public
  snapshots, bounded validation and appearance progress, queued or running cancellation,
  status retry, same-process reattachment, and a final reviewed-source fingerprint gate.
  Optional QPDF AES-256 output is decrypted for a second verification of page and form
  structure, annotation counts, stable markers, subtypes, appearances, and embedded-image
  resources; the direct WebView annotation-export command has been removed.
- Graphical AcroForm workspace with searchable and filterable fields, page-linked widget
  overlays, typed text, checkbox, radio, choice, password, and multi-select controls,
  required-field validation, keyboard navigation, reset actions, and 100-step undo/redo.
- Bounded native form inspection and export with hierarchical names, inherited field
  properties, rotation-aware geometry, exact UTF-16 values, generated text and button
  appearances, optional supported-field flattening, XFA rejection, signed-source and
  changed-source guards, direct-annotation preservation, and create-new reopening checks.
- Form publication now runs as a typed shared job with content-free public snapshots,
  monotonic field and appearance progress, queued or running cancellation, status retry,
  same-process reattachment, and a final reviewed-source fingerprint gate. Optional
  QPDF AES-256 output is decrypted for a second verification using stable field names,
  exact values, remaining-field counts, appearances, and flattened-content markers;
  the direct WebView form-export command has been removed.
- Graphical Page Finish workspace with all/current/custom page selection, visual-edge
  crop margins, proportional A3/A4/A5/Letter/Legal/custom paper fitting, portrait and
  landscape controls, live page placement, output dimensions, and selected-page status.
- Rotation-aware watermark, header, footer, and Bates-number layers with page and file
  tokens, colour, size, opacity, angle, alignment, layer, margin, prefix, suffix, starting
  number, padding, and position controls, plus source fingerprints, certificate guards,
  transformed annotation and form-widget coordinates, and exact reopening verification.
- Page Finish publication now runs as a typed shared job with content-free public
  snapshots, bounded page and verification progress, queued or running cancellation,
  status retry, same-process reattachment, and a final reviewed-source fingerprint gate.
  Optional QPDF AES-256 output is decrypted for a second check of page boxes, forms,
  bookmarks, annotation counts, operation markers, and mark layers; the direct WebView
  finishing-export command has been removed.
- Graphical permanent-redaction workspace with manual draw and move tools, black or
  white fills, page navigation, bounded undo/redo, local text and name matching,
  email-address discovery, safe wildcard patterns, selectable suggestions, raster
  quality controls, destructive-effect acknowledgement, and visible export progress.
- Bounded native redaction export that replaces every marked page with one lossless
  image-only page, burns committed regions into the raster, strips metadata, actions,
  attachments, annotations, forms, bookmarks, names, thumbnails, optional-content
  catalogue data, and tagged-document structures, prunes unreachable objects, and
  verifies source fingerprints, certificate acknowledgement, page geometry, exact
  markers, image resources, raster lengths, empty selected-page text, privacy residue,
  page count, unlocked output, and create-new publication after reopening.
- Permanent-redaction publication now runs as a typed shared job with bounded queue
  admission, secret-free public snapshots, monotonic page/pixel/verification progress,
  queued and running cancellation, retrying status polls, and same-process reattachment.
  The reviewed source fingerprint is checked when work starts and after output
  verification immediately before create-new publication; passwords, source paths,
  redaction regions, and page rasters remain only in transient native request memory.
- Versioned, size-bounded local recovery drafts for PDF page plans and ordered scan
  sessions, with automatic and manual saving, startup Continue/Discard controls,
  three create-new snapshot generations, and fallback from interrupted writes.
- Strict recovery schemas that exclude passwords, signature images, document text,
  and document bytes, while restoring page operations, scan settings, workflow,
  selection, and zoom.
- Recovery of unencrypted imported PDF sources and page identities, with older draft
  compatibility and deliberate draft suppression when an imported password is in use.
- Debounced standalone Merge and Split recovery through the same rotating snapshot
  store, preserving bounded source order, ranges, and page-group text while rejecting
  unknown or secret-bearing fields and rechecking source-file presence. Passwords,
  certificate acknowledgements, output protection, undo stacks, and jobs remain
  session-only.
- Self-contained multi-image PDF creation with paper and card presets, EXIF
  auto-orientation, fitted placement, DPI controls, margins, and verified output.
- Embedded PNG, JPEG, TIFF, WebP, BMP, and GIF decoding with bounded allocations,
  white transparency flattening, ImageMagick fallback, and source-file protection.
- Colour, greyscale, and thresholded monochrome scan modes with JPEG quality control.
- Pure-Rust scan clean-up with confidence-gated page-edge detection, automatic crop,
  projective perspective correction, local illumination normalisation, cancellation
  checkpoints, and per-export applied-page counts.
- Debounced selected-page before/after previews using the same bounded decode and
  clean-up pipeline as PDF export, including shared-job progress, cancellation, safe
  retry and reattachment, stale-result rejection, and object-URL clean-up.
- Recovery-safe clean-up settings with explicit backwards-compatible defaults.
- Optional OCRmyPDF searchable output, deskewing, and installed Tesseract language
  discovery with a local language selector.
- Live OCRmyPDF engine percentages through an embedded one-use progress plug-in and
  bounded stderr parser, with machine, Rich, and tqdm format support, monotonic mapping
  into overall scan progress 76–90%, prompt cancellation, and crash-safe plug-in
  clean-up.
- Bounded OCR readiness preflight with distinct OCRmyPDF, Tesseract, language-pack,
  timeout, and command-failure diagnostics before scan images are decoded.
- Selected-page OCR confidence review over the cleaned export raster, strict bounded
  Tesseract TSV parsing, low-confidence word overlays, side-by-side corrections,
  and temporary user-word hints for the final OCR pass.
- Page-level searchable-text verification counts and page-specific review warnings
  after the OCR output has been reopened.
- Optional QPDF AES-256 scan/OCR output applied only after the prepared image or
  searchable candidate passes, followed by decryption and repeated page, embedded-image,
  and searchable-text coverage checks. Source images are fingerprinted again before
  publication, public snapshots omit paths, OCR hints, and passwords, and retained
  failures are content-free.
- An ignored engine-backed English, Turkish, rotated, and noisy OCR corpus gate with
  documented fixture contracts and token-recall thresholds.
- Bounded native scan/OCR jobs with typed queued, running, succeeded, failed, and
  cancelled snapshots, monotonic stage progress, automatic status retry, and
  same-process frontend reattachment.
- Cancellation-aware image preparation, ImageMagick, and OCRmyPDF execution with
  bounded child-process diagnostics and publication only after output verification.
- Shared connected-scanner discovery and capture contracts with validated requests,
  bounded app-owned capture directories, seven-day stale-session clean-up, Windows
  WIA acquisition, and Linux SANE `scanimage` acquisition.
- Capability-aware scanner controls for device, flatbed or feeder source, duplex,
  page limit, paper size, DPI, and colour mode, with captured pages loaded directly
  into the existing review, OCR, and verified PDF-export workflow.
- Packaged macOS ImageCaptureCore scanner bridge with asynchronous device discovery,
  functional-unit capability inspection, flatbed and feeder capture, duplex controls,
  page-limit cancellation, deterministic file transfer, and strict versioned JSON IPC.
- Intel, Apple Silicon, and universal macOS scanner-helper builds generated from source
  by npm hooks, bundled as a Tauri external binary, and architecture-verified in CI.
- Ignored private connected-scanner release harness and documented Windows WIA, macOS
  Image Capture, and Linux SANE physical-device matrix. Hardware evidence remains an
  explicit release gate.
- Responsive mobile editor ordering and browser-based desktop/mobile visual checks.
- Dedicated PDF.js production chunk so the application shell remains below the
  bundler's large-chunk advisory threshold.
- Windows MSI and NSIS packaging.
- GitHub CI and tagged cross-platform release workflows, including a universal macOS
  application target for Intel and Apple Silicon.
- Native Rust regression tests on Windows, macOS, and Linux CI.

### Fixed

- Xcode 26-compatible universal macOS scanner verification now places the input before
  `lipo -verify_arch`, and CI uploads native end-to-end evidence only when an earlier
  stage produced it. Native desktop tests now request their evidence viewport explicitly
  and record the rendered DOM size, avoiding WebKitGTK's unsupported zero window-rectangle
  response on headless Linux. The first hosted iPhone/iPad simulator run passed its
  complete compile, bundle-metadata, archive, hash, and evidence-upload gate.
- The process-tree cancellation regression now samples a stable optional heartbeat after
  termination. This preserves descendant-leak detection while accepting the valid empty
  test file left when macOS interrupts the heartbeat's truncate-and-write operation.
- Parallel Rust tests now allocate private temporary directories through one atomic,
  collision-resistant helper. This removes timestamp-resolution races observed on hosted
  macOS runners without weakening per-test clean-up or document isolation.
- Native PDF text layers now publish their completed streaming text into the retryable
  per-source-page search cache. Evidence waits for the visible render before searching,
  avoiding the second PDF.js stream rejected by WKWebView while retaining the final
  localised failure state in hosted diagnostics.
- Native PDF workspaces now keep their clear-job callback stable while inspection and
  publication snapshots change, preventing source-bound dialogs from closing during a
  review. The bookmark workspace also gives expanded printed-contents controls
  intrinsic grid rows, so they remain visible and cannot overlap output protection.
- The transitive native lockfile now resolves `anyhow` 1.0.103, clearing
  `RUSTSEC-2026-0190` before the release snapshot is archived.
- Windows veraPDF discovery now converts canonical `\\?\` batch-file paths to the
  ordinary absolute form required by `cmd.exe`, preserves UNC and Unicode paths, and
  allows a bounded 30-second cold JVM readiness start without weakening the faster
  native-engine probe limit.
- PDF/A conversion without a new OCR pass now preserves pages that already contain a
  searchable text layer, allowing OCR-plus-PDF/A Batch Recipes to complete and retain
  their verified text coverage.
- Clean npm installs now resolve PostCSS 8.5.25 instead of the vulnerable 8.5.16
  lockfile entry. The production dependency audit remains clear; the remaining 24
  development-only advisories are confined to the pinned WebDriver/Mocha toolchain.
