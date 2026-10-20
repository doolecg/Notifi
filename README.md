# Notifi

A tray utility suite for Windows and Linux: sound, system monitor, window tools, keyboard and mouse tweaks, clipboard, capture, app management, energy and display. Turn on only the features you use. Built with Tauri 2, Rust and Svelte.

## Clean room

Notifi is a clean-room work. It is written only from behaviour specs in `spec/`, which come from public docs and screenshots. No source code from other projects is read or copied. Name, icon and look are Notifi's own.

## Build

Needs Rust (stable), Node 20+ and the [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```sh
npm install
npm run dev     # run with hot reload
npm run build   # release build and installers
```

Set `NOTIFI_NO_FOCUS=1` to make windows open without taking focus (handy for test runs).

## Licence

MIT, see `LICENSE`.
