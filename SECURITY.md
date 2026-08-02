# Security

PDF files can contain sensitive data and hostile input. Treat every PDF as
untrusted.

The supported security claims, experimental workflows, and unavailable capabilities are
summarised in [feature status](docs/FEATURE_STATUS.md). That document is part of the
release contract and must be updated when a boundary changes.

## Rules

- Do not upload user PDFs to external services by default.
- Keep the public rendering corpus synthetic and generated from repository source.
  Bound fixture count, paths, file sizes, total bytes, page counts, sampled canvas
  dimensions, expected text, and report fields. Never substitute personal, customer,
  certificate-bearing, or otherwise private documents in public CI.
- Keep the public OCR corpus synthetic and generated from the bundled standard font.
  Bound filenames, byte counts, hashes, decoded dimensions, language and recall
  contracts, expected-text encoding, and sampled pixels. Engine reports may contain
  versions, fixture digests, recall values, searchable-page counts, and progress
  outcomes, but never recognised text, temporary paths, or user document content.
- Reuse only that synthetic image-only fixture for public PDF/A engine evidence.
  Retain profile identifiers, page and searchable-page counts, pass/fail totals,
  validator and conversion engine versions, and the fixture-manifest digest only.
  Reject unknown report fields and never retain PDF bytes, recognised text, passwords,
  local paths, raw veraPDF output, or rule examples copied from a user document.
- Do not execute scripts embedded in PDFs.
- Keep embedded links and form controls inert in display-only previews. Any future
  interactive form or link workflow must require an explicit, reviewed user action.
- Keep XFA rendering disabled unless it is reviewed and sandboxed explicitly.
- Do not overwrite source documents in place.
- Validate desktop PDF range requests against a canonical PDF path, the opening file
  size and modification time, strict byte boundaries, and a bounded response size.
  Stop loading and require a reopen when those file checks change.
- Treat the registered Tauri command list as a security boundary. Route every native
  structural publication and validation operation through the generic bounded job
  scheduler. Keep direct IPC limited to reviewed bounded byte transport, capability and
  readiness probes, recovery, audit, signature-vault, scanner-discovery, and aggregate
  status calls, and fail the static allow-list regression when that surface changes.
- Treat `runtime_capabilities` as a closed presentation contract, not client-side
  authorisation. On mobile, recheck the compiled capability policy inside Rust before
  queuing work and reject every request that requires a desktop subprocess or connected
  scanner. Do not probe, launch, or emulate those engines from the WebView.
- Do not present overlay-based redaction as secure redaction.
- Do not present privacy cleaning as redaction, antivirus scanning, or proof that
  no visually concealed content remains.
- Present Document Health accessibility findings as static preflight guidance, not
  WCAG or PDF/UA certification. The separate PDF Standards validator may report
  PDF/UA conformance, but correct semantics and reading order still require
  assistive-technology testing of the finished document.
- Present the application shell's automated keyboard and modal-focus contracts as a
  regression baseline, not proof of WCAG conformance. Release candidates still require
  the packaged keyboard-only and assistive-technology matrix in
  [accessibility testing](docs/ACCESSIBILITY_TESTING.md), using non-sensitive fixtures.
- Compile WebDriver plugins, capabilities, global Tauri access, boot diagnostics, and
  one-shot chooser control only into the dedicated E2E application identity. Refuse an
  identifier mismatch, bind the embedded driver to loopback, keep evidence path-free,
  and fail production builds containing any E2E bridge marker. Use only synthetic
  repository-generated or in-memory fixtures, as described in
  [native end-to-end testing](docs/E2E_TESTING.md).
- Bound Document Health to 20,000 pages, one million indirect objects, two million
  object references, 64 direct nesting levels, 20,000 unique font resources, 32 MiB
  per decoded page or Form XObject content stream, 16 MiB per decoded ICC profile,
  4,096 ICC tags, 32 nested Form XObject levels, 100,000 page-specific form contexts,
  64 MiB of object-stream decompression, and 2,000 returned findings. Reserve the final
  finding slot to disclose truncation.
- Run Document Health through the shared bounded scheduler. Keep its source path and
  password only in transient request memory, exclude both from queued public snapshots
  and process-restart records, retain only a content-free terminal error, honour
  cancellation throughout object/page/resource traversals, and compare the source size
  plus exact modification time again before returning the typed report. The completed
  report remains local in volatile job state and is deliberately excluded from copyable
  diagnostics and the Activity audit.
- Run certificate validation through its own shared read-only job kind. Keep the PDF
  and up to 16 trust-root paths only in transient request memory; exclude them from
  queued snapshots and process-restart records; retain only path-scrubbed reports and
  content-free failures; and exclude typed reports from copyable diagnostics and the
  Activity audit. Bound recursive signature-structure inspection to 64 direct levels
  and two million visited nodes, bound pyHanko output and runtime, terminate its process
  tree on cancellation, and compare exact PDF and trust-root size/modification-time
  fingerprints immediately before report delivery.
- Treat Document Health as structural guidance rather than a PDF standards, colour,
  font, or malware verdict. ICC checks validate bounded binary
  headers, declared size, class, colour-space/channel agreement, PCS, date, rendering
  intent, reserved bytes, and tag ranges; they do not prove colourimetric correctness
  or required-tag conformance. Form XObject inspection is cycle-safe and bounded, so
  a disclosed limit finding means remaining branches need a specialist validator.
- Treat PDF Standards as a separate typed workflow. Accept only explicit PDF/A-1b,
  PDF/A-2b, or PDF/A-3b profiles; require unencrypted output, unchanged page count, a
  matching-profile independent veraPDF verdict, and an unchanged source fingerprint
  before create-new publication. PDF/A conversion rewrites the document and invalidates
  existing certificate signatures, so require the shared signature-risk acknowledgement.
  PDF/A validation is not PDF/UA, PDF/X, accessibility, malware, legal, or institutional
  retention-policy certification.
- PDF/UA-1 and PDF/UA-2 are validation-only and must select veraPDF's exact `ua1` or
  `ua2` flavour. Do not validate a privately decrypted rewrite as though it were the
  exact protected source. A PDF/UA pass does not repair tags or replace semantic,
  reading-order, keyboard, and assistive-technology testing.
- PDF/X-1a:2001, PDF/X-3:2002, and PDF/X-4 results are bounded structural preflight,
  never conformance certification. Keep `preflight-passed` distinct from `conforms` in
  native and interface types, disclose omitted profile-specific colour and transparency
  semantics, and require an independent specialist validator before print approval.
- For validation of an encrypted source, decrypt only into the ownership-token-protected
  private workspace, validate that copy, and force the original verdict to
  non-conforming because PDF/A forbids encryption. Never send a password through the
  veraPDF command line. Bound validator output to 4 MiB, JSON traversal to 200,000 nodes,
  returned failed rules to fifty, and each retained rule field to 320 bytes.
- Bound Privacy Inspection and Privacy Cleaner to 20,000 pages, one million indirect
  objects, 1,024 password bytes, and 64 MiB of object-stream decompression. Bound the
  inspection further to two million direct object nodes, 64 nesting levels, 32 MiB per
  decoded page-content stream, and 256 typed findings.
- Run Privacy Inspection through its own shared read-only job kind. Keep the source path
  and password only in transient request memory; exclude them from queued snapshots and
  restart records; retain only a path-free typed report or content-free failure; and
  exclude that report from copyable diagnostics and Activity entries. Honour
  cancellation throughout direct-object, optional-content, resource, annotation, and
  content-operator traversal, then compare exact source size and modification time
  before report delivery.
- Require a successful privacy inspection and its exact source size and modification
  time before cleaning. Reject a source that changed after review or during processing,
  never overwrite it, and publish only after reopening and verifying every selected
  removal category. For protected output, apply cancellable QPDF AES-256 after prepared
  verification, decrypt the candidate, repeat the checks, and then perform the final
  source-fingerprint gate.
- Treat hidden-layer, invisible-text, zero-opacity, hidden-annotation, and crop-box
  findings as review signals. Invisible text commonly belongs to OCR; optional-content
  membership policies, alternate configurations, Form XObject operations, colour
  matching, clipping, and binary private payloads are not evaluated completely.
- Never remove only `OCProperties`, declared `Extensions`, or unknown private keys:
  doing so can expose hidden artwork or break extension-dependent content. Metadata
  cleaning may remove page-piece history and Web Capture `SpiderInfo`, `URLS`, and
  `IDS`; attachment cleaning may remove an embedded PDX index.
- Keep temporary files isolated and delete them after jobs complete. Register app-owned
  PDF candidates, batch workspaces, certificate passfiles, OCR hint files, and scan
  rasters in the private app-data lease registry before they can be created. Hold an
  exclusive operating-system lock for the complete lifetime of each artefact.
- At start-up, inspect a bounded number of strict, size-limited lease records. Remove a
  target only when its lease is unlocked, its canonical parent and exact app-owned name
  still match, and its type is an ordinary file or an ownership-token-matched directory.
  Reject symbolic links, Windows reparse points, unknown record fields, changed tokens,
  and malformed or future-dated records. Expose only aggregate clean-up counts.
- Connected-scanner captures are an explicit exception to immediate temporary clean-up:
  they remain in a bounded app-owned session directory for recovery and are pruned
  after seven days.
- Log diagnostics without leaking document text unless the user requests it.
- Keep PDF passwords in memory only and redact them from engine errors.
- Do not place passwords in child-process command lines or persistent job history.
- Keep shared scan/OCR, standalone searchable OCR, compression, privacy inspection and
  cleaning, batch inspection and publication, merge,
  split, organiser, page-import inspection, bookmark inspection and publication,
  annotation inspection and publication, page-finishing inspection and publication,
  form inspection and publication, certificate-signing,
  permanent-redaction inspection and publication, and password-protection job snapshots free of passwords,
  source paths, document text,
  document bytes, signature bytes, redaction regions, and image bytes. Requests may
  wait only in bounded process memory; remove the
  secret-bearing request when it is cancelled or handed to a worker. Retained terminal
  results may contain published output paths and operation counts. A cancelled job must
  not publish temporary output.
- Never implement Retry by retaining or replaying a secret-bearing job request. Rebuild
  it from the current reviewed interface state and repeat destination selection.
  Construct copyable diagnostics from an explicit allow-list of public snapshot fields;
  exclude request data and the complete typed result even when the result contains a
  published path.
- Represent active jobs across process restarts with a separate private lock-backed
  journal. Persist only schema version, opaque entry ID, workflow kind, and start time.
  Never persist the public job ID, request, source or destination path, filename,
  password, passphrase, OCR hint, signature or certificate data, raster, document
  content, stage, progress, error, warning, or result. Bound records to 8 KiB, scan at
  most 512 directory entries, and restore at most the newest 32 valid stale entries.
- Hold one exclusive operating-system lock for each journal entry. Skip locks owned by
  another live application instance; reject links, Windows reparse points, unknown
  fields, malformed or mismatched identifiers, oversized records, and future times.
  Remove valid stale entries before exposing a one-time interrupted snapshot.
- Never report that an interrupted job definitely did or did not publish its output:
  the process may have stopped after create-new publication but before terminal-state
  reporting. Tell publication users to check the chosen destination, then rebuild a
  fresh request from current or separately recovered interface state. Read-only jobs
  must simply rerun from current inputs.
- Keep operation audit entries separate from public job snapshots and typed results.
  Persist only an opaque audit-entry ID, workflow kind, terminal outcome, start and
  completion times, and duration. Never persist job IDs, stages, progress, errors,
  warnings, filenames, source or destination paths, passwords, document text or bytes,
  signature data, redaction data, OCR hints, or typed results.
- Bound operation history to 500 entries and 512 KiB per snapshot. Serialise writers
  across application processes with an operating-system lock, keep three create-new
  generations for interrupted-write fallback, reject unknown fields and invalid or
  future times, and use user-only directory and file modes on Unix. JSON export must
  create a new file; confirmed clearing must remove the older snapshot generations.
- Expose scan/OCR creation, standalone searchable OCR, compression export, privacy
  inspection and cleaning, batch inspection and publication,
  merge, split, organiser export, page-import inspection, bookmark inspection and
  publication, annotation inspection and publication, page-content inspection and
  publication,
  page-finishing inspection and publication, form inspection and publication,
  permanent-redaction inspection and publication, edit-safety inspection, and password
  protection to the WebView only through the bounded job commands. Keep synchronous
  Rust entry points for internal tests and worker dispatch, not as separately registered
  Tauri commands.
- Do not register parallel direct Tauri wrappers for scheduler-backed scan export or
  standalone searchable OCR,
  certificate validation, compression preview, Document Health, scan clean-up preview,
  OCR confidence review, scanner capture, or edit-safety inspection. Scan export uses
  only the generic start, get, list, and cancellation job commands. Keep synchronous
  helpers internal to backend enforcement or native tests where required.
  Capability/readiness probes, bounded local file reads, and scanner discovery remain
  separate synchronous boundaries.
- Run edit-safety review through one aggregate read-only job for at most 250 sources.
  Keep every path and password only in the transient request; cap each password at
  1,024 UTF-8 bytes, decompressed object streams at 64 MiB, parsed objects at 1,000,000,
  and readable pages at 100,000 per PDF. Map staged progress across sources, honour
  cancellation during structural traversal, scope same-kind reattachment separately
  for every consuming workflow, cancel stale source selections by exact job ID before
  replacement, return only ordered path-free results or content-free per-source
  failures, and recheck size and modification time before delivering each successful
  result.
- Bound every QPDF password to 127 UTF-8 bytes, reject line breaks and nulls, and send
  QPDF arguments through standard input rather than the process command line. Drain
  standard output and error concurrently with a 1 MiB retained diagnostic bound, stop
  work after 30 minutes, redact passwords and paths from errors, terminate the complete
  platform process tree when cancellation is requested, and await the immediate child.
- Keep an exhaustive protection policy beside the shared job-kind contract. Every
  structural publisher must be classified as optional AES-256 output, source-encryption
  preservation, password-aware unencrypted archival output, or explicit protection
  management before it can pass the release tests. Bind optional protection to native
  password validation, cancellable QPDF publication and password-aware reopening.
  Treat image and scanner intake and read-only inspections as non-publishing classes.
- Bind password addition and removal to the edit-safety source size and modification
  time. Check that fingerprint before QPDF work and after output verification,
  immediately before the final cancellation gate and create-new publication.
- Bound organiser exports to 249 imported PDFs and 50,000 output pages, each source
  password to 1,024 UTF-8 bytes, prepared signature PNG data to 16 MiB decoded and
  8,192 pixels per source dimension, and password-lock values to QPDF's 127-byte limit.
  Check every primary and imported source against its opening size and modification
  time before reading and again after output verification, immediately before the
  final cancellation gate and create-new publication.
- Bound page-import review to one canonical PDF, a 4,096-character range expression,
  50,000 selected pages, a 1,024-byte opening password, and 64 MiB of object-stream
  decompression. Keep its source, password, and range only in transient request memory;
  exclude the complete request from queued snapshots, recovery records, diagnostics,
  and Activity entries; honour cancellation during certificate-structure traversal;
  retain only content-free failures; and compare exact source size and modification time
  immediately before delivering the typed selected-page report.
- Bound merge to 250 sources and 50,000 output pages, split to 250 output groups,
  every page-range expression to 4,096 characters, and each source password to 1,024
  UTF-8 bytes. Reject existing destinations and source/destination collisions. Prepare
  and verify every split part before the final cancellation and publication stage.
  Fingerprint every canonical source when copying starts and recheck every fingerprint
  before publishing merge or split output. Retained failures must not expose paths,
  passwords, or parsed document details.
- Apply optional merge protection only after the unencrypted combined page tree passes.
  Bound and distinguish both output passwords, run QPDF through cancellable control,
  require AES-256 encryption, decrypt the candidate, repeat page-tree and fresh-catalogue
  checks, then perform the final multi-source fingerprint gate. Do not claim that source
  bookmark trees or AcroForm catalogues are merged.
- For protected Split output, keep every plain and protected part temporary until the
  complete set passes. Apply the same bounded distinct passwords to each part, require
  encryption, decrypt and repeat structural checks for every part, recheck the source,
  and only then enter the rollback-capable publication loop.
- Keep standalone Merge and Split undo/redo histories in memory and bound them to 100
  operations. Strip every Merge source password from past and future snapshots, never
  record password edits as operations, and clear the live password when undo or redo
  restores a sanitised source state. Split history may contain only page-group text.
- Bound batch recipes to 50 unique canonical PDF sources, 20 GiB total source data,
  1,024 password bytes per source, 240 UTF-8 bytes per plain `.pdf` output filename,
  100,000 pages per source, unique source and destination paths, and an existing output
  directory. Recheck each inspected size, modification time, password-aware page count,
  and source fingerprint before work and again before publication.
- Run Batch Recipe source review through its own shared read-only job kind. Accept at
  most fifty unique canonical sources, keep every source path and password only in the
  transient aggregate request, and exclude that request from public snapshots, restart
  records, diagnostics, and Activity entries. Map bounded Privacy Inspection progress
  and cancellation across the source set, retain only ordered path-free reports or
  content-free per-file failures, and require each successful report to pass its final
  source fingerprint before delivery.
- Store only bounded recipe names and processing settings in local storage. Never save
  batch source paths, passwords, findings, output folders, or job requests in a recipe.
- Permit scanner or image-scan intake only after the existing scan workflow publishes
  and reopens a verified create-new PDF. Keep the hand-off in volatile interface state
  with exactly the verified output path and a generic origin. Do not carry a device
  identifier, capture path list, image bytes, scan settings, OCR hints, or password into
  Batch Recipes. Require any opening password to be entered again before inspection.
- Prepare and verify every batch output in an isolated output-side workspace before
  publication. Reject existing destinations, check cancellation before publication,
  and remove files created by the current batch if the complete set cannot be published.
- Run batch OCR before privacy cleaning and compression. Validate the language, require
  OCRmyPDF and Tesseract readiness, and keep recognised text out of retained job state.
  Decrypt a protected OCR source only into the isolated workspace through QPDF, preserve
  the inspected page count, reopen and inspect searchable text layers, and report bounded
  page numbers when searchable coverage is incomplete.
- Route standalone existing-PDF OCR through that same bounded engine and verification
  path. Require a strict one-source, one-destination request; reject source overwrite;
  validate the language and passwords; enforce certificate acknowledgement; recheck
  the source before publication; and verify page count plus searchable-text coverage
  before and after optional AES-256 protection. Keep queued paths and passwords
  transient, map retained failures to content-free diagnostics, and never add a
  cancellable checkpoint after create-new publication.
- Keep optional batch output passwords shared across the run but session-only. Apply
  QPDF AES-256 protection once to each final prepared candidate, decrypt it in memory
  to repeat page-count and searchable-layer comparisons, then recheck every original
  source before publishing any member of the complete set.
  Retained batch errors must distinguish protection from source-password failures
  without exposing paths, passwords, document content, or engine diagnostics.
- Bound OCR engine probes and confidence output, validate language codes and word
  boxes, and never persist or log recognised words from confidence review.
- Run OCR confidence review through its own shared read-only job kind. Keep the source
  path and clean-up settings in transient request memory; exclude them from queued and
  restart snapshots; retain only content-free failures; and keep recognised words only
  in the volatile typed result and visible review draft. Exclude that result from
  restart records, copyable diagnostics, and Activity entries. Honour cancellation
  before and after embedded decoding, throughout clean-up, and by terminating the
  complete ImageMagick or Tesseract process tree. Recheck the exact source-image size
  and modification time before report delivery.
- Emit OCRmyPDF progress through an app-supplied, one-use local plug-in containing no
  document data. Bound streamed records to 16 KiB, accept only the exact machine marker
  or guarded OCR phase formats, enforce internally consistent 0–100% values, and ignore
  duplicate, decreasing, malformed, unrelated, and oversized records. Do not retain
  progress records as engine diagnostics.
- Keep reviewed OCR corrections in memory only. Write recognition hints to a
  create-new temporary file, pass its path as a separate process argument, and delete
  it after OCR succeeds or fails. Describe hints as guidance, not guaranteed changes.
- Encrypt every stored visual signature, including its label, source name, dimensions,
  and PNG bytes, with authenticated AES-256-GCM. Derive a fresh key for each entry with
  Argon2id and a random salt; never store the passphrase or derived key.
- Expose only opaque identifiers, modification times, and encrypted file sizes while
  the signature library is locked. Return decrypted pixels to the interface only after
  an explicit unlock, clear passphrase state after use, and require a separate
  confirmation before deleting an encrypted copy.
- Treat a lost signature-library passphrase as unrecoverable. Frontend JavaScript
  strings cannot be guaranteed to be erased from memory immediately, so minimise their
  lifetime and never write them to logs, recovery drafts, or persistent settings.
- Keep PKCS#12 passphrases out of process arguments, logs, recovery data, and persistent
  settings. Confirm them in the interface, pass them to pyHanko through a create-new
  one-use temporary file, use user-only file modes on Unix, delete the file after the
  command, and clear the frontend passphrase state after every signing attempt.
- Keep source-PDF passwords for certificate signing and validation out of process
  arguments, environment variables, logs, diagnostics, recovery records, and persistent
  settings. Validate and hold them only in transient memory, send one bounded UTF-8 line
  through pyHanko's standard input, and patch its password prompt only through a fixed
  app-owned `sitecustomize.py` in a private temporary directory. The bridge contains no
  secret, uses user-only modes on Unix, and is covered by ownership-token and lock-backed
  crash clean-up.
- Accept only bounded local `.p12` or `.pfx` identities and bounded PEM, CRT, CER, or
  DER trust roots. A selected trust root is security-sensitive because it can make
  signatures in its chain appear trusted.
- Permit remote timestamp services only over HTTPS. Reject timestamp URLs containing
  credentials, query strings, or fragments so secrets are not exposed through child
  process arguments. Plain HTTP is reserved for loopback release testing.
- Publish a certificate-signed copy only after it reopens, contains a new signature
  with a complete PDF byte range, and pyHanko confirms cryptographic integrity. Treat
  an intact but untrusted signature as indeterminate, never as proof of signer identity.
- Keep certificate operations incremental and create-new. Never overwrite the source.
  For an encrypted input, reopen the signed candidate with the supplied password and
  reject publication unless its encryption state matches the source before running
  post-signature validation.
- Run certificate publication through the bounded shared queue. Poll cancellation while
  pyHanko is active, terminate its complete platform process tree and await the immediate
  child before reporting cancellation, recheck the source fingerprint immediately before
  publication, and retain only content-free errors and sanitised results without paths,
  field contents, trust roots, timestamp details, or certificate diagnostics.
- Keep scan clean-up confidence-gated and non-destructive. If page edges cannot be
  detected reliably, retain the original framing and report that no geometric
  correction was applied.
- Bound clean-up previews to 900 pixels, discard stale frontend results, revoke their
  object URLs, and keep any ImageMagick normalisation raster temporary.
- Fingerprint each canonical scan source after validation and recheck its size and
  modification time immediately before publication. When scan protection is selected,
  apply cancellable QPDF AES-256 only to the verified image or OCR candidate, require
  encrypted output, decrypt it with a bounded loader, and repeat page, embedded-image,
  and searchable-text coverage checks. Keep scan passwords and OCR hints out of public
  snapshots, retained failures, logs, and recovery drafts.
- Register the one-use OCR progress plug-in in the same private crash-cleaned lease
  registry as OCR hint files, use create-new and user-only file modes, and remove it
  after success, failure, or cancellation.
- Validate scanner device identifiers and capability requests before acquisition.
  Invoke WIA through a fixed script with request values in the child environment, and
  invoke SANE with separate process arguments rather than constructing shell commands.
  Bound retained adapter output and discovery/capture runtime.
- Invoke the packaged macOS Image Capture helper by a verified, non-symlink executable
  beside the application, or from the generated source-build directory in debug builds.
  Send capture values through bounded, versioned JSON standard input rather than process
  arguments, bound helper output and runtime, and treat native device strings as untrusted.
- Own long-running external commands with a suspended Windows launch assigned to a
  kill-on-close Job Object before resume, or a separate macOS/Linux process group.
  Terminate the whole tree on cancellation, timeout, monitor failure, normal parent
  exit, or wrapper drop so descendants cannot retain temporary files, pipes, or scanner
  devices.
- Accept captured pages only when they are unique, non-empty, supported image files
  directly inside the allocated capture directory and no larger than 512 MB each.
- Require a completed certificate-signature preflight and explicit acknowledgement
  before any workflow rewrites a signed source; enforce the acknowledgement again
  in the trusted command layer before creating output.
- Bound PDF compression by image dimensions, decoded allocation, image count, total
  pixels, object-stream decompression, preview payload, password length, and JPEG
  quality. Recompress only direct DeviceRGB or DeviceGray 8-bit images without masks
  or non-default decode arrays; never JPEG-recompress unsupported image streams.
- Keep compression previews local and memory-only. Estimate output through a counting
  writer and run preview work through a distinct shared read-only job. Keep its source
  path and password in transient request memory; retain content-free failures and a
  path-free report filename; exclude sample images and typed results from restart
  records, copyable diagnostics, and Activity entries; honour cancellation throughout
  compatible-image processing; and recheck the exact source size/modification-time
  fingerprint before returning the volatile sample report. Publish only a verified
  create-new file that is smaller than the source, recheck the opening source
  fingerprint immediately before publication, and never imply that a compressed rewrite
  preserves certificate signatures.
- When compression output protection is selected, validate distinct bounded passwords,
  apply cancellable QPDF AES-256 only after the prepared smaller copy passes, require
  encrypted output, decrypt it in memory, repeat page-count, form, and bookmark checks,
  and reject a protected candidate that is no longer smaller than the source. Retained
  compression-job errors must not include paths, passwords, or document content.
- Compare PDFs through bounded local range requests. Limit each document to 2,000
  pages, each page to 100,000 text items, 500,000 extracted characters, and 50,000
  comparison tokens, and the selected visual map to two million pixels.
- Keep comparison passwords, extracted text, and raster buffers in session memory only.
  Cancel stale work, destroy both PDF.js loading tasks, and discard the buffers when
  the comparison workspace closes.
- Bound bookmark editing to 2,000 entries, seven hierarchy levels, 256 title characters,
  1,024 title bytes, 4,000 named-destination nodes, 16 destination dereferences, and
  64 MB of object-stream decompression. Reject cycles, missing page targets, invalid
  colours, skipped hierarchy levels, and sources whose size or modification time changed
  after review.
- Bound printed contents to a 128-character and 512-byte title, selected bookmark levels,
  38 entries per A4 page, and 64 generated pages. Embed the bundled font and Unicode map
  directly, replace unsupported printed glyphs visibly, and leave the original bookmark
  titles unchanged. Keep the title and options transient and exclude them from public
  queued and interrupted snapshots.
- Run bookmark source inspection through a distinct shared read-only job. Keep its path
  and password transient; omit them from queued and restart snapshots; honour
  cancellation through named-destination and outline traversal; retain only content-free
  failures; and verify exact source size and modification time before report delivery.
- Treat bookmark output as a full rewrite: require certificate-signature acknowledgement,
  never overwrite the source, publish create-new, preserve form structure, and reopen and
  compare every bookmark before publication. Recheck the reviewed source size and
  modification time immediately before publication. When output protection is selected,
  bound both passwords to 127 UTF-8 bytes, apply QPDF AES-256 in the cancellable worker,
  then decrypt and repeat the bookmark, page, and form checks. Explain that reader
  permissions are advisory and that specialist zoom coordinates and external destinations
  are normalised to whole-page Fit.
- Verify every generated contents marker, title, entry count, content stream, Type0 and
  CIDFontType2 structure, ToUnicode map, embedded TrueType stream, and link destination
  after reopening. Confirm that the first source page is not marked as generated, shift
  every source destination consistently, and repeat the same checks after AES-256
  decryption. Explain that printed page numbers are physical output numbers and generated
  pages are untagged, so they cannot support a PDF/UA conformance claim.
- Stream heading analysis locally and retain at most 2,000 pages, 250,000 text items,
  25,000 items per page, 100,000 candidate lines, and 1,000 suggestions. Cancel stale
  streams and never persist source text, passwords, or unselected heading material.
- Bound annotation review to 500 candidate entries per page and 2,000 overall. Expose
  editable source identities only for indirect, self-contained, representable
  `FreeText`, single-quad `Highlight`, `Stamp`, single-stroke `Ink`, `Square`, `Circle`,
  and plain `Line` dictionaries. Keep direct, linked, rich, multi-part, structurally
  complex, and excess annotations visible and read-only.
- Bound annotation export to 2,000 combined add, update, and remove changes, 500 built
  items per page, 10,000 points per freehand
  item, 250,000 total points, 4,096 text characters, 12 MB decoded PNG data, 8,192-pixel
  source dimensions, 2,048-pixel embedded dimensions, 128 MB decoded image allocation,
  1,024 password bytes, and 64 MB object-stream decompression.
- Run annotation source inspection through a distinct shared read-only job. Keep its
  path and password transient; omit them from queued and restart snapshots; honour
  cancellation during page traversal; retain only content-free failures; and verify
  exact source size and modification time immediately before delivering the report.
- Treat annotation output as a full rewrite: fingerprint the reviewed source, require
  certificate-signature acknowledgement, never overwrite the source, and accept only
  source identities rediscovered in the exact reviewed input. Reject duplicate,
  invented, cross-page, type-changing, or simultaneous update/remove requests. Remove
  only explicitly targeted array entries, preserve unsupported annotations and AcroForm
  structure, publish create-new, then reopen and verify every generated or replacement
  marker, subtype, appearance, image resource, page count, and exact per-page annotation
  count. Recheck the source immediately before publication. For protected
  output, apply cancellable QPDF AES-256 to the prepared copy, decrypt the candidate,
  and repeat the checks using stable marker values rather than indirect object numbers.
- Keep draft annotation text and image pixels in session memory only. Full Unicode text
  remains in the PDF annotation contents, but warn when the built-in Windows Latin
  appearance font substitutes unsupported glyphs; do not imply CJK or RTL shaping.
  Exclude draft text, image bytes, identifiers, passwords, and paths from public job
  snapshots. Retained annotation-job failures use content-free diagnostics.
- Bound page-content review to a 32 MiB decoded stream, 512 MiB total decoded content,
  50,000 streams, 250,000 operators per stream, two million operators overall, 20,000
  editable text runs, 5,000 editable images, four million direct reference-scan nodes,
  32 page-tree levels, a 1,024-byte password, and 64 MiB of object-stream decompression.
  Bound publication to 2,000 text edits and
  500 image edits, 4,096 replacement characters, 16 KiB replacement text, 24 MiB image
  data, 8,192-pixel source dimensions, 4,096-pixel embedded dimensions, and 256 MiB of
  decoded image allocation.
- Expose an editable text identity only for an exact, unshared, indirect page content
  stream referenced exactly once across the whole document and one `Tj` string whose
  active font and size are known and whose original bytes round-trip through the bounded
  font encoding. Require every replacement to
  round-trip through that same font. Expose an editable image identity only for an
  exact unambiguous page-level `q`, axis-aligned positive `cm`, image `Do`, `Q` block
  with an effectively identity outer transform. Preserve shared streams, direct or
  malformed content, nested forms, complex text operators, arbitrary vectors, and
  unsupported encodings read-only.
- Treat page-content output as a reviewed structural rewrite. Accept only opaque source
  identities rediscovered in the exact full-SHA-256 reviewed input; reject duplicates,
  invented identities, stale size, modification time, or hash, and certificate-signed
  input without acknowledgement. Never overwrite. Reopen the prepared copy and verify
  edited stream markers and decoded hashes, untouched stream hashes, replacement-image
  markers, exact RGB hashes and dimensions, page count, AcroForm and bookmark presence,
  and per-page annotation counts. Recheck the complete source hash immediately before
  publication. For protected output, apply cancellable QPDF AES-256 only after the
  prepared check, decrypt it, and repeat the stable checks.
- Clone image resource dictionaries page-locally before replacement so inherited or
  shared resources are not mutated. Do not prune unrelated objects during page-content
  export: an old image object may remain unreachable or unpainted. State this in the
  result and direct users to Privacy Cleaner before sharing when hidden-data removal
  matters.
- Bound AcroForm handling to 2,000 fields, 5,000 widgets, a field-tree depth of 32,
  512-character and 2,048-byte field names, 4,096-character and 16 KiB values, 1,000
  options per field, 20,000 options overall, 1,024-byte passwords, and 64 MiB of
  object-stream decompression. Reject malformed field trees, cycles, invalid values,
  excessive structures, and XFA rather than attempting partial dynamic-form execution.
- Run AcroForm source inspection through a distinct shared read-only job. Keep its path
  and password transient; omit them from queued and restart snapshots; honour
  cancellation through page annotations, widgets, options, and recursive field-tree
  traversal; retain only content-free failures; and verify exact source size and
  modification time before delivering the field model.
- Treat form output as a full rewrite: fingerprint the reviewed source, require explicit
  certificate-signature acknowledgement, never overwrite the source, preserve unrelated
  annotations and unsupported fields, publish create-new, then reopen and verify values,
  appearances, page count, flattened markers, and removed field objects before publication.
  Recheck the source immediately before publication. For protected output, verify the
  prepared copy by object identity, apply cancellable QPDF AES-256, then decrypt and
  verify stable field names, exact values, counts, appearances, and markers without
  relying on indirect object numbers.
- Keep draft form values and opening passwords in session memory only and exclude them
  from recovery or shared-job snapshots. Retained form-job failures use content-free
  diagnostics rather than field names or values. Store exact Unicode field values in the PDF, but report when
  the built-in Windows Latin appearance font substitutes unsupported glyphs. Explain that
  encrypted input produces an unlocked form copy unless output protection is selected.
- Flatten only text, checkbox, radio, and choice fields whose widgets have complete page
  geometry. Preserve signature fields, push buttons, unsupported fields, unrelated page
  annotations, and incomplete widgets; never present flattening as certificate signing.
- Bound page finishing to 20,000 pages, a 4,096-byte range expression, 512-character and
  2,048-byte mark templates, 1,024 expanded characters, 14,400-point page dimensions,
  7,200-point crop margins, 1,024-byte passwords, and 64 MiB of object-stream
  decompression. Reject invalid page trees, page boxes, rotations, ranges, colours,
  dimensions, margins, coordinate arrays, and mark values before writing output.
- Run Page Finish source inspection through a distinct shared read-only job. Keep its
  path and password transient; omit them from queued and restart snapshots; honour
  cancellation through page-geometry and annotation traversal; retain only content-free
  failures; and verify exact source size and modification time before delivering the
  typed workspace model. Keep the synchronous entry point available only to native
  tests and controlled worker dispatch, not as a registered Tauri command.
- Treat page finishing as a full rewrite: fingerprint the reviewed source, require
  certificate-signature acknowledgement, never overwrite the source, publish create-new,
  then reopen and verify page count, visible and media boxes, form and bookmark presence,
  annotation counts, per-page operation markers, and every generated mark layer. Recheck
  the source immediately before publication. For protected output, apply cancellable
  QPDF AES-256 to the prepared copy, decrypt the candidate, and repeat every structural
  check using stable marker values rather than indirect object numbers.
- Explain that crop changes the visible page box but leaves hidden content recoverable;
  it is never redaction. Resizing wraps original content in a clipped proportional
  transform and carries standard annotation, link, popup, ink, and form-widget geometry,
  but specialist bookmark destinations and uncommon private coordinate structures still
  require review.
- Keep page-finish passwords and draft text in session memory only. Mark layers use a
  built-in Windows Latin font and report unsupported glyph substitution. Explain that
  marks are visual rather than cryptographic. Exclude mark text, passwords, paths, and
  settings from public job snapshots; retained finishing-job failures use content-free
  diagnostics. Explain that encrypted input produces an unlocked copy unless output
  protection is selected.
- Bound permanent redaction to 20,000 source pages, 256 rasterised pages per export,
  10,000 regions per page, 100,000 regions overall, 8,192 pixels per raster dimension,
  40 million pixels per page, 300 million pixels overall, 32 MiB of decoded PNG data
  per page, 256 MiB overall, 192 MiB image-decoder allocation, 14,400-point page
  dimensions, 1,024-byte passwords, and 64 MiB of object-stream decompression.
- Run permanent-redaction source inspection through a distinct shared read-only job.
  Keep its path and password transient; omit them from queued and restart snapshots;
  honour cancellation through page-geometry and annotation traversal; retain only
  content-free failures; and verify exact source size and modification time before
  delivering the typed destructive-workspace model. Keep the synchronous entry point
  available only to native tests and controlled worker dispatch, not as a registered
  Tauri command.
- Bound search assistance to 100,000 extracted text items and two million characters
  per page, 2,000 returned suggestions, a 512-character query, short wildcard runs,
  and a 100-step session history. Treat matches as approximate review suggestions;
  never convert them into redactions without explicit selection.
- Redact by rendering the reviewed page upright to a clean bounded PNG and sending its
  reviewed normalised geometry separately. Treat the raster and geometry as untrusted
  request data: reject unknown fields, non-finite or out-of-bounds rectangles, regions
  below the minimum review size, excessive counts, and excessive mask-pixel work. Decode
  the PNG in Rust, expand each region by one pixel to cover raster edges, apply opaque
  black or white masks in reviewed order, and replace the selected page dictionary,
  content, and resources with one generated lossless RGB image. Never publish an
  annotation, overlay-only result, frontend-painted raster, or claimed count as
  permanent redaction.
- Treat redaction output as a full privacy publication: fingerprint the reviewed
  source, check its opening size and modification time when the worker starts and again
  after output verification immediately before publication, require certificate-signature
  acknowledgement, never overwrite, publish
  create-new, strip metadata and identifiers, scripts and actions, attachments,
  annotations and AcroForm data, bookmarks and named destinations, thumbnails,
  optional-content catalogue data, and tagged-document structures, then prune
  unreachable objects.
- Reopen prepared redaction output and verify the original page count, every selected page's
  exact marker and native-derived redaction count, upright media and crop boxes, one
  generated content stream, one generated image resource, decoded raster size and the
  SHA-256 digest of the complete native-masked RGB raster, absence of searchable text,
  absence of page-private entries and global privacy residue before publication. For
  protected output, apply cancellable QPDF AES-256 only after this check, require an
  encrypted candidate, decrypt it with the opening password, and repeat every redaction
  and privacy check. A failed check must leave no destination file.
- Explain that selected pages lose selectable text, links, annotations, forms, and
  accessibility semantics. Running OCR afterwards may recreate text from visible areas
  and requires a new privacy review. Unselected page artwork remains structural PDF
  content, while interactive and private document structures are removed globally.
- Keep redaction rectangles, extracted search text, suggestions, source passwords, and
  page rasters in session memory or bounded transient native request memory only.
  Exclude them from public job snapshots and recovery snapshots, remove queued request
  data on cancellation or worker hand-off, and release canvas memory after each PNG has
  been prepared. Keep output passwords in transient request memory too, and retain only
  content-free redaction-job failures.
- Recovery drafts may contain document names and local source paths. Never include
  passwords, signature images, document text, or PDF/image bytes in those drafts.
- Do not create a recovery draft for a page plan that depends on an in-memory imported
  PDF password. Unencrypted imported source paths may be recovered locally.
- For standalone Merge and Split recovery, persist only bounded source identities,
  source paths, source order, range text, and page-group text. Reject unknown nested
  fields and duplicate Merge sources, verify source-file presence before restoration,
  and require passwords and certificate-risk acknowledgements again. Do not persist
  output protection, undo stacks, job requests, or recognised document content.
- Explain that PDF printing, copying, and editing permissions may be ignored by
  some readers and are not a substitute for a certificate-backed signature.
- Explain that an encrypted visual-signature library protects stored signature images;
  it does not authenticate a signer or make a PDF cryptographically tamper-evident.
- Generate release checksums by streaming only recognised distributable files, reject
  duplicate or control-character filenames, and never include absolute build paths.
  Sanitise dependency source URLs before writing licence reports so credentials,
  queries, fragments, and local dependency paths are not published.
- Inspect packages on their native build platform before release metadata is created.
  Reject missing, duplicate, stale, malformed, linked, architecture-mismatched, or
  invalidly signed package candidates. Mount macOS disk images read-only, extract Linux
  packages only into private temporary directories, and record only bounded filenames,
  product metadata, hashes, expected public signer identity, timestamp and notarisation
  state, and container identities in evidence.
- Treat clean-container Linux installation as compatibility evidence, not a sandbox for
  untrusted third-party packages. Mount only the release bundle directory read-only,
  use fixed distribution images, record their immutable image IDs, require dynamic-link
  closure, and publish no host path or command output.
- Audit the complete tracked and untracked Git candidate tree before publication.
  Require the public release files, reviewed source and image types, strict UTF-8/LF
  text, and bounded file and tree sizes. Reject generated or private paths, environment
  files, private-key containers, credential signatures, and personal absolute home
  paths. Build the public source archive only from the exact audited Git index.
- Derive release SBOMs and licence declarations from locked npm and Cargo metadata.
  Treat the licence report as a review inventory rather than legal advice, and keep
  package signing and notarisation as mandatory gates distinct from updater signatures.
- Fail CI and tagged release preflight on any npm advisory reachable from distributable
  dependencies or any RustSec vulnerability in the locked Cargo graph. Review
  development-only npm findings and RustSec maintenance or soundness warnings separately,
  keep affected tools pinned, and track both ecosystems with Dependabot until compatible
  upstream fixes are available.
- Keep publisher certificate containers, passwords, the temporary-keychain password,
  and the App Store Connect private key outside the repository and source archive.
  Supply them only to their matching runner through the protected `updater-signing`
  environment. Write only the Git-ignored, public platform-signing overlay; never write
  certificate bytes, passwords, or API private keys into project configuration.
- Import the Windows PFX as non-exportable and require its exact configured thumbprint,
  private key, Code Signing extended-key usage, validity, and minimum remaining lifetime.
  Require both MSI and NSIS signatures to match that thumbprint and carry a timestamp.
  Remove every certificate introduced by the import and the temporary PFX immediately
  after the build, without removing pre-existing store entries.
- Bind the macOS Developer ID identity to the configured Apple team, import it into an
  ephemeral keychain, and keep the App Store Connect key in a mode-`600` temporary file.
  Require the packaged app's exact team identifier, secure timestamp, successful
  Gatekeeper assessment, and stapled notarisation ticket. Restore the original keychain
  and remove every temporary credential file immediately after the build.
- Keep the updater private key and its password outside the repository and source
  archive. Supply them only through the protected `updater-signing` environment; write
  only the release-only `createUpdaterArtifacts` overlay, and fail tagged builds when
  the channel, HTTPS endpoint, public key, or private signing credential is incomplete.
- Keep update checks user-triggered. Bind each official build to one alpha, beta, or
  stable HTTPS endpoint and embedded public key, retain only bounded public progress and
  version metadata, and never expose an interface or recovery path that bypasses
  signature verification. Development builds must remain unconfigured and offline.
  Compile the updater only for desktop targets; iOS must expose only an App Store-managed
  status and must not accept a custom endpoint, key, package, or in-app installation.
- Promote only a published immutable release through the separately approved
  `updater-promotion` environment. Revalidate every platform entry, signature, and
  immutable asset URL, require the channel copy of `latest.json` to be byte-identical,
  and retain path-free evidence. Stop a faulty channel by withdrawing its manifest;
  recover automatically only with a higher-version signed fix or revert, never an
  unsafe downgrade. Follow [signed application updates](docs/UPDATES.md) for key
  rotation, key loss, and manual recovery.

## Reporting Issues

Use GitHub's private vulnerability reporting feature for the repository. Please
do not publish exploitable PDF samples, credentials, certificates, signatures,
or personal documents in a public issue.
