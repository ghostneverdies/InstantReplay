# Security

Instant Replay is a free, open-source screen recorder. This document describes what it does on your system.

## What the app does

- Captures the screen and audio continuously while recording is running, and keeps the most recent 30, 60 or 120 seconds in memory.
- Writes an MP4 to disk only when you save a replay (hotkey, button or tray menu).
- Records the whole desktop of the monitor it captures, not individual windows.
- Records system audio and, if enabled, the selected microphone.
- Registers one global hotkey (default `Ctrl + Numpad *`). It does not install a keyboard hook and does not read keystrokes.
- Runs as a normal user process (`asInvoker`), with a tray icon.
- Allows a single instance. A background watchdog terminates any other `InstantReplay.exe` process it finds.
- Optionally starts with Windows through a per-user `Run` registry entry (`--tray`).
- Shows a memory warning if usage stays high, and writes a log file to `%TEMP%`.

## What the app does not do

- No network code. It does not upload, send or receive data.
- No telemetry, analytics or tracking.
- No background services or scheduled tasks.
- No modification of system files or Windows settings beyond its own registry entries.
- No bundled third-party installers or runtimes. FFmpeg, Java and virtual audio drivers are not used.

The only links it opens are to the project's GitHub page in your browser (title-bar button; the installer also opens it after a fresh install).

## Data on disk

| Location | Contents |
|---|---|
| `%APPDATA%\InstantReplay\settings.json` | Settings |
| `Pictures\Instant Replay` (default) or your chosen folder | Saved replays and thumbnails |
| `%TEMP%\debug_<timestamp>.txt` | Log file |

## Installer

`InstantReplaySetup.exe` is built with NSIS from `installer/installer.nsi` and requires administrator rights. It:

1. Copies the app to `Program Files\Instant Replay`
2. Registers an Instant Replay uninstall entry and an `.ir` file type entry under `HKLM`
3. Adds a start-with-Windows entry under `HKCU`
4. Creates Start Menu and optional desktop shortcuts
5. Opens the project's GitHub page after a fresh install

The uninstaller stops the app, removes the registry entries, shortcuts and install folder, and can optionally delete recordings in the default `Pictures\Instant Replay` folder. It leaves `%APPDATA%\InstantReplay` in place.

## Why antivirus software may flag it

Screen capture, a global hotkey, a startup entry, and a process that terminates its own duplicates are patterns also seen in malware. Everything the app does is in this repository.

## Reporting

Open an issue on the GitHub repository.