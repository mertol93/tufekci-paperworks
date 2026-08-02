# Signed Application Updates

Tüfekci Paperworks has a local, user-triggered update client for official release
builds. It does not check in the background. Selecting **Updates** and then **Check for
updates** requests the manifest for the channel embedded in that build. Tauri verifies
the downloaded updater artefact against the embedded Minisign public key; signature
verification cannot be bypassed by the interface.

This updater applies only to Windows, macOS, and Linux. iOS excludes the updater
dependency and plug-in at compile time; its Updates surface reports that delivery is
managed by the App Store and offers no custom channel, endpoint, package download, or
in-app restart operation.

Ordinary development and source builds do not contain an endpoint or public key and
remain offline. Updater signatures are separate from Windows publisher signing and
macOS signing and notarisation. Tagged builds enforce all three credential contracts;
the first real certificate-backed package and update evidence remains a release gate.

## Channels

The release version determines exactly one channel:

| Version | Channel | Mutable manifest release |
| --- | --- | --- |
| `x.y.z-alpha.n` | Alpha | `updates-alpha` |
| `x.y.z-beta.n` or `x.y.z-rc.n` | Beta | `updates-beta` |
| `x.y.z` | Stable | `updates-stable` |

The application endpoint is
`https://github.com/<owner>/<repository>/releases/download/updates-<channel>/latest.json`.
The manifest points back to signed artefacts on the immutable `v<version>` release.

## One-Time Setup

1. Generate a Tauri updater signing key outside the repository and store its private
   key and password in an offline recovery location. Never commit either value.

   ```bash
   npm run tauri signer generate -- -w ~/.tauri/tufekci-paperworks.key
   ```

   On Windows PowerShell, use
   `npm.cmd run tauri signer generate -- -w "$HOME\.tauri\tufekci-paperworks.key"`.

2. Create a protected GitHub Environment named `updater-signing` with required
   reviewers. Add `PAPERWORKS_UPDATE_PUBLIC_KEY` as an environment variable and
   `TAURI_SIGNING_PRIVATE_KEY` plus `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` as environment
   secrets. Use the complete single-line public-key value produced by Tauri. It is a
   base64-wrapped Minisign document; do not substitute its decoded `RW...` line.
   Add the platform publisher variables and secrets documented in
   [Publisher Signing](BUILD.md#publisher-signing) to the same protected environment.
3. Create a protected GitHub Environment named `updater-promotion` with required
   reviewers. It needs no updater private key; its approval governs movement of an
   already signed and validated manifest onto a channel.
4. Restrict release and environment administration, protect release tags, and retain a
   tested offline copy of the signing material.

The updater private key and each platform's publisher credentials are supplied only to
the matching tagged native build steps. The generated
`src-tauri/updater.release.conf.json` contains only
`bundle.createUpdaterArtifacts: true`; the separate platform overlay contains public
signer settings only. Both are ignored by Git and never archived.

## Tagged Release

Pushing the exact `v<version>` tag runs `.github/workflows/release.yml`:

1. The preflight validates every project version, the exact tag, the Windows package
   version, and the derived update channel.
2. Each native build enters the protected `updater-signing` environment. Missing or
   malformed updater or matching platform publisher credentials fail before compilation.
3. Platform builds run serially while Tauri Action creates signed updater artefacts and
   assembles `latest.json` on the immutable draft release.
4. The metadata job downloads that manifest, requires all supported platform families,
   checks bounded Minisign signatures, and binds GitHub API asset IDs to the exact
   immutable release inventory before retaining a path-free hash report. Cryptographic
   package verification still occurs in the installed Tauri client before installation.
5. Release maintainers review all package, OCR, PDF/A, certificate, source, licence,
   SBOM, updater, signing, notarisation, and installation evidence before publishing the immutable
   release. A draft cannot be promoted.

## Channel Promotion

Run **Promote signed update channel** manually with the published immutable release tag
and its derived channel. The `updater-promotion` environment must be approved. The
workflow revalidates the source release and signed-update manifest, publishes only
`latest.json` and path-free evidence to `updates-<channel>`, then downloads the channel
asset and requires it to be byte-identical to the reviewed source.

Promotion never rebuilds or re-signs an application. Stable promotion must use a stable
version; alpha, beta, and release-candidate versions cannot be placed on the stable
channel.

## Rollback and Withdrawal

If an update must stop immediately, remove the channel manifest asset while the incident
is reviewed:

```bash
gh release delete-asset updates-alpha latest.json --yes
```

Use the affected channel tag for beta or stable. Removing the pointer stops new checks
from finding the release; it does not alter installations already completed.

The safe automatic recovery is a **higher-version** signed release that reverts or fixes
the fault, followed by ordinary reviewed promotion. Do not publish a lower-version
manifest, weaken signature verification, replace immutable release artefacts, or reuse a
version. If the installed application cannot launch, users must manually reinstall a
reviewed immutable package. Record the incident, affected channel, withdrawal time,
replacement version, and retained promotion evidence.

## Key Rotation and Loss

For planned rotation, build a bridge release signed by the old private key while
embedding the new public key. Promote it with the normal review, allow supported
installations time to migrate, then sign later releases with the new private key. Test
this sequence on all three operating systems before production use.

If the private key is lost, existing installations cannot safely trust a replacement
key through the updater. Withdraw every channel manifest, revoke the CI secret, create a
new key, and distribute a newly platform-signed application for manual installation. If
the key may be compromised, also restrict release write access immediately and treat all
channel and immutable release assets as incident evidence. Never add an unsigned or
accept-any-key recovery path.

## Release Evidence Gate

The updater infrastructure is implemented, but the release-plan item remains open until
a real credential-backed Windows, universal macOS, and Linux tagged build has passed;
the immutable release has been published; its exact manifest has been approved and
promoted; update and restart have been exercised on each supported platform; and a
withdrawal plus higher-version rollback rehearsal has retained non-sensitive evidence.
