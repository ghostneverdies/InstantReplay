# Instant Replay

A Windows app that keeps the last 30, 60 or 120 seconds of your screen and audio in memory and saves them as an MP4 when you press a hotkey.

## Features

- Rolling in-memory replay buffer (30 / 60 / 120 seconds, capped at 512 MB of encoded video)
- Global save hotkey, default `Ctrl + Numpad *`, rebindable in Settings
- System audio and microphone recording, each switchable, with microphone selection
- Frame rate: 30, 60 or 120 FPS
- Quality presets: Fast, Balanced, Quality
- Capture method: Windows Graphics Capture or DXGI Desktop Duplication
- Video encoder: Auto, Hardware or Software (H.264)
- Gallery of saved replays with thumbnails, built-in player, full-screen playback and "show in folder"
- System tray icon, toast notifications, close-to-tray, single instance
- Optional start with Windows (launches minimized to tray)
- Dark / light mode with Mica or Acrylic backdrop
- Warns when the app's memory use stays unusually high

## Requirements

- Windows 10 version 2004 (build 19041) or later, x64
- .NET 8 Desktop Runtime (the build is framework-dependent)
- WebView2 Runtime (used by the media player)

## Install

Run `InstantReplaySetup.exe`. It installs to `Program Files\Instant Replay`, creates Start Menu and optional desktop shortcuts, and registers start with Windows. Nothing else is downloaded or installed.

## Build

Requires the .NET 8 SDK and the Rust toolchain.

```
cd engine\rust
cargo build --release
copy target\release\engine.dll ..\engine.dll
cd ..\..
dotnet build -c Release
```

The engine must be copied to `engine\engine.dll` before the .NET build, which bundles it with the app. The installer script is `installer\installer.nsi` (NSIS).

## Usage

1. Open the app and press **Start Replay**.
2. Press the hotkey (or use **Save Replay**) to save the buffered footage.
3. Replays are saved as `replay_YYYYMMDD_HHMMSS\replay.mp4` in `Pictures\Instant Replay`, or in the save location you set.

## Settings

Replay duration, frame rate, quality, video encoder, capture method, system audio, microphone, save replay hotkey, save location, start with Windows, dark mode, backdrop type, debug logging.

## Files the app writes

| Path | Contents |
|---|---|
| `%APPDATA%\InstantReplay\settings.json` | Settings |
| `Pictures\Instant Replay` (default) | Saved replays and thumbnails |
| `%TEMP%\debug_<timestamp>.txt` | Log file |
| `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` | Start with Windows entry |

## Environment variables

- `IR_GPU=0` forces the CPU conversion path
- `IR_ENCODER=hw|sw|auto` overrides the encoder choice
- `IR_IMPROVED_CONVERT=1` uses the higher-quality two-pass GPU color conversion

## Third-party code

`engine/rust/shaders/wcap_shaders.hlsl` is taken unmodified from [mmozeiko/wcap](https://github.com/mmozeiko/wcap) (Unlicense). The Rust engine vendors `windows-capture` under `engine/rust/vendor`.

## License

MIT, see [LICENSE](LICENSE). The installer displays [installer/terms.txt](installer/terms.txt).

## Donations

USDT (TRC-20): `TX8e5fyFvRs6foXK3GUbJbEQE1Q2bXZ8QK`