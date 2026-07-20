# Security

## This App Is Not a Virus

Instant Replay is a free, open-source screen recording application. This document explains what the app does, why it may trigger antivirus warnings, and why it is safe to use.

## Why Antivirus Software May Flag This App

Antivirus programs sometimes flag applications like Instant Replay for the following reasons:

### 1. It Records Your Screen

Instant Replay uses the **Desktop Duplication API** (via FFmpeg's `ddagrab` filter) to capture your screen contents. This is the same Windows API used by legitimate tools like OBS Studio, NVIDIA ShadowPlay, and Xbox Game Bar. Screen capture functionality can overlap with how screen recorders used by malware operate, which sometimes causes false positives.

### 2. It Installs a Virtual Audio Device

The **Virtual Audio Capturer** (from [screen-capture-recorder-to-video-windows-free](https://github.com/rdp/screen-capture-recorder-to-video-windows-free)) is a DirectShow filter that routes system audio into FFmpeg for recording. Antivirus software may flag audio capture drivers because they intercept audio streams — this is a known false positive pattern for virtual audio devices.

### 3. It Uses Global Hotkeys

The app registers a **global hotkey** (`Ctrl + Numpad *` by default) via the Win32 `RegisterHotKey` API. This is the same mechanism used by applications like Discord, OBS, and Steam. Some antivirus heuristics flag global hotkey registration because keyloggers also use similar APIs.

### 4. It Modifies the System PATH

The installer adds `C:\ffmpeg` to the system PATH environment variable so FFmpeg can be found by the app and other tools. Environment variable modification is a behavior shared by both legitimate software installers and malware, which can trigger heuristic detections.

### 5. It Adds Windows Defender Exclusions

The installer adds Defender and Controlled Folder Access exclusions for the app's install directory. This is necessary because the app writes frequently to disk (saving replay segments in real-time), and Windows' ransomware protection can block legitimate file writes. While adding exclusions is a legitimate pattern for gaming and recording apps, some security tools flag it.

### 6. It Runs FFmpeg as a Background Process

FFmpeg runs continuously as a background process to maintain the rolling recording buffer. Long-running background processes that read from the screen and write to disk can resemble patterns seen in screen-capturing malware.

## What This App Does NOT Do

- Does **not** collect, transmit, or upload any data
- Does **not** access the internet except to download dependencies (FFmpeg, Virtual Audio Capturer) during installation
- Does **not** read keystrokes or log keyboard input (the global hotkey uses Win32's `RegisterHotKey`, not a keyboard hook)
- Does **not** capture individual windows or specific applications — it captures the entire desktop output
- Does **not** install any background services or scheduled tasks
- Does **not** modify system files or Windows settings beyond the PATH and Defender exclusions
- Does **not** contain any telemetry, analytics, or tracking code
- Does **not** bundle any third-party adware, toolbars, or unwanted software

## Dependencies Explained

### FFmpeg

| | |
|---|---|
| **What it is** | The industry-standard open-source multimedia framework |
| **What it does here** | Captures your screen via the Desktop Duplication API, captures audio via DirectShow, and encodes both into H.264/AAC MP4 files |
| **Why it's safe** | FFmpeg is one of the most widely used open-source projects in the world, maintained by a large community. It is used by VLC, YouTube, Netflix, and thousands of other applications |
| **Source** | [github.com/BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) (GPL-licensed build) |
| **Installed to** | `C:\ffmpeg\ffmpeg.exe` and `C:\ffmpeg\ffprobe.exe` |

### Virtual Audio Capturer

| | |
|---|---|
| **What it is** | A DirectShow virtual audio driver for Windows |
| **What it does here** | Creates a virtual audio device that captures your system/desktop audio and routes it to FFmpeg for recording |
| **Why it's safe** | Open-source project hosted on GitHub. It only creates an audio loopback device — it does not send audio anywhere |
| **Source** | [github.com/rdp/screen-capture-recorder-to-video-windows-free](https://github.com/rdp/screen-capture-recorder-to-video-windows-free) |
| **Installed to** | `C:\Program Files\ScreenCapturerRecorder\` |

### .NET 8 Runtime (Bundled)

| | |
|---|---|
| **What it is** | Microsoft's cross-platform application runtime |
| **What it does here** | Runs the application. Bundled as self-contained — users don't need to install .NET separately |
| **Why it's safe** | Developed and maintained by Microsoft. Used by millions of applications worldwide |
| **Source** | [dotnet.microsoft.com](https://dotnet.microsoft.com/) |

### Windows App SDK 1.7 (Bundled)

| | |
|---|---|
| **What it is** | Microsoft's UI framework for modern Windows desktop apps |
| **What it does here** | Provides the WinUI 3 interface (buttons, settings page, dialogs, etc.) |
| **Why it's safe** | Official Microsoft framework, successor to WPF/UWP |
| **Source** | [github.com/microsoft/WindowsAppSDK](https://github.com/microsoft/WindowsAppSDK) |

## Verifying the Installer

The installer (`InstantReplaySetup.exe`) is built using [Inno Setup](https://jrsoftware.org/isinfo.php), a widely-used, open-source Windows installer framework. You can inspect the installer script at `installer/setup.iss` in the source repository to see exactly what the installer does.

### What the installer does:

1. Copies the app files to `Program Files\Instant Replay`
2. Downloads FFmpeg from `github.com/BtbN/FFmpeg-Builds` and installs it to `C:\ffmpeg`
3. Downloads Virtual Audio Capturer from `github.com/rdp/screen-capture-recorder-to-video-windows-free` and installs it silently
4. Adds `C:\ffmpeg` to the system PATH
5. Adds a Windows Defender exclusion for the install directory
6. Adds a Controlled Folder Access exclusion for the app executable
7. Creates Start Menu and optional desktop shortcuts

### What the uninstaller does:

1. Removes the app files from `Program Files\Instant Replay`
2. Removes `C:\ffmpeg` from the system PATH and deletes the folder
3. Uninstalls Virtual Audio Capturer
4. Removes the Windows Defender and Controlled Folder Access exclusions

## Building from Source

You can verify the app's safety by building it yourself from source:

```bash
git clone https://github.com/Anonymi69/InstantReplay.git
cd InstantReplay
dotnet publish -c Release -r win-x64 --self-contained true -p:PublishSingleFile=false -p:WindowsPackageType=None -o publish
```

The published output contains only the .NET runtime, Windows App SDK, and the application code — all of which can be inspected in the repository.

## Contact

If you have security concerns or questions, please open an issue on the GitHub repository.
