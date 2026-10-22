# Notifi

Notifi turns the top of your Windows screen into a small control centre: music, timers, AI agents, a chat and a set of desktop tools, all from one pill that drops open when you need it.

![The Notifi Island open on the Nook: now playing, calendar, notifications and scratchpad](docs/images/island.png)

The Island open on the Nook: what is playing, the calendar, recent notifications and a scratchpad.

## Get started in about 30 seconds

Needs Windows 10 or 11 (64-bit).

1. Download `Notifi_<version>_x64_en-US.msi` from the [latest release](https://github.com/doolecg/Notifi/releases/latest).
2. Run it. Notifi starts in the tray and the pill appears at the top of your screen.
3. Click the pill to open it, or press `Ctrl+Alt+P`.

That's it. Notifi keeps itself up to date from GitHub releases, and the tray icon opens the settings, where every feature can be turned off.

## What it does

| Capability | What you get |
| --- | --- |
| The Island | A pill (at the top of the screen, or on the taskbar) that opens into tabs: Nook, Now Playing, Timer, Volume, AI Agents and a module grid. When idle it shows the time, the weather, a notification count, quick AI prompt buttons and CPU, memory and AI meters. |
| AI agents | Working, needs-you and done states for Claude Code, Codex, OpenCode and Copilot CLI sessions, with session time and plan use. |
| AI chat | T3 Code's own chat inside the Island, opened on the chat you used last, with Escape to close and Windows voice typing. |
| Media | Now playing with controls, time-synced lyrics, volume and brightness sliders, and an app volume mixer. |
| Desktop tools | Clipboard history, screenshots and screen text, a colour picker, window snapping, a command bar and an app switcher. |
| System | CPU, memory, GPU, disk and network readouts, a speed test, energy and display controls. |
| Apps | Installed apps and their updates through winget and Scoop, with a cleaner for what an uninstall leaves behind. |
| Settings | Every option in one window, applied as you change it, with shortcuts you can rebind. |

## Common commands

```sh
npm install                 # once, after cloning
npm run dev                 # run the app with live reload
npm run build               # release build and the MSI installer
npm run check               # type-check the interface
cargo test --manifest-path src-tauri/Cargo.toml   # Rust tests
```

## Install

- **Windows installer (recommended).** The MSI from the [releases page](https://github.com/doolecg/Notifi/releases). It updates itself after that.
- **From source.** Needs Rust (stable), Node 20 or newer and the [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/). Clone the repo, then `npm install` and `npm run build`; the installer is written to `target/release/bundle/msi/`.

Linux (KDE and GNOME, X11 and Wayland) builds from source; there is no Linux download yet.

## Using it with other tools

- **T3 Code.** The AI chat shows T3 Code running on your own PC (it talks to `127.0.0.1` only). Install and start T3 Code, then turn on the AI chat button in the Island settings.
- **Claude Code and other agents.** The AI Agents tab reads the session files those tools already write on your PC. Nothing needs installing in them.

## Configuration

Settings are saved in `%APPDATA%\io.notifi.app\settings.json` and are best changed from the settings window, which applies them straight away.

```jsonc
{
  "general": {
    "autoUpdate": true,       // check GitHub and download new versions in the background
    "updateCheckHours": 3,    // how often to check while Notifi runs
    "installOnQuit": true     // install a downloaded update when you quit
  },
  "shortcuts": {
    "islandToggle": "Ctrl+Alt+P"
  }
}
```

The one people hit first: a shortcut another app already uses cannot be taken. The settings window flags it; pick another.

## Privacy

Most of Notifi works only on your PC. These are the times it uses the internet:

- **Updates.** Asks GitHub for the newest release every few hours and downloads it. Turn off "Update automatically" to stop this.
- **Lyrics.** Sends the song title and artist to [LRCLIB](https://lrclib.net) when you open lyrics.
- **Claude plan limits.** Off until you turn it on. Then it sends the login Claude Code saved on your PC to Anthropic to read your 5-hour and weekly use. The login is read for each request and never stored by Notifi.
- **Weather.** Off until you type a city in the Island settings. Then the city name is sent to [Open-Meteo](https://open-meteo.com) (a free weather service, no account) to find the place and read the weather, every 15 minutes.
- **Speed test.** Talks to open [LibreSpeed](https://librespeed.org) servers, only when you start a test.
- **Share links.** Uploads a capture to the server you set, only when you ask it to.

Clipboard history, notifications, notes, captures and settings stay on your PC.

## Docs

- `RELEASE_NOTES.md`: what changed in each version.
- `docs/progress.md`: what was built, what was checked and what was not.
- `spec/`: how each feature is meant to behave.

## Contributing

Start with the spec for the feature in `spec/`, then the code it names. Before sending a change, run `npm run check` and the Rust tests above. A good change does one thing, keeps to the spec, and says what was tested in the running app.

## Credits

Notifi is modelled on [Vorssaint](https://vorssaint.com), a set of macOS menu-bar tools. It is a clean-room work: it was written only from behaviour specs drawn from Vorssaint's public pages and screenshots, and no Vorssaint code, images or text were used. Notifi's name, icon and look are its own.

It is built with [Tauri](https://tauri.app), Rust and [Svelte](https://svelte.dev), shows [T3 Code](https://github.com/pingdotgg/t3code) for its chat, gets lyrics from [LRCLIB](https://lrclib.net), weather from [Open-Meteo](https://open-meteo.com) and speed tests from [LibreSpeed](https://librespeed.org).

## Licence

MIT, see [LICENSE](LICENSE).

## Disclaimer

This software is provided "as is", without warranty of any kind, as set out in the [LICENSE](LICENSE). To the extent permitted by law, the authors are not liable for any loss or damage from using it, including lost data, lost work or damage to files or projects. Nothing here limits liability that cannot be limited by law, such as for death or personal injury caused by negligence.

Back up your work before using it. You use it at your own risk.

Notifi is not made by, affiliated with or endorsed by Vorssaint, Anthropic, OpenAI, GitHub, Microsoft, Spotify, Amazon or the makers of T3 Code or OpenCode. Their names and trademarks belong to their owners and are only used to say what Notifi works with. AI agents' output can be wrong; check it before relying on it. The app cleaner and app updates change installed software: read what they will do before you confirm.
