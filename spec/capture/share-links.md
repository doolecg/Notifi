# Share links (temporary links for captures)

Upload a screenshot or recording and copy a link that expires on its own. Vorssaint uses its own online service; Notifi has no public service. Instead it ships a tiny open-source share server the user hosts (for example in a Docker container on their own server) and enters its address in settings.

## User-visible behaviour
- From the screenshot preview or editor, and from the recording editor and finished recordings, a Share link action uploads the file and copies the link.
- Expiry choices: 1 hour, 6 hours, 24 hours for screenshots; 1 hour or 6 hours for recordings (recordings are first compressed locally to under a size limit, default 100 MB).
- A separate shortcut (off by default) uploads the latest screenshot and copies the link; it never uploads an edited-then-discarded capture.
- The server deletes the file when the expiry passes. The link page shows the item and says when it expires.
- Editing or discarding a capture, or turning links off, cancels a pending upload so a late result never copies a link to the original. If an upload finished after cancellation, Notifi asks the server to revoke it when reachable.
- A privacy panel explains what is sent (only the file and the chosen expiry) and that the server is the user's own.
- If no server is set, the actions are hidden and the settings page shows how to set one up, with a "Test connection" button.

## Settings
- Server address and optional access token (stored in the OS credential store)
- Default expiry (default 1 hour)
- Compress recordings to under (default 100 MB)
- Enable temporary links (default off until a server is set)
- Shortcut for latest screenshot (default none)

## Default shortcuts
None. (Vorssaint's default for the latest-screenshot link is off.)

## Edge cases
- Upload failures keep the local capture and show a retry button.
- Oversize files are re-compressed or refused with a message.
- Tokens are never logged or included in backups.
- Offline: the share action is disabled with a reason.

## Permissions
Network access to the user's server only.

## Platform notes
- Same on Windows and Linux: HTTP upload from the Rust core.
- The server is a separate repository/image in Notifi's organisation (MIT), configured with an upload size limit, expiry choices and a cleanup job.
- Status: Windows PARTIAL, Linux PARTIAL (works only with a self-hosted server; no public service is built in).

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
