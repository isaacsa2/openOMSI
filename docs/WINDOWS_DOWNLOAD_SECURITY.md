# Windows downloads, antivirus warnings and code signing

An antivirus **detection** is not the same as a Windows SmartScreen **reputation warning**. Do not assume either is harmless.

## Before running the game

1. Download the exact Windows ZIP from [the official openOMSI releases](https://github.com/openOMSI-org/openOMSI/releases); not a mirror, an attachment posted in chat, or a third-party launcher.
2. Open the release's **Assets** and find the SHA-256 shown by GitHub. You can also retrieve the same `digest` through the GitHub Releases API. Compare it to your downloaded ZIP:

   ```powershell
   Get-FileHash -Algorithm SHA256 "C:\Downloads\openOMSI-VERSION-windows-x64.zip"
   ```

   A mismatch means **do not run it**. Keep in mind a matching hash proves correspondence with a particular GitHub release asset, not that its source or publisher is trustworthy.
3. Inspect the files after extraction:

   ```powershell
   Get-AuthenticodeSignature "C:\Games\openOMSI\openomsi.exe" |
     Select-Object Status, SignerCertificate
   ```

   `NotSigned` is expected for releases made **without** a configured code-signing certificate. It does not prove a file is malicious or safe. A valid signer signature is useful evidence, not a malware scan.
4. If Microsoft Defender identifies a threat, open **Windows Security → Virus & threat protection → Protection history** and record the **detection name**, **affected file path**, openOMSI **release version** and the file's **SHA-256**. Do not disable protection, restore a quarantined executable or add the whole game directory to exclusions while the detection is unresolved.
5. Report a suspected *false positive* to [Microsoft Security Intelligence](https://www.microsoft.com/en-us/wdsi/filesubmission). Include the exact affected sample **only when you are comfortable sharing it**. For an actual security vulnerability, follow the [private security reporting policy](../SECURITY.md); do not publish an exploitable report as a public issue.

The project previously received reports about a virus warning on manual downloads ([issue #1753](https://github.com/openOMSI-org/openOMSI/issues/1753)). The updater's DLL replacement bug was separately fixed; that does not, by itself, prove the alert false.

## Official Windows release signing

A trusted **Authenticode code-signing certificate** is required to build publisher reputation and sign `openomsi.exe`. Open source alone does not bypass SmartScreen. The Windows release workflow supports an optional signing step on trusted `main` builds and explicitly **never signs pull-request builds**.

The release owner can configure these GitHub Actions secrets:

- `WINDOWS_CODESIGN_PFX_B64`: base64 of the PKCS#12/PFX signing certificate **including the private key**.
- `WINDOWS_CODESIGN_PFX_PASSWORD`: password protecting the PFX.

For sensitive production signing keys, a managed hardware-backed signing service / HSM is preferable to storing an exportable PFX in CI. Only the owner of the signing identity should configure access.

When both secrets exist, the Windows build signs **first-party EXEs only** using Microsoft's SignTool with SHA-256 and an RFC 3161 timestamp, verifies each signature, and fails the build if signing fails. Vendor DLLs are **not** re-signed. When the secrets are missing, the workflow publishes **unsigned** builds with a visible warning; it makes no claim that SmartScreen warnings will disappear.

## Download and update integrity

- The game's updater checks the GitHub-provided SHA-256 before installing a release and re-hashes cached archives before reuse.
- When the GitHub API cannot provide a release checksum, automatic installation refuses the unverified archive and tries again on the next update check. Players can use the official releases page after independently checking the asset.
- The Windows packaging workflow pins the DXC ZIP to a specific Microsoft release and checks its SHA-256 before copying the compiler DLLs.

These controls mitigate tampering in transit, accidental cache corruption and unpinned binary dependencies. They cannot certify that an antivirus alert is false or replace independent malware analysis.

## Additional strict verification in official builds

- Release archives have a strict maximum size and a restricted official GitHub download path (unless the operator explicitly sets the developer override `OMSI_UPDATE_URL`).
- The updater enforces a maximum number of ZIP entries, rejects duplicate archive paths and refuses excessive decompressed sizes. It checks cached files again immediately before installation.
- Windows release packages check an allowlist of expected executable and DLL filenames. The checked-in Steam redistributable is pinned by its Git blob identifier, so any binary replacement needs a source review.
- Official releases also publish `SHA256SUMS.txt` for convenient cross-checking; GitHub's individual asset digest remains an additional source for comparison.

These checks are intended to fail closed when a release has unexpected contents. Release maintainers must update the allowlist or pinned Git object only after independently verifying a legitimate dependency change.
