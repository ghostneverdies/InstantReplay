# Instant Replay

A Windows desktop app that continuously records your screen into a rolling buffer, letting you save the last 30, 60, or 120 seconds as an MP4 replay on demand — similar to NVIDIA ShadowPlay's Instant Replay, but built entirely on open-source tooling.

## Features

- **Rolling replay buffer** — Continuously captures screen + audio into 1-second segments that cycle based on your chosen duration (30s / 60s / 120s)
- **Global hotkey save** — Press a configurable hotkey (default: `Ctrl + Numpad *`) to instantly save the buffered replay to disk
- **Dual audio capture** — Records desktop audio and optionally your microphone simultaneously, with independent sync offsets
- **System tray integration** — Minimizes to the system tray with a right-click context menu; close button hides to tray instead of quitting
- **Start with Windows** — Optional auto-start via a registry entry that launches silently in the tray
- **Recording quality presets** — Fast (ultrafast/CRF 23), Balanced (faster/CRF 20), Quality (medium/CRF 18)
- **Configurable frame rate** — 30, 60, or 120 FPS
- **Customizable save location** — Defaults to `Pictures\Instant Replay`
- **Watchdog** — Auto-restarts FFmpeg if it crashes or after 60 minutes of continuous recording
- **Buffer health monitoring** — Detects capture stalls and displays error state
- **Memory leak detection** — Warns via tray balloon if memory usage exceeds safe limits
- **Dark mode / Light mode** — With optional Mica transparency backdrop

## Requirements

- Windows 10 (build 17763) or later
- x64 processor

## Installation

### Option 1: Installer (Recommended)

Download `InstantReplaySetup.exe` and run it. The installer will:

1. Install the app to `Program Files\Instant Replay`
2. Download and install [FFmpeg](https://ffmpeg.org/) to `C:\ffmpeg` (if not already installed)
3. Download and install [Virtual Audio Capturer](https://github.com/rdp/screen-capture-recorder-to-video-windows-free) for desktop audio capture (if not already installed)
4. Download and install [Java Runtime](https://adoptium.net/) (required by Virtual Audio Capturer, if not already installed)

### Option 2: Build from Source

```bash
git clone https://github.com/Anonymi69/InstantReplay.git
cd InstantReplay
dotnet publish -c Release -r win-x64 --self-contained true -p:PublishSingleFile=false -p:WindowsPackageType=None -o publish
```

The published output contains only the .NET runtime, Windows App SDK, and the application code — all of which can be inspected in the repository.

The app can also run as an MSIX package by setting `WindowsPackageType` to `MSIX` in the project file.

## Usage

1. Launch the app — it starts recording immediately to a rolling buffer
2. When something cool happens, press `Ctrl + Numpad *` to save the last 30/60/120 seconds as an MP4
3. Find your replay in `Pictures\Instant Replay` (or your custom save location)

### Settings

- **Duration** — How many seconds to keep in the buffer (30, 60, or 120)
- **Frame Rate** — 30, 60, or 120 FPS
- **Quality** — Fast, Balanced, or Quality encoding preset
- **Audio** — Toggle desktop audio and/or microphone with individual volume and sync offsets
- **Hotkey** — Click the hotkey field and press your desired key combination
- **Save Location** — Browse to change where replays are saved
- **Start with Windows** — Auto-launch on login

## Dependencies

### FFmpeg (Required)

The video/audio engine. Installed to `C:\ffmpeg` by the installer.

- Used for screen capture via the `ddagrab` filter (Desktop Duplication API)
- Used for audio capture via DirectShow with the Virtual Audio Capturer
- Used for encoding to H.264 (libx264) + AAC
- Downloaded from [BtbN FFmpeg Builds](https://github.com/BtbN/FFmpeg-Builds) (GPL build)

### Virtual Audio Capturer (Required for desktop audio)

A DirectShow virtual audio device that routes system audio into FFmpeg for recording.

- Installed from [screen-capture-recorder-to-video-windows-free](https://github.com/rdp/screen-capture-recorder-to-video-windows-free)
- Required only if you want to capture desktop/system audio
- The installer handles download and silent installation

### Java Runtime (Required by Virtual Audio Capturer)

The Virtual Audio Capturer depends on Java Runtime to function.

- Installed from [Eclipse Adoptium (Temurin)](https://adoptium.net/) (JDK 21)
- Detected by running `java --version` and checking for known Java installation paths
- The installer handles download and silent installation if not already present

### .NET 8 Runtime (Bundled)

The app is distributed as self-contained — the .NET 8 runtime is bundled inside the executable, so users do not need to install .NET separately.

### Windows App SDK 1.7 (Bundled)

Provides the WinUI 3 UI framework. Bundled with the self-contained deployment.

## Tech Stack

| Component | Technology |
|---|---|
| UI | WinUI 3 (WinAppSDK 1.7) |
| Runtime | .NET 8 (self-contained, x64) |
| Video/Audio | FFmpeg (external process) |
| Screen Capture | FFmpeg `ddagrab` filter (Desktop Duplication API) |
| Audio Capture | FFmpeg + DirectShow (Virtual Audio Capturer) |
| Encoding | libx264 (video), AAC (audio) |
| Tray Icon | Win32 P/Invoke (`Shell_NotifyIcon`) |
| Hotkeys | Win32 P/Invoke (`RegisterHotKey`) |
| Settings | JSON file at `%APPDATA%\InstantReplay\settings.json` |

## License

This is free and open-source software. See the [Terms and Conditions](installer/terms.txt) for details.

## Donations

If you find this useful, consider donating:

- **USDT (TRC-20)**: `TX8e5fyFvRs6foXK3GUbJbEQE1Q2bXZ8QK`
