# macOS signing and notarisation

The release workflow signs every macOS binary with a Developer ID certificate
and has Apple notarise it, so Gatekeeper opens a downloaded emulator without
the "cannot be opened because the developer cannot be verified" block. It does
this only when all six secrets below exist on `emu198x/emu198x`. With none of
them it builds unsigned binaries, as releases up to v0.25.0 did; with some but
not all it fails, so a half-configured release cannot ship.

## How it works

`release.yml` reads this file's secret names; keep the two in step.

1. **Configure macOS signing** checks the six secrets. When all exist it
   exports `CODESIGN_IDENTITY`, `CODESIGN_CERTIFICATE`,
   `CODESIGN_CERTIFICATE_PASSWORD` and `CODESIGN_OPTIONS=runtime` for dist.
2. **Build artifacts** runs `dist build`. With `macos-sign = true` in
   `Cargo.toml` and those variables set, dist imports the certificate into a
   temporary keychain and runs `codesign --sign <identity> --options runtime`
   on each binary *before* archiving it, so the shipped `.tar.gz` holds signed
   binaries.
3. **Notarise macOS binaries** unpacks this target's archives and fails unless
   every binary has a Developer ID Application signature, the hardened-runtime
   flag and a secure timestamp. It zips the binaries and runs
   `xcrun notarytool submit --wait`; anything but `Accepted` prints
   `notarytool log` and fails the release.
4. **Remove signing keychains** drops the temporary keychains from the
   runner's search list.

Archives are not changed after signing, so dist's checksums stay valid. A bare
executable cannot carry a stapled ticket, so Gatekeeper fetches the
notarisation ticket from Apple on first launch; that needs a network
connection once.

### Why dist signs and the workflow notarises

dist 0.32 signs macOS binaries natively but does not notarise (its
`sign/macos.rs` says notarisation is future work). Signing has to happen inside
`dist build`, because dist archives and checksums the binaries in the same
command; signing afterwards would mean repacking every archive and rewriting
dist's manifests.

dist's `codesign` call takes no `--timestamp`. `codesign` asks Apple's
timestamp server by default for Developer ID signatures, and the notarise
step's check and Apple's own check both reject a binary without a secure
timestamp, so a missing timestamp fails the release instead of shipping.

dist's generated `release.yml` would set the `CODESIGN_*` variables for the
whole job straight from the secrets. A missing secret then arrives as an empty
string, which dist treats as set, and the build fails trying to sign. The
hand-written configure step exists to avoid that; keep it if `release.yml` is
ever regenerated.

### No entitlements

The hardened runtime blocks JIT, unsigned executable memory, `DYLD_*`
variables, loading dylibs signed by another team, and microphone, camera and
similar protected resources unless an entitlement allows them. The emulators
need none of these:

- **Graphics (wgpu on Metal)** and **audio output (cpal on CoreAudio)** use
  system frameworks, which the hardened runtime allows.
- **Audio input:** nothing in the workspace opens an input stream, so no
  microphone entitlement.
- **Gamepads (gilrs on IOKit HID):** not a hardened-runtime resource.
- **Network:** the ESP AT modem and Ultimate UCI network peripherals open
  outgoing TCP/UDP sockets, which need no entitlement outside the App Sandbox;
  these binaries are not sandboxed. MCP runs over stdio, not a socket.
- **JIT:** none; every CPU core is an interpreter.
- **Third-party dylibs:** none are loaded, so library validation stays on.

A hardened-runtime, ad-hoc signed `emu198x-spectrum` ran its 48K ROM headless
to the copyright screen with screenshot and audio capture, and ran windowed for
six seconds without an error. Add an entitlements plist only when a feature needs one; dist
0.32 has no option to pass it, so that would mean signing in the workflow.

## Secrets

Create these as **repository** secrets on `emu198x/emu198x` (Settings →
Secrets and variables → Actions). Release builds run only on tag pushes, so
pull requests never need them.

| Secret | Value |
|---|---|
| `CODESIGN_CERTIFICATE` | The Developer ID Application certificate and its private key as a `.p12`, base64-encoded |
| `CODESIGN_CERTIFICATE_PASSWORD` | The password set when exporting the `.p12` |
| `CODESIGN_IDENTITY` | The certificate's full name, e.g. `Developer ID Application: Steve Hill (ABCDE12345)` |
| `NOTARYTOOL_KEY` | The full text of the App Store Connect API key file `AuthKey_<KEYID>.p8`, including the `BEGIN`/`END` lines |
| `NOTARYTOOL_KEY_ID` | The key's ID, e.g. `2X9R4HXF34` |
| `NOTARYTOOL_ISSUER` | The issuer ID shown above the keys table, a UUID |

### Export the Developer ID Application certificate

If you do not have one yet, create it in Xcode (Settings → Accounts → your
team → Manage Certificates → + → Developer ID Application). Only the Account
Holder can create Developer ID certificates.

1. Open Keychain Access, select the **login** keychain and the **My
   Certificates** tab.
2. Find **Developer ID Application: …** and expand it to check a private key
   sits under it. Without the key the export cannot sign anything.
3. Right-click the certificate (not the key) → Export → format **Personal
   Information Exchange (.p12)** → save as `DeveloperID.p12` and set a strong
   password.
4. Read the exact identity name:

   ```sh
   security find-identity -v -p codesigning
   ```

5. Store the three signing secrets (the password and identity commands prompt
   for the value):

   ```sh
   base64 -i DeveloperID.p12 | gh secret set CODESIGN_CERTIFICATE -R emu198x/emu198x
   gh secret set CODESIGN_CERTIFICATE_PASSWORD -R emu198x/emu198x
   gh secret set CODESIGN_IDENTITY -R emu198x/emu198x
   ```

6. Delete `DeveloperID.p12`.

### Create the App Store Connect API key

1. In [App Store Connect](https://appstoreconnect.apple.com), go to **Users and
   Access → Integrations → App Store Connect API → Team Keys**. The first time,
   the Account Holder has to request access.
2. Click **+** (Generate API Key), name it `emu198x notarisation`, and give it
   the **Developer** role, the least that can notarise.
3. Download `AuthKey_<KEYID>.p8`. Apple offers the download once only.
4. Note the **Key ID** in the key's row and the **Issuer ID** above the table.
5. Store the three notarisation secrets:

   ```sh
   gh secret set NOTARYTOOL_KEY -R emu198x/emu198x < AuthKey_<KEYID>.p8
   gh secret set NOTARYTOOL_KEY_ID -R emu198x/emu198x
   gh secret set NOTARYTOOL_ISSUER -R emu198x/emu198x
   ```

6. Keep the `.p8` somewhere safe or delete it; a lost key is revoked and
   replaced, not recovered.

## Verify a release

In the release run, each macOS `build-local-artifacts` job should log
`30 signed binaries verified` (one per emulator in that release) and
`Notarised: submission <id>`.

Then check the result as a user gets it:

1. On a Mac, download a macOS `.tar.gz` from the GitHub release **in a
   browser**. Only a browser download carries the quarantine flag that makes
   Gatekeeper check the file; `curl` and `gh release download` skip the check.
2. Double-click the archive to extract it.
3. Check the binary:

   ```sh
   spctl -a -vv -t open --context context:primary-signature emu198x-spectrum
   ```

   Use the `open` context, not `-t exec`: `-t exec` assesses only `.app`
   bundles and rejects any bare executable, notarised or not, with "the code
   is valid but does not seem to be an app".

   It should report:

   ```text
   emu198x-spectrum: accepted
   source=Notarized Developer ID
   origin=Developer ID Application: …
   ```

   `codesign -dvv emu198x-spectrum` should show `flags=0x10000(runtime)` and a
   `Timestamp=` line.
4. Run it from Finder or Terminal; it should open without a Gatekeeper prompt.

`rejected` or `source=Unnotarized Developer ID` means notarisation did not
register: check the run's notarise step output and `notarytool log`.
