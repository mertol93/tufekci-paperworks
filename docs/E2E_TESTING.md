# Native End-to-End Testing

Tüfekci Paperworks uses WebdriverIO against the real Tauri window on Windows, macOS,
and Linux. The suite exercises the application in its native webview and calls the real
Rust backend. It does not replace the generated PDF/OCR corpora, Rust tests, physical
scanner matrix, package installation checks, or assistive-technology review.

## Covered Workflows

The fixed evidence contract contains eleven cases:

1. Native shell startup, Tauri mode, engine readiness, the standalone Recognise Text
   workspace, and a bounded desktop viewport.
2. Arrow-key workflow navigation with wrapping, plus document-editor skip focus.
3. Modal initial focus, focus containment, Escape handling, and opener focus return.
4. Real Rust-backed PDF opening followed by single-page drag and explicit two-page
   selection; reviewed two-document drag insertion; verified Copy with an unchanged
   source; verified Move with deferred source removal and one-step undo; ordered group
   drag, group step movement, rotation, duplication and deletion with one-step undo; then
   ordinary duplication, paper-sized blank insertion, rotation, deletion, undo, and redo.
5. Non-blank PDF.js canvas pixels and progressive text search across the edited plan.
   The case waits for the visible canvas and text layer to finish before starting the
   document-wide extraction, so it does not overlap two PDF.js text streams. Pure
   boundary tests separately cover selected-locale casing, compatibility normalisation,
   retryable rejected caches, and content-free extraction failures.
6. `Ctrl+P` routing, rejected and accepted custom page ranges, bounded local PDF.js
   print-intent rendering from the edited organiser plan, physical per-page boxes,
   non-blank prepared pixels, volatile previews, and a simulated system-dialogue request
   that never sends a physical CI print job.
7. Native-reviewed existing-text replacement with undo, redo, create-new publication,
   and the Rust verifier's success result.
8. Bookmark creation, linked A4 contents preview with shifted physical page numbers,
   create-new publication, and the native verifier's success result.
9. Merge-source drag reordering, selected ranges, enabled bookmark preservation,
   create-new publication, and exact preserved and omitted counts from the native result.
10. An independent four-page signature journey: image drop and local background removal;
   typed initials; freehand canvas creation; named reusable assets; page drop and
   existing-placement movement; encrypted local-vault save; a sanitised wrong-passphrase
   result; successful retry and fixture deletion; proportional resize; rotation;
   duplication; undo and redo; placement locking; create-new flattening; exact native
   mark-count verification; and reopening with non-blank rendered pixels.
11. Explicit Turkish and German interface switching; translated workflow, organiser,
    Merge, searchable OCR, image-scan, connected-scanner, Split, Protect, Compression,
    local Activity, and signed-Update controls; translated
    document-editor, thumbnail, and live page-canvas accessibility names; live German document-size
    formatting; a Turkish AES-256 password prompt with rejected and accepted attempts;
    a German reopen and safe cancellation of the same prompt; persisted locale identity;
    root `lang` metadata; German organiser and scan and Turkish release-surface visual
    evidence; and restoration of the British English default.

The PDF is generated from repository source and contains synthetic page-size and text
fixtures. The uploaded signature image is generated in memory, while typed initials and
freehand strokes are created through the real application controls. Public runs must
not use personal, customer, signed, or otherwise sensitive documents.

## Test-Only Boundary

The native driver is excluded from production in several independent layers:

- Cargo's `e2e` feature enables the optional `tauri-plugin-wdio` and
  `tauri-plugin-wdio-webdriver` dependencies. The default feature set enables neither.
- `src-tauri/tauri.e2e.conf.json` uses the dedicated
  `org.tufekci.paperworks.e2e` identifier and grants the WDIO capabilities only to that
  build. The ordinary Tauri configuration does not expose the global Tauri API or WDIO
  permissions.
- Rust refuses to start an E2E-feature build under any other application identifier.
- Vite aliases the WebDriver guest bridge and one-shot open/save selections only in
  `e2e` mode. Production resolves a side-effect-free function that always returns
  `null`.
- Every production frontend build scans its emitted HTML and JavaScript and fails if a
  WebDriver, boot-diagnostic, or E2E chooser marker is present.

The embedded WebDriver server is intended only for the isolated E2E binary and listens
on loopback. Never distribute or use that binary as a production build. The dedicated
identifier also separates its application data from an ordinary installation.

`@wdio/tauri-service` 1.2.0 currently imports an API added in
`@wdio/native-utils` 2.5.0 while its published dependency resolves 2.4.0. The exact npm
override is deliberate and covered by the boundary test; remove it when the upstream
packages align.

The locked test graph also narrows two security overrides: Mocha resolves
`serialize-javascript` 7.0.5, and `recursive-readdir`'s `minimatch` 3.1.5 resolves
`brace-expansion` 1.1.17. Both replace advisory-affected transitive versions without
changing the distributable application graph. A clean `npm ci`, the complete frontend
suite, and the native runner must pass before either override changes; remove them when
the corresponding upstream dependency ranges include reviewed fixed versions.

The release gate runs `npm run security:audit-production` and permits no known advisory
in dependencies shipped with the application. A full `npm audit` can separately report
development-only findings inherited through WebdriverIO and Mocha. Those findings still
require maintainer review, exact test-tool pins, and Dependabot tracking, but they must
not be confused with the distributable dependency result or bypassed with a forced,
incompatible downgrade.

## Local Commands

Install the normal development prerequisites and locked dependencies, then run:

```bash
npm ci
npm run e2e
```

The combined command generates the rendering fixture, builds the isolated debug app,
runs the native suite, and verifies the report. Its stages can also be run separately:

```bash
npm run e2e:build
npm run e2e:test
npm run e2e:verify
```

The app must be allowed to open a desktop window. The suite requests 1280 × 820 and
records the live DOM content viewport used by the responsive interface; this avoids the
unsupported zero window-rectangle response returned by embedded WebKitGTK. On Linux,
install Xvfb and run it inside a 1280 × 900 virtual screen:

```bash
WEBKIT_DISABLE_COMPOSITING_MODE=1 \
  xvfb-run -a --server-args="-screen 0 1280x900x24" npm run e2e:test
```

On macOS the debug test app builds for the runner's native architecture; official
release packaging remains a separate universal Intel and Apple Silicon build. Windows
uses the installed WebView2 runtime.

## Apple Mobile Acceptance

The eleven-case WebDriver contract is a desktop contract and must not be presented as
iOS/iPadOS UI evidence. `.github/workflows/apple-mobile.yml` provides a narrower
credential-free compile gate: it validates source configuration and frontend tests,
generates the ignored Xcode project on macOS 15, compiles the unsigned arm64 simulator
application, verifies the main bundle's identifier, iPhone/iPad device families, iOS 16
minimum, simulator platform, document and indirect-input settings, and non-empty
executable, then retains a ZIP and content-free SHA-256 report.

Before an Apple mobile build can be distributed, retain simulator and physical-device
evidence for:

- compact iPhone and regular iPad widths in portrait and landscape, including split
  view and safe areas;
- opening PDFs and images through the in-app Files picker, password prompts for reading,
  large-document loading, search, zoom, and non-blank page rendering;
- page selection and reordering through visible buttons as well as touch, keyboard,
  trackpad, and pointer input;
- pure-Rust page editing, image-to-PDF, visual-signature preparation, encrypted vault,
  flattening, create-new export, reopen, and source preservation;
- clear disabled states for OCR, PDF/A, QPDF protection changes, certificate signing,
  connected scanning, and camera capture, including attempted backend requests;
- App Store-managed update presentation with no desktop update endpoint or download;
- Dynamic Type, VoiceOver, keyboard focus, contrast, reduced motion, interruption,
  background/foreground, low-memory, and failed-save behaviour; and
- installation and upgrade through a signed development build and TestFlight without
  leaking documents, paths, passwords, signature images, or diagnostics into evidence.

Camera capture, system-wide Open In hand-off, native mobile OCR, and App Store
distribution remain unavailable until their own implementation and evidence are added.

## Drag-and-Drop Detail

The embedded driver can deliver pointer actions but cannot ask WebView2 or WebKit to
promote synthetic pointers into browser-owned HTML drag events. The page test therefore
dispatches the complete `dragstart`, `dragenter`, `dragover`, `drop`, and `dragend`
sequence inside the real native webview. It waits for the visible dragging class before
drop so React has committed the source page or selected page set. The case first proves
a single-page move, uses the explicit toggle to select two pages, checks the visible and
accessible selection state, drags the ordered group, verifies the exact source-page
order, and undoes the complete group move once. It then rotates, duplicates, moves and
deletes the same selection, proving each result and undo boundary before continuing.
This proves the application's drag, selection, group-action and history operation;
ordinary manual pointer testing still covers the operating system's physical input path.

The merge-source test dispatches the same complete browser drag sequence and verifies
the reordered source names before publication. The signature test uses the real image
and page drop handlers, then dispatches deterministic pointer events inside the native
webview for canvas drawing and existing-placement movement. This exercises React's
pointer-capture paths without the Tauri/WebView2 deadlock seen with long W3C action
chains. It verifies prepared pixels, visible placement geometry, history and lock state,
the native export result, and a reopened PDF rather than checking only that controls were
clicked. The localisation case uses the native select control and verifies visible
translation, accessibility metadata, page-action controls, and locale-aware formatting
on a live document. It submits a known wrong password before opening the generated
AES-256 fixture in Turkish, then reopens and cancels the German prompt before restoring
the ordinary four-page fixture, whose canvas name is checked in both languages. This
proves retry state and modal lifetime through the
real PDF.js password callback without recording the secret in evidence. It also opens Document Health, Privacy
Cleaner, PDF Comparison, Page Finish, Annotation, Forms, Page Content, and Permanent
Redaction, PDF Standards, Batch Recipes, and Bookmarks in Turkish and German and
checks their translated headings and primary actions. Optional screenshot capture includes a
multi-selection frame as `page-multi-selection-desktop.png` and a German organiser frame
as `localisation-de-organiser-desktop.png`, plus translated Turkish release surfaces as
`localisation-tr-release-surfaces-desktop.png`.

The print case replaces only the last `window.print()` boundary with an isolated
test-build counter. Page selection, PDF.js print-intent rendering, form and annotation
storage, page geometry, blob-image creation, print DOM construction, and pixel sampling
still run in the real native webview. Physical printer and PDF-target output stay in the
separate manual matrix because CI must never dispatch an unattended print job.

## Evidence

A successful run creates exactly one ignored file under `e2e-evidence/`, named for the
platform and architecture. The strict schema records only:

- product and release identity;
- operating system, architecture, and native webview;
- desktop mode, embedded provider, test boundary, and viewport;
- the eleven fixed case identifiers with `passed` outcomes.

Reports reject unknown fields, failed or reordered cases, unsafe sizes, carriage-return
line endings, and unexpected files. They contain no paths, filenames, PDF text, image
bytes, passwords, logs, screenshots, or document-derived content.

CI retains one report from each operating system for 14 days. Tagged builds retain
release reports for 30 days, and release metadata fails unless there is exactly one
Linux x64, one Windows x64, and one native x64 or arm64 macOS report. The verified
reports are attached to the draft release. The release-plan item remains open until the
first real three-platform workflow run has been retained and reviewed.

## Troubleshooting

- Rebuild with `npm run e2e:build-app` after changing application source or the E2E
  Tauri configuration.
- Only one suite instance may use the configured embedded port at a time.
- A failed suite writes no partial success report. Fix the failure and rerun the whole
  suite before calling `npm run e2e:verify`.
- Run `npm run build` to prove the production bridge scan still passes after changing
  Vite, Cargo features, Tauri capabilities, or the test bridge.
