# Release Metadata

Every tagged draft release must include machine-readable evidence for the exact
packages published with it. The release workflow gathers the bundles from all three
platform jobs and generates these files only after every native package check and Linux
installation test succeeds:

- `package-report-windows-x64.json`: exact MSI and NSIS inventory, metadata, architecture,
  Authenticode state, exact signer thumbprint, timestamp state, size, and SHA-256.
- `package-report-macos-universal.json`: mounted-DMG identity, universal Mach-O payload,
  Developer ID team, secure timestamp, Gatekeeper and stapled notarisation-ticket state,
  size, and hashes.
- `package-report-linux-x64.json`: AppImage, deb, and rpm container and metadata checks,
  with one byte-identical executable payload hash.
- `linux-install-report-x64.json`: exact container-image identities and successful
  AppImage extraction or package installation on Ubuntu 22.04, Debian 13, and Fedora 43.
- `package-evidence-summary.json`: hashes, exact expected publisher identities, and
  bounded observed package identities from the four reports above. It contains no local
  paths.

- `SHA256SUMS`: SHA-256 for each distributable package, sorted by release filename.
- `RELEASE-MANIFEST.json`: package filename, source path, byte size, and SHA-256.
- `sbom-npm.cdx.json`: npm's CycloneDX 1.5 application SBOM from `package-lock.json`.
- `sbom-cargo.cdx.json`: CycloneDX 1.5 components and dependency edges from locked
  Cargo metadata.
- `DEPENDENCY-LICENCES.json` and `DEPENDENCY-LICENCES.csv`: one combined declaration
  inventory for npm and Cargo packages.

The workflow also attaches separate path-free rendering, OCR, PDF/A, certificate, and
native GUI evidence reports. Certificate reports record the exact pyHanko/OpenSSL pair,
public fixture digest, visible and incremental signing, encrypted-input preservation,
integrity/trust separation, and mandatory tagged timestamp result; generated identity
material is never retained.

The licence inventory is not legal advice and does not declare compatibility. A
missing licence, `UNLICENSED`, `SEE LICENSE`, `UNKNOWN`, `NONE`, or `LicenseRef`
declaration is marked for manual review. A release owner must also review changes to
declared licence expressions even when the automated review count is zero.

## Local Generation

Build or gather the intended packages into one directory, then run:

```bash
npm run release:metadata -- path/to/release-assets path/to/release-metadata
```

The generator reads both lockfiles, invokes `cargo metadata --locked`, and runs npm's
own package-lock-only CycloneDX command. Cargo may need to download target-specific
packages the first time because the inventory covers locked cross-platform
dependencies, not only crates compiled on the current machine.

Set `SOURCE_DATE_EPOCH` to produce stable timestamps. The tagged workflow derives it
from the tagged commit and also replaces npm's random SBOM serial number with a stable
lockfile-derived UUID. Package hashing is streamed so large installers are not loaded
fully into memory.

The artefact directory must contain at least one recognised package. Duplicate
filenames are rejected because GitHub release assets share one filename namespace.
Generated metadata files are excluded if the output directory sits below the input.

Package evidence is stricter than the final metadata collector. Each native report has
an exact schema and rejects stale versions, duplicate formats, unexpected fields,
unsafe filenames, invalid signatures, architecture mismatches, malformed containers,
path disclosure, an unexpected Windows signer, a missing timestamp, an unexpected Apple
team, a failed Gatekeeper assessment, or a missing stapled ticket. The aggregate accepts
exactly the three platform reports and one Linux installation report for the same
release and signature policy.

## Release Review

1. Confirm the package-evidence summary, all four package/install reports, and every
   three-platform rendering, OCR, PDF/A, certificate, and native GUI report are attached.
2. Confirm every expected Windows, macOS, and Linux package appears in the manifest.
3. Confirm `SHA256SUMS` matches the files downloaded from the draft release.
4. Review every flagged dependency and every licence-expression change.
5. Validate both CycloneDX documents with the release security tooling.
6. Confirm the exact Windows signer and timestamp and the macOS Developer ID team,
   timestamp, Gatekeeper, and stapled-ticket fields in the native reports. Keep the draft
   unpublished until the real credential-backed and representative installation gates
   have passed.

Checksums, SBOMs, and package reports improve provenance and reviewability; they do not
create Windows code signatures, Apple notarisation, package-manager signatures, or
update signatures.
