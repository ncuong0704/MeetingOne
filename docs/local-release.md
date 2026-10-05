# Local Windows releases

Run from the repository root in PowerShell with Node, pnpm 9.15.9, Rust and the Visual Studio C++ build tools installed:

```powershell
pnpm --dir frontend install --frozen-lockfile
.\release.ps1 -Version 0.1.2
```

Provide the existing updater signing key through `TAURI_SIGNING_PRIVATE_KEY` (or `TAURI_SIGNING_PRIVATE_KEY_PATH`) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The script also reads quoted assignments for those two variables from ignored local `.env` files or `NOTES.md`. It never executes those files or prints their contents. Keep the key compatible with the public key shipped in previous app versions.

The script updates the app versions, builds locally, packages the required native runtime DLLs, produces both Windows installers and verifies their updater signatures. The five upload assets are staged in `.release-v<version>/artifacts/`: EXE, MSI, their `.sig` files and `latest.json`. Generated files and local signing notes are ignored by Git. Authenticode signing is separate and requires the certificate setup used by `frontend/src-tauri/scripts/sign-windows.ps1`.

Use `-SkipBuild` only after a successful local build of the exact sources being released. The executable version must match the requested release version. Check the installers on a Windows test machine and run the recording/recovery acceptance checks in [development-quality.md](development-quality.md) before distributing them broadly.

Commit the release sources, tag that commit and push the tag. Create a GitHub release with all five verified assets, inspect the uploaded assets, then publish it as the latest release. The updater endpoint reads `latest.json` from the latest GitHub release; its download URLs must match the uploaded filenames. `release.ps1` does not push code, publish a release or trigger a GitHub Actions installer build.

Draft releases and prereleases are not returned by the public latest-release endpoint. After publication, verify both `https://api.github.com/repos/ncuong0704/MeetingOne/releases/latest` and the public updater manifest resolve to the new version. Apps only offer updates to a strictly newer version; an app already running the same version will need a manual reinstall if its draft installer was replaced before publication.
