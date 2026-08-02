# Printing Test Matrix

Tüfekci Paperworks prepares print pages locally with PDF.js and opens the operating
system print dialogue. The application does not enumerate printers or bypass that
dialogue. Printer choice, copies, paper, colour, orientation, scaling, and duplex
settings remain under operating-system and driver control.

Prepared output is intentionally rasterised at Standard (150 dpi) or High (300 dpi).
It is not vector-preserving PDF passthrough. One request is limited to 100 pages,
50 megapixels per page, and 120 megapixels overall. Temporary PNG object URLs are
revoked whenever the document, range, quality, visual-mark choice, or workflow changes.

## Automated Boundary

Run:

```bash
npm run test:frontend
npm run e2e
```

The unit gate checks range parsing, rotation-aware physical geometry, quality budgets,
PDF.js print intent, current form/annotation storage, visual-mark composition, volatile
object URLs, and the production system-print call. The isolated native E2E build
replaces only the final `window.print()` call with a counter. It renders two real pages
inside WebView2, WKWebView, or WebKitGTK, checks non-blank pixels and per-page physical
boxes, and verifies that exactly one dialogue request was made. It never sends a CI job
to a physical or virtual printer.

## Release Hardware

Retain one completed worksheet for each supported platform:

| Platform | Required webview | Required targets |
| --- | --- | --- |
| Windows x64 | WebView2 | One physical printer and Microsoft Print to PDF |
| macOS x64 or arm64 | WKWebView | One physical printer and Save as PDF |
| Linux x64 | WebKitGTK | One CUPS physical printer and Print to File |

Use only repository-generated fixtures. Never use personal, customer, signed, medical,
financial, identity, or otherwise sensitive documents in retained evidence.

For each platform and target, verify:

1. `Ctrl+P` or `Command+P` opens Print settings and does not start printing.
2. All, current, and custom ranges accept valid input and reject empty, malformed,
   reversed, out-of-document, overlong, and over-100-page requests.
3. Reordered, duplicated, rotated, imported, and blank pages appear in workspace order.
4. Portrait, landscape, mixed-size, A4, US Letter, business-card, and large-square pages
   are neither clipped nor stretched unexpectedly by the selected driver setting.
5. Printable annotations and current AcroForm values appear; non-printing content obeys
   the source PDF's print intent.
6. Included visual signatures and initials are flattened at their reviewed position,
   size, and angle. Excluding them removes them from prepared output.
7. Standard and High quality produce legible small text and line work at their stated
   resolution; no interface chrome, preview labels, or hidden pages are printed.
8. Printer, copies, paper, colour or greyscale, orientation, scaling, and duplex controls
   can be changed in the system dialogue and are honoured by the target.
9. Password-protected PDFs print only after a successful local opening password; no
   password, path, filename, page image, or form value appears in logs or evidence.
10. Cancellation during checking and rendering opens no dialogue and leaves no visible
    prepared pages. Changing settings revokes the old preview and prepares a fresh one.
11. A very large page, a job above the pixel budget, and a document above 100 selected
    pages fail with the documented local safety message rather than exhausting memory.
12. An unavailable or disabled print service reports a useful failure and does not
    close, overwrite, export, upload, or otherwise mutate the source document.

## Evidence Record

Record only platform, architecture, app version, webview version, target category,
printer-driver family, tested settings, case outcomes, and reviewer date. A retained
PDF-target artefact may contain only generated fixture content. Do not retain local
paths, hostnames, usernames, printer serial numbers, queue addresses, document-derived
text from non-fixtures, or screenshots containing unrelated desktop content.

Printing remains **Experimental** in `FEATURE_STATUS.md` until all three platform
worksheets and representative output artefacts have been reviewed. A vector-preserving
native spool path, if added later, requires a new architecture and fidelity review; it
must not silently replace or weaken this bounded raster path.
