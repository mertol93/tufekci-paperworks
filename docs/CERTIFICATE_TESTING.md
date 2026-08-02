# Certificate Corpus Testing

The ordinary Rust suite tests request validation, rotation-aware field geometry,
integer pyHanko field coordinates, signature-structure limits, PDF text decoding,
same-size SHA-256 mutation detection, private workspace and password-bridge clean-up,
encrypted-PDF password handling, bounded command output, cancellation, scheduler
privacy, and the distinction between cryptographic integrity and certificate trust. It
does not need a signing identity, pyHanko, OpenSSL, or network access.

The ignored `live_certificate_corpus` test exercises the real engine. The public runner
creates its identity and encrypted fixture at run time, removes every private artefact,
and retains only a closed path-free report.

## Generated Gate

Install pyHanko 0.36.2, pyhanko-cli 0.4.2, and OpenSSL 3, or a subsequently reviewed
compatible set. Generate the rendering fixture and execute the live gate with:

```bash
npm run qa:certificate-engine
```

The command:

1. Regenerates and verifies the public PDF rendering corpus.
2. Creates a short-lived 3,072-bit RSA test identity and PKCS#12 file in the operating
   system temporary directory.
3. Uses pyHanko to create a standards-readable AES-256 encrypted input.
4. Creates and reopens a visible signature, then appends and reopens an invisible
   signature as a second incremental revision.
5. Requires the final revision to have a complete byte range and validates both
   integrity and the configured disposable trust root.
6. Revalidates without that root and requires the signatures to remain intact while
   signer trust is reported as unestablished.
7. Signs the encrypted input through the bounded standard-input password bridge,
   reopens it with the same password, and requires encryption to be preserved.
8. Removes the generated key, certificate, passwords, PDFs, and native outputs in a
   `finally` path after success or failure.

The retained report is written to
`qa-fixtures/certificate-engine/certificate-engine-report-<platform>-<architecture>.json`.
It contains product and engine versions, the public source-fixture SHA-256 digest, and
boolean scenario outcomes. It contains no local path, subject details, password,
certificate, private key, PDF content, or raw engine output.

## Tagged Timestamp Gate

An ordinary local run deliberately permits `timestampTested: false`, because it should
not depend on a remote service. A tagged release is stricter. Its Windows, macOS, and
Linux jobs set:

```text
PAPERWORKS_REQUIRE_CERTIFICATE_TIMESTAMP=1
PAPERWORKS_TEST_TSA_URL=https://tsa.example.test
```

The URL must identify an HTTPS RFC 3161 service and cannot contain credentials, a
query, or a fragment. With the release policy enabled, a missing service, failed
timestamp, missing PAdES validation information, or `timestampTested: false` report
fails the job. The real release URL belongs in the protected GitHub environment as
`PAPERWORKS_TEST_TSA_URL`; never commit service credentials.

PowerShell for a manual timestamp-enabled run:

```powershell
$env:PAPERWORKS_REQUIRE_CERTIFICATE_TIMESTAMP = "1"
$env:PAPERWORKS_TEST_TSA_URL = "https://tsa.example.test"
npm run qa:certificate-engine
```

macOS or Linux:

```bash
export PAPERWORKS_REQUIRE_CERTIFICATE_TIMESTAMP=1
export PAPERWORKS_TEST_TSA_URL=https://tsa.example.test
npm run qa:certificate-engine
```

## Custom Corpus

For institutional or algorithm-specific testing, create a private directory outside
the repository containing:

```text
certificate-corpus/
  encrypted.pdf
  pdf-password.txt
  unsigned.pdf
  signer.p12
  passphrase.txt
  trust-root.pem
```

`unsigned.pdf` must be an ordinary PDF suitable for a visible field on page 1.
`encrypted.pdf` must be a standard password-protected PDF readable with the exact value
in `pdf-password.txt`. `signer.p12` must contain a disposable signing certificate and
private key, its passphrase belongs in `passphrase.txt`, and `trust-root.pem` must
validate the signing chain. Text secret files may have one final line ending.

Run the native test directly after setting `PAPERWORKS_CERTIFICATE_CORPUS` and,
optionally, `PAPERWORKS_TEST_TSA_URL`:

```bash
cargo test --manifest-path src-tauri/Cargo.toml live_certificate_corpus -- --ignored --nocapture --test-threads=1
```

Keep the directory user-private. Never use a production identity, upload the fixture as
CI evidence, or commit it to Git.

## Evidence Boundary

A passing generated report proves the recorded engine pair, fixture, operating system,
and architecture. Tagged evidence must be retained separately for Windows, macOS, and
Linux and must show `timestampTested: true`. Live tampered-signature, revoked,
expired-certificate, timestamp-service failure, alternative encryption-handler, and
very-large-PDF cases remain distinct release checks before the certificate workflow can
lose its experimental label.
