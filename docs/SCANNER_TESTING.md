# Connected-Scanner Testing

Connected-scanner tests are a private hardware release gate. CI compiles and tests the
typed contracts, parsers, path checks, and macOS bridge, but hosted runners do not have
representative scanners. Do not mark connected capture release-ready until this matrix
has passed on the intended operating-system versions and drivers.

Use a synthetic printed chart containing text, colour blocks, greyscale ramps, fine
lines, page edges, and a page number. Do not scan identity documents, signatures, or
personal material for public evidence.

## Automated Hardware Harness

The ignored `live_connected_scanner_capture` test discovers the selected device through
the platform adapter, applies a validated request, captures into a private temporary
directory, enforces the requested page limit, verifies path confinement and file bounds,
and decodes every returned image. Run it once without a device ID to print the IDs found
by the adapter; that discovery-only run is expected to stop with a request for an ID.

Windows PowerShell:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml live_connected_scanner_capture -- --ignored --nocapture
$env:PAPERWORKS_SCANNER_DEVICE_ID = 'the-discovered-id'
$env:PAPERWORKS_SCANNER_SOURCE = 'flatbed'
$env:PAPERWORKS_SCANNER_DUPLEX = '0'
$env:PAPERWORKS_SCANNER_DPI = '300'
$env:PAPERWORKS_SCANNER_COLOUR = 'colour'
$env:PAPERWORKS_SCANNER_WIDTH_MM = '210'
$env:PAPERWORKS_SCANNER_HEIGHT_MM = '297'
cargo test --manifest-path src-tauri/Cargo.toml live_connected_scanner_capture -- --ignored --nocapture
```

macOS or Linux:

```bash
npm run build:macos-scanner # no-op on Linux
cargo test --manifest-path src-tauri/Cargo.toml live_connected_scanner_capture -- --ignored --nocapture
PAPERWORKS_SCANNER_DEVICE_ID='the-discovered-id' \
PAPERWORKS_SCANNER_SOURCE='flatbed' \
PAPERWORKS_SCANNER_DUPLEX='0' \
PAPERWORKS_SCANNER_DPI='300' \
PAPERWORKS_SCANNER_COLOUR='colour' \
PAPERWORKS_SCANNER_WIDTH_MM='210' \
PAPERWORKS_SCANNER_HEIGHT_MM='297' \
cargo test --manifest-path src-tauri/Cargo.toml live_connected_scanner_capture -- --ignored --nocapture
```

For feeder runs, set `PAPERWORKS_SCANNER_SOURCE=feeder` and optionally set
`PAPERWORKS_SCANNER_PAGE_LIMIT` between 1 and 200. Set
`PAPERWORKS_SCANNER_DUPLEX=1` only for a duplex feeder. Supported colour values are
`colour`, `greyscale`, and `monochrome`; supported request DPI is 75 to 1,200.

## Required Matrix

Record the operating-system version, application commit, scanner make/model, connection,
driver/backend version, request, page count, warnings, and result for each run.

| Platform | Backend | Required coverage |
| --- | --- | --- |
| Windows | WIA | Flatbed; feeder simplex; feeder duplex; 150/300/600 DPI; all colour modes; A4 and US Letter; page limit |
| macOS | Image Capture | USB and, where available, network discovery; flatbed; feeder simplex; feeder duplex; 150/300/600 DPI; all colour modes; A4 and US Letter; page limit |
| Linux | SANE | At least two maintained backends where practical; flatbed; feeder simplex; feeder duplex; 150/300/600 DPI; all colour modes; A4 and US Letter; page limit |

Also test these deliberate failures through the application interface:

- no scanners connected;
- selected scanner disconnected before capture and during capture;
- empty feeder;
- flatbed request sent to a feeder-only device and the reverse;
- duplex requested from a simplex feeder;
- unsupported DPI or colour mode reported by a driver;
- capture folder or device failure;
- requested page limit reached during a feeder batch;
- application closure after capture and seven-day stale-session clean-up.

The original chart and scanner-owned files must remain unchanged. Successful pages must
appear in scan review, survive clean-up and OCR, and produce a verified create-new PDF.
Errors must be in UK English, must not expose shell syntax or document content, and must
leave no partial PDF publication.

After each successful flatbed and feeder PDF export, use the completion-banner Batch
Recipes action. Confirm that the verified PDF appears once with a Connected scanner
intake label, that a protected scan asks for its opening password again, and that the
saved recipe JSON contains no source path, origin, device information, settings, image
bytes, OCR hints, or password.

For each platform, also exercise the shared job lifecycle through the application:

- cancel a queued capture and a running flatbed or feeder capture;
- confirm the adapter and its descendants stop and partial capture files are removed;
- refresh the frontend during capture and confirm progress reattaches in the same process;
- close the application process during capture and confirm the next launch shows one
  non-resumable interrupted result without a device identifier, settings, or page paths;
- review the device and feeder, start a fresh capture, and confirm it does not replay the
  old request;
- fail page opening after a successful capture, then test both Retry Opening Pages and
  Discard Capture; and
- confirm successful pages open in order and expire under the seven-day capture policy;
  and
- hand the verified scan PDF to Batch Recipes, inspect it, and run one searchable or
  PDF/A recipe without carrying the scanner request or password into retained state.

## macOS Build Gate

The source bridge is `src-tauri/native/macos-scanner/main.m`. `npm run build` compiles
arm64 and x86_64 helpers and creates a universal binary. Before a macOS release, verify:

```bash
xcrun lipo -verify_arch arm64 x86_64 \
  src-tauri/binaries/tufekci-paperworks-scanner-universal-apple-darwin
npm run desktop:build
codesign --verify --deep --strict --verbose=2 \
  "src-tauri/target/universal-apple-darwin/release/bundle/macos/Tüfekci Paperworks.app"
```

The current direct-distribution app is not sandboxed. If App Sandbox is enabled later,
the scanner process must be reviewed with the `com.apple.security.device.usb` entitlement
required by ImageCaptureCore for sandboxed USB access on macOS 14 and newer. Signing and
notarisation remain separate release-engineering gates.

Keep hardware logs private when they contain serial numbers, persistent device IDs, user
paths, or driver diagnostics. A release note may record pass/fail coverage without those
identifiers.
