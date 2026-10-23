---
name: release
description: Cut and publish a Notifi release to GitHub (version bump, release notes, signed MSI, tag, GitHub release). Use when asked to release, publish or ship a new Notifi version.
---

# Release Notifi

Releases are built and signed on the maintainer's PC and published to `doolecg/Notifi` from `main`. The in-app updater reads the latest GitHub release, so publishing is what ships it to users. Only do this when the user asks for a release in this turn. `release.yml` in CI is a build check only and publishes nothing.

Past releases (0.3.0, 0.3.1) are the pattern: tag `X.Y.Z` (no `v`), title `Notifi X.Y.Z`, target `main`, body is that version's section of `RELEASE_NOTES.md` without its `# Notifi X.Y.Z` heading, and two assets: `Notifi_X.Y.Z_x64_en-US.msi` and `Notifi_X.Y.Z_x64_en-US.msi.sig`.

## Steps

1. **Version.** Set it in all of: `package.json`, `package-lock.json` (top two `"version"` lines), `[workspace.package]` in `Cargo.toml`, `src-tauri/tauri.conf.json`, and the `notifi`, `notifi-core` and `notifi-platform` entries in `Cargo.lock`. They must match.
2. **Notes.** Add a `# Notifi X.Y.Z` section at the top of `RELEASE_NOTES.md` (above the previous one, separated by `---`) with `## New`, `## Changed`, `## Fixed` as needed. Plain, user-facing wording. Only claim what was checked; leave out anything not verified.
3. **Checks.** `npm run check` and `cargo test --workspace` must pass. Quote the result.
4. **Commit** on the dev branch (`dev-X.Y.Z`). Author and committer are `doolecg <doolecg@gmail.com>` (already the git config). No Claude or AI attribution, no `Co-Authored-By`. Message: short, plain, `X.Y.Z: <what changed>`. Do not stage `.claude/settings.local.json`, `.codegraph/` or `.env`.
5. **Build.** `npm run build` writes `target/release/bundle/msi/Notifi_X.Y.Z_x64_en-US.msi`.
6. **Sign.** `node scripts/sign-update.mjs target/release/bundle/msi/Notifi_X.Y.Z_x64_en-US.msi` writes the `.sig` beside it, using `NOTIFI_UPDATE_KEY` from the git-ignored `.env`. Never print or commit the key. Without a signature the app refuses to install the update.
7. **Merge into `main`** and push: `git checkout main`, `git merge --ff-only dev-X.Y.Z`, `git push origin main`. Also push the dev branch: `git push -u origin dev-X.Y.Z`. If the fast-forward fails, stop and tell the user rather than forcing.
8. **Publish.** Put the notes body in a temp file (the section without its heading), then:
   ```sh
   gh release create X.Y.Z target/release/bundle/msi/Notifi_X.Y.Z_x64_en-US.msi target/release/bundle/msi/Notifi_X.Y.Z_x64_en-US.msi.sig \
     --target main --title "Notifi X.Y.Z" --notes-file <file>
   ```
   The PR and release body carry no AI attribution lines.
9. **Verify.** `gh release view X.Y.Z --json tagName,targetCommitish,assets` shows both assets in state `uploaded`, and the `.msi` digest matches `sha256sum` of the local file. Then switch back to the dev branch.
10. **Next dev branch.** Optionally `git checkout -b dev-X.Y.(Z+1)` from `main`.

## Rules

- The release is public and updates users' installs. If a step fails, stop and report; do not retry destructive steps (no force-push, no deleting tags or releases) without being asked.
- Do not sign anything but the MSI built from the commit being released.
- Tell the user what was not checked, e.g. a real signed update end to end.
