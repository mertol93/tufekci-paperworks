# Contributing

Thanks for helping build Tüfekci Paperworks.

## Development Principles

- Keep document processing local by default.
- Prefer proven PDF engines over custom PDF parsing.
- Never overwrite a source PDF in place.
- Add tests around page range parsing, output path handling, and command
  generation before expanding an adapter.
- Keep UI workflows previewable before destructive-looking operations.

## Local Setup

Install Node.js, Rust, and the external document tools listed in
`docs/BUILD.md`.

```bash
npm install
npm run desktop:dev
```

Before opening a pull request, run:

```bash
npm run release:source-check
npm run release:apple-mobile-check
npm run check
npm run test:frontend
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
```

## Pull Requests

- Keep changes focused.
- Include screenshots for visible UI changes.
- Include sample PDFs or fixtures only when licensing permits redistribution.
- Explain which operating systems and desktop/mobile targets were tested. Do not claim
  an iPhone/iPad build from source checks alone; simulator compilation requires macOS
  and device/App Store claims require retained signed evidence.
