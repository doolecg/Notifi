# AI chat on the bar

Written for Notifi. It borrows no code or text from any chat app.

## What it does

- An AI chat tab on the Island's tab strip opens a chat inside the Island itself, in the tall menu. A chat button on the taskbar AI Agents bar and a shortcut action ("Island: AI chat") open the Island on that tab.
- The chat is the host app's own web page, so sessions, the project picker, model and effort, the chat itself, and the side panels (files, diffs, artifacts, other chats) are the host's, not copies.
- While the chat tab is open, a dictate button on the strip starts Windows voice typing in it.
- When an AI question is waiting, its menu has a "Prompt chat" button that switches to the chat, and the chat has a "Questions (n)" button that goes back.
- The chat comes back to the chat that was open last, when it is still there.

## Hosts

- Version 1 supports T3 Code only. The host list is built so others can be added: a host says how to find its local address and how to sign in.
- T3 Code is found from its `userdata/server-runtime.json` (in `T3CODE_HOME`, else `~/.t3`). Its address must be on this computer (127.0.0.1 or localhost). The server must answer at `/.well-known/t3/environment`.
- Not built yet: Operant, OpenCode, the Claude app and the Claude Code, OpenCode and other command line versions.

## Sign-in

- The first time the chat opens in a run of Notifi, Notifi runs T3 Code's own `t3 auth pairing create --json` and opens the pairing link it returns. The link works once and lasts 2 minutes. Notifi stores no password and no token.
- That pairing gives the chat the same access T3 Code has: projects, files, terminals. It is the same as pairing a phone. Remove it any time from the connections list in T3 Code.

## How it is shown

- The page is a second webview inside the Island's window, laid over an empty area of the tall menu, not an iframe: the host's sign-in cookie is not kept in a frame.
- It gets no access to Notifi's own commands. It is hidden when the Island closes or the tab changes.
- The menu stays open while the pointer is over the chat.

## Rules

- Off until the user turns on "AI chat button".
- If T3 Code is not installed or not running, the chat area says so with a Try again button. Notifi does not start it.
- Dictation sends the Windows voice typing shortcut (Win+H) to the chat. It needs Windows 10 or 11, online speech recognition turned on, and a microphone. Microsoft processes the speech. On other systems it does nothing.

## Not built

- Turning the resting pill itself into the prompt box.
- A compact "prompt plus last message" strip.
- Choosing the host in Notifi's settings (only T3 Code exists).
