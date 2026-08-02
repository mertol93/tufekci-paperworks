# Accessibility Testing

Tüfekci Paperworks is designed for keyboard use and assistive technologies, but the
project does not claim WCAG conformance or accessibility certification. Automated
contracts protect the application shell and modal focus behaviour; packaged builds
must still pass the manual matrix below before a stable release.

## Automated Baseline

`npm run test:frontend` verifies that:

- the shell offers a visible-on-focus link directly to the document editor;
- every interactive control receives a high-contrast visible keyboard focus outline;
- the workflow chooser exposes one tab stop with Arrow key, Home, and End navigation;
- every modal dialog uses shared initial-focus, Tab containment, safe Escape, and focus
  return behaviour; and
- modal titles, selected workflow state, document landmarks, live status messages, and
  alert regions remain present in source contracts.

These checks prevent structural regressions. They do not reproduce a browser
accessibility tree, a screen reader, operating-system contrast settings, magnification,
or speech input.

## Release Matrix

Retain one completed record for each packaged release candidate.

| Platform | Package | Browser surface | Keyboard | Screen reader | Display settings |
| --- | --- | --- | --- | --- | --- |
| Windows 11 x64 | Signed MSI and NSIS | WebView2 | Keyboard only | Narrator and current NVDA | 200% text, Contrast theme |
| macOS current and previous | Signed, notarised app/DMG | WKWebView | Keyboard navigation enabled | VoiceOver | 200% zoom, Increase Contrast |
| Ubuntu LTS x64 | AppImage and deb | WebKitGTK | Keyboard only | Orca | 200% text, High Contrast |
| Fedora current x64 | rpm | WebKitGTK | Keyboard only | Orca | 200% text, High Contrast |

Use a release package, not the Vite development preview. Record the exact OS, assistive
technology version, application version, package SHA-256, tester, date, and result.

## Keyboard-Only Pass

1. Launch the app and use the skip link to reach the document editor without traversing
   the header or workflow list.
2. Traverse the header, workflow chooser, document toolbar, pages, canvas actions, and
   inspector without a pointer. Confirm that focus is always visible and never clipped.
3. In the workflow chooser, use Up, Down, Left, Right, Home, and End. Confirm that the
   chosen workflow and its details change together and that only one workflow is in the
   ordinary Tab order.
4. Open an ordinary PDF, a password-protected PDF, an image batch, and a recovery draft.
   Use each thumbnail selection toggle, Ctrl/Command+Space, Shift+Space, Ctrl/Command+A,
   and Escape; then complete single- and multi-page reordering, rotation, duplication,
   deletion, cross-document copy or move, undo, and redo without a pointer. Use the
   numeric insertion control as the keyboard alternative to cross-document drag.
5. Open each of the thirteen modal workspaces. Confirm predictable initial focus, forward and
   reverse Tab wrapping, Escape behaviour, disabled-close behaviour during publication,
   and focus return to the opener.
6. Complete scan clean-up, OCR review, cross-document transfer, signature placement, protection, export,
   cancellation, retry, and error recovery without a pointer.
7. At 200% text size and operating-system high contrast, confirm that controls, labels,
   status text, document pages, and dialogs do not overlap, clip, or disappear.

## Assistive-Technology Smoke Test

1. Confirm that the application name, main landmark, PDF workflow navigation, document
   editor, selected workflow panel, headings, forms, and search region are announced.
2. Confirm that workflow selection, pressed controls, disabled actions, checkboxes,
   progress, success messages, warnings, and errors expose both names and states.
3. Open every modal and confirm that its title and purpose are announced, background
   controls are not reached, and focus returns to the opening control when it closes.
4. Check page thumbnails, page numbers, search results, OCR confidence words, form field
   labels, signature controls, encryption permissions, and redaction warnings for useful
   names that do not depend on colour or icon shape.
5. Read a tagged reference PDF in logical order and compare it with the Document Health
   preflight. Record reading-order defects separately; static preflight is not a substitute
   for assistive-technology review.

## Evidence Record

Store only non-sensitive release evidence. Do not retain customer documents, passwords,
signature images, certificate keys, scanner identifiers, or OCR text.

```text
Application version:
Source revision:
Package and SHA-256:
Operating system:
Assistive technology and version:
Display settings:
Tester and date:
Keyboard-only result:
Assistive-technology result:
Issues and issue links:
Evidence file paths:
```

The release accessibility gate remains open until every required row has passed, all
blocking defects are closed, and the retained evidence matches the published packages.
