# Crash logs

Notifi keeps a short log when it hits a problem, so you can report it.

## User-visible behaviour
- A Rust panic, or an unhandled Windows exception, writes a text file to the `crashes` folder in Notifi's config folder (`%APPDATA%\io.notifi.app\crashes`). It holds the version, system, time, the message and a backtrace.
- At most one log in any 5 seconds, and the newest 10 are kept.
- Twenty seconds after the next start, if a log hasn't been reported or dismissed, the Island shows "Notifi hit a problem last time" and clicking it opens Settings, About.
- Settings, About shows the newest unreported crash with Report and Dismiss.
- Report opens a new GitHub issue in the browser (title and body filled in, label `crash`) and marks the logs as reported. The log has the user name and home folder replaced with `<user>`, and is cut to about 2000 characters. The issue is created only if you submit it there.
- Dismiss marks the logs as reported without opening anything.

## Not covered
- Crashes inside WebView2's own processes, and the app being killed from outside.
- Backtraces show function names only where the build has symbols; the panic message and file and line always show.
