# Kill process

Search running processes and force quit, restart or terminate them, with children, after confirmation.

## User-visible behaviour
- A searchable list of running processes with CPU and memory use.
- Actions: end the process, force kill, restart (relaunch the app), and end the whole process tree.
- Always asks for confirmation before ending a process; system-critical processes are protected.
- Also reachable from the Command Bar (rows follow the sort of the process page) and from app-usage lists ("Force kill" appears when the feature is installed).
- Off until installed from Features.

## Settings
- Enable (default off); confirm before ending (default on); protect system processes (default on)

## Default shortcuts
None. Command Bar keyword.

## Edge cases
- Restart only for apps that can be relaunched from a known path.
- Elevated processes need elevation; Notifi reports why a kill failed.

## Permissions
None for the user's own processes.

## Platform notes
- Windows: TerminateProcess and job objects for trees. Linux: SIGTERM then SIGKILL and process groups.
- Status: Windows PORT, Linux PORT.

Sources: https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/README.md, https://raw.githubusercontent.com/vorssaint/vorssaint-utils/main/CHANGELOG.md
