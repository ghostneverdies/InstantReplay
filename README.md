# Instant Replay 1.5

[![Stars](https://img.shields.io/github/stars/ghostneverdies/InstantReplay?style=flat-square)](https://github.com/ghostneverdies/InstantReplay/stargazers)
[![Forks](https://img.shields.io/github/forks/ghostneverdies/InstantReplay?style=flat-square)](https://github.com/ghostneverdies/InstantReplay/forks)
[![Downloads](https://img.shields.io/github/downloads/ghostneverdies/InstantReplay/total?style=flat-square)](https://github.com/ghostneverdies/InstantReplay/releases)
[![Latest release](https://img.shields.io/github/v/release/ghostneverdies/InstantReplay?style=flat-square)](https://github.com/ghostneverdies/InstantReplay/releases/latest)
![views](https://komarev.com/ghpvc/?username=ghostneverdies-InstantReplay&label=README+views&color=007ec6&style=flat-square)


[![Rust 48.32%](https://img.shields.io/static/v1?label=Rust&message=48.32%25&color=dea584&style=flat-square)](https://api.github.com/repos/ghostneverdies/InstantReplay/languages)
[![C# 47.58%](https://img.shields.io/static/v1?label=C%23&message=47.58%25&color=178600&style=flat-square)](https://api.github.com/repos/ghostneverdies/InstantReplay/languages)
[![JavaScript 1.84%](https://img.shields.io/static/v1?label=JavaScript&message=1.84%25&color=f1e05a&style=flat-square)](https://api.github.com/repos/ghostneverdies/InstantReplay/languages)
[![NSIS 0.99%](https://img.shields.io/static/v1?label=NSIS&message=0.99%25&color=8b5cf6&style=flat-square)](https://api.github.com/repos/ghostneverdies/InstantReplay/languages)
[![CSS 0.81%](https://img.shields.io/static/v1?label=CSS&message=0.81%25&color=563d7c&style=flat-square)](https://api.github.com/repos/ghostneverdies/InstantReplay/languages)
[![HTML 0.47%](https://img.shields.io/static/v1?label=HTML&message=0.47%25&color=e34c26&style=flat-square)](https://api.github.com/repos/ghostneverdies/InstantReplay/languages)


## Features

- Rolling in-memory replay buffer (30 / 60 / 120 seconds, capped at 512 MB of encoded video)
- Global save hotkey, default `Ctrl + Numpad *`, rebindable in Settings
- System audio for Whole Desktop; process audio for Game Capture; independently selectable microphone recording
- Maximum frame rate: 30, 60 or 120 FPS
- Quality presets: Low, Balanced, High
- Two replay targets: Whole Desktop and Game Capture, with offline game detection and custom executable rules
- Capture method: native Windows Graphics Capture or DXGI Desktop Duplication
- Video encoder: Auto, Hardware or Software (Media Foundation H.264)
- Gallery with thumbnails, inline playback, optional fullscreen, seeking, volume and "show in folder"
- System tray icon, animated clip popup, warning notifications, close-to-tray, single instance
- Optional start with Windows (launches minimized to tray)
- Updates page with manual checks, automatic checks every 24 hours, and verified full-installer downloads from GitHub Releases
- Dark / light mode with a permanent Mica backdrop
- Warns when the app's memory use stays unusually high

## Requirements

- Windows 10 version 2004 (build 19041) or later, x64
- .NET 8 Desktop Runtime (the build is framework-dependent)
- [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (required for both inline and fullscreen playback)

## Install

Run `InstantReplaySetup.exe`. It installs to `Program Files\Instant Replay`, creates Start Menu and optional desktop shortcuts, and registers start with Windows. The installer does not download prerequisites. Installing over an existing copy replaces app files without opening the uninstaller; settings and saved replays are retained.

## Build

Requires a compatible .NET SDK, the x64 MSVC Rust toolchain, Visual Studio C++ Build Tools and the Windows SDK. Cargo must be on PATH.

```
cd engine\rust
cargo build --release
copy target\release\engine.dll ..\engine.dll
cd ..\..
dotnet build -c Release -p:Platform=x64
```

The engine must be copied to `engine\engine.dll` before the .NET build, which bundles it with the app. If `CARGO_TARGET_DIR` is set, copy `release\engine.dll` from that directory instead of the local `target` directory. To package version 1.5, install NSIS and publish the app into the directory used by the installer:

```powershell
dotnet publish -c Release -p:Platform=x64 -o bin\publish\win-x64
cd installer
& 'C:\Program Files (x86)\NSIS\makensis.exe' installer.nsi
```

This creates `installer\InstantReplaySetup.exe`. The installer does not install .NET or WebView2 automatically.

## Usage

1. Open the app. Replay buffering starts automatically; **Start Replay** starts it again after stopping.
2. Press the hotkey (or use **Save Replay**) to save the buffered footage.
3. Replays are saved as `replay_YYYYMMDD_HHMMSS\replay.mp4` in `Pictures\Instant Replay`, or in the save location you set.

Only one app instance owns the replay engine. Additional launches signal the running window and exit before creating another capture session. Startup handles rapid repeated launches and recovers an abandoned instance lock after a crash.

Gallery thumbnails are cached in `%TEMP%\thumbnails`. If Windows cannot supply a thumbnail, the app extracts a frame directly from the replay; unreadable cached thumbnails are regenerated.

Replay saves share immutable encoded video packets with the buffer instead of duplicating the entire recording. MP4 conversion uses one reusable frame buffer, and trimmed audio packets are borrowed while writing. Before saving, the engine checks Windows committed-memory headroom and refuses the save if there is less than a 512 MB reserve plus the estimated audio workspace. A refusal shows the normal failed-clip popup and logs the reason; the existing buffer is retained. This reduces save-time memory spikes but cannot prevent other applications from exhausting Windows memory. A system-managed page file is recommended on machines that encounter virtual-memory exhaustion.

## Settings

The app checks for the WebView2 Runtime at startup. If it is missing, a persistent warning above the application pages links to Microsoft's download page. It has its own layout row and does not cover page content; it is hidden when WebView2 is installed. Capture and replay saving use the native engine independently of WebView2.

The scrollable Settings page puts replay target, video quality, audio and storage first. Encoder, capture method and logging are grouped under Advanced. Changes to recording settings take effect when applied, which restarts an active buffer. Returning to a page resets its scroll position.

Quality changes bitrate only. All encoder choices use the same maximum output bounds of **4K (3840 × 2160)**, preserve the source aspect ratio and do not upscale smaller sources. Auto no longer applies a hidden 900p limit. Actual 4K frame rates depend on encoder and system performance; the source must be 4K to produce 4K output. Presets use one shared mapping for both UI labels and native encoder parameters:

| FPS | Low | Balanced | High |
|---|---|---|---|
| 30 | 2 Mbps | 3 Mbps | 4.5 Mbps |
| 60 | 4 Mbps | 6 Mbps | 9 Mbps |
| 120 | 8 Mbps | 12 Mbps | 18 Mbps |

These are bitrate targets, not fixed output file sizes; static scenes can use less data. Existing `Fast` and `Quality` settings remain compatible with Low and High. Quality does not change the selected encoder or frame-pacing behavior.

**Whole Desktop** captures the selected monitor and system audio. It bypasses all game detection, focus switching and automatic pausing.

**Show mouse cursor** enables or hides the recorded pointer for both WGC and DXGI. Apply changes to restart the buffer with the selected visibility. DXGI retains the last desktop image for cursor-only updates and composites the current pointer separately. GPU encoding uses a separate Direct3D device/context so desktop readback cannot hold the encoder's context lock; captured frames are released before encoding. DXGI pacing allows up to 2 ms of timestamp jitter, bounded by the frame interval, while rejecting duplicate and stale frames. The GPU encoder checks for pending frames before waiting so a missed wake-up does not add an unnecessary delay.

WGC requests a 1 ms minimum update interval when Windows supports it, matching original wcap. Older Windows versions retain their default capture interval. WGC and DXGI allow up to 2 ms of timestamp jitter, bounded to one eighth of the selected frame interval, while rejecting duplicate and out-of-order timestamps. After a stall, pacing resumes from fresh source frames instead of generating catch-up frames. The FPS setting remains a ceiling: source rendering, system load, and encoder throughput determine actual delivery. Static or paused scenes are not padded with artificial duplicate frames.

**Game Capture** captures recognized games and apps added on the **Games** navigation page, which appears when this target is selected. Add an executable filename or browse to an `.exe`, give it a name, and optionally require a title keyword. Browsing stores the filename; the app never launches that executable. Built-in games are included automatically.

- Capture requires both a match in the built-in or custom games list and a visible, titled foreground window. Titles alone never authorize capture. Start armed and wait until both requirements are met.
- A second game opening in the background does not change the target.
- Focusing a different eligible window discards the old replay and starts a new buffer with that window's process audio, including its child processes.
- Focusing an unlisted application pauses game video and game audio while retaining the current buffer. Returning to the same game resumes it.
- The microphone remains independent and continues during game pauses. With microphone recording enabled, the pause retains elapsed time and holds the last game image so microphone speech stays synchronized, without encoding duplicate frames. With the microphone disabled, paused time is removed from the replay timeline.
- Closing the captured window discards its replay and returns to waiting, including when capture was paused. Stopping replay disarms detection; the next game cannot restart it automatically.

The offline catalog in `assets/games.json` contains 8,460 game/client names and 8,967 Windows executable rules, derived from the [public Discord detectable-applications catalog](https://discord.com/api/v9/applications/detectable) with local Minecraft client rules. `assets/games-source.json` records its origin, retrieval date, source checksum and local additions. This is a recognition catalog, not a popularity ranking. Generic executable names require a matching game title; custom keywords distinguish shared executables such as `javaw.exe`. Launchers, Windows shell/input windows, cloaked/tool windows and Instant Replay itself are excluded. The catalog is loaded once into an immutable executable index. Only the foreground window is checked, custom rules are indexed too, and process names use a bounded cache; polling does not scan the full catalog or enumerate every process. Detection uses ordinary Win32 window/process queries in `GameDetector.cs`; it does not inject code into games or scan their memory.

Minecraft Java runs inside `java.exe` or `javaw.exe`; its launcher executable is not the capture target. Built-in title rules cover vanilla Minecraft, Fast Client, Silent Client, Lunar Client, Badlion Client, Feather Client, TLauncher and Legacy Launcher, including compact client names where applicable. Built-in Minecraft Java rules also require an `LWJGL`, `GLFW30` or `SDL_app` game-window class so Java launcher/control windows are not selected. Games launched through TLauncher or Legacy Launcher that retain the ordinary Minecraft title use the vanilla rule. The recognized game must still be foreground to start or resume capture, and app audio follows the game process rather than its launcher. Explicit custom rules remain available for other Java applications.

Game Capture always uses WGC and requests square window corners regardless of the stored rounded-corner preference; the previous Windows corner preference is restored when capture stops. Selecting it with DXGI configured displays a WinUI compatibility dialog and switches to WGC; DXGI remains available for Whole Desktop. Auto encoding tries hardware and falls back to Microsoft's Media Foundation software H.264 encoder. Process audio requires Windows build 20348 or later (Windows 11 supported); a process-audio failure is reported rather than falling back to system audio.

Game Capture detects client-size, window-style, monitor and display-size changes, pauses while they settle for 600 ms, then recreates the window session and discards the previous buffer. WGC skips stale resize frames before rebuilding its frame pool and processes the newest available frame from its bounded pool after capture stalls, rather than encoding a backlog. Capture startup verifies that frames arrive and retries failed window sessions with a cooldown. Whole Desktop does not switch capture targets or add special recovery for individual games.

**Fullscreen capture limitation:** Game Capture may be unable to capture fullscreen apps or games, especially exclusive fullscreen. Video may appear black or frozen even while audio continues. Try **Whole Desktop**, which may capture fullscreen content that Game Capture cannot; fullscreen capture is not guaranteed with either target. If neither works, use borderless or windowed mode. Protected content may remain unavailable.

Capture follows fresh compositor/desktop timestamps, skips superseded frames and rejects duplicate or out-of-order timestamps. It does not create filler frames on a timer. During a stationary scene, MP4 sample duration holds the last valid image. The FPS setting is a maximum, so static scenes produce fewer encoded frames. Hardware encoding uses tracked Media Foundation samples so GPU buffers are recycled only after the encoder releases them; Software selects synchronous Media Foundation H.264 encoding, as in wcap.

Replay saves show a compact popup at the top left of the foreground display with the app icon. Successful saves show **Clipped Last N Seconds**, using the saved MP4 duration; failed saves show **Failed To Clip** and **See debug log for more details**, with the reason recorded in the log. Save results no longer show an infobar in the main window. The popup slides in from the left, remains visible for three seconds, then slides out without activating or stealing game focus. It requests capture exclusion and is raised above borderless/fullscreen windows on a best-effort basis; legacy exclusive fullscreen can prevent it from appearing, and a popup failure never invalidates the saved clip. Other warnings retain their existing notification path.

Click a gallery replay to play inside the app. The fullscreen button or F switches views; Escape returns from fullscreen. Switching preserves playback position, play/pause state, volume and mute. The inline player follows the recorded aspect ratio with rounded video corners; fullscreen includes an exit control in the corner. Volume waves fade in sequence as volume decreases: outer wave first, then inner wave, then the muted indicator.

## Updates

The **Updates** page sits at the bottom of navigation. **Check for updates** reads the latest stable release from `ghostneverdies/InstantReplay` on GitHub. Numeric version comparison treats `v1.5` and `1.5.0` as equal and offers only newer versions, never a downgrade. Automatic checks run when due at startup and every 24 hours while the app runs; the last attempt is persisted across restarts. Failed checks do not interrupt recording, and manual checks can retry immediately.

When a newer release is found, a WinUI dialog over the app offers **Update now** or **Not now**. The reminder appears at most once per 24 hours, including across restarts and repeated manual checks. Reminders wait until the app is in foreground use rather than bringing it above a game or opening it from the tray. Declining keeps recording and leaves the update available on the Updates page.

**Download and install** asks before restarting, downloads the complete `InstantReplaySetup.exe`, and verifies its byte count and GitHub-provided SHA-256 digest. Recording continues during download. After verification, a temporary local PowerShell helper waits for the app to fully exit, runs the installer with `/S -quiet /UPDATE=1 /D=<current app directory>`, waits for installation to finish, and reopens the app as the original user. Windows administrator approval can still appear. Save any replay you want before installing: the in-memory buffer is cleared on exit. Cancellation or download failure leaves the running app untouched; cancelling elevation reopens the existing app and reports the failure on the Updates page.

Release tags must use numeric versions such as `v1.6.0`. Upload `InstantReplaySetup.exe` as a release asset with a SHA-256 digest available through GitHub's API. No token, VPS, patching plugin, or GitHub credentials are embedded in the app. The installer source supports `-quiet` as well as NSIS's `/S`; update mode skips the finish page and browser launch and preserves existing startup and shortcut preferences. It overwrites app files directly instead of launching the uninstaller. Settings and saved recordings are retained.

When preparing a release, keep the app version in `InstantReplay.csproj`, the installer version fields in `installer/installer.nsi`, and the GitHub release tag consistent. The updater reads the app's assembly version; uploading an older app under a newer tag would cause it to keep offering that update. There is no automatic installer rollback. A failed installation can be repaired by rerunning the full installer.

## Files the app writes

| Path | Contents |
|---|---|
| `%APPDATA%\InstantReplay\settings.json` | Settings |
| `%APPDATA%\InstantReplay\updates.json` | Last update check, last reminder time and cached release metadata |
| `%APPDATA%\InstantReplay\update-result.json` | Installer result, consumed after reopening |
| `%TEMP%\InstantReplay-Updates\<unique folder>` | Verified installer and temporary local update helper |
| `Pictures\Instant Replay` (default) or your chosen folder | Saved replay folders and MP4 files |
| `%TEMP%\thumbnails` | Cached replay preview images |
| `%TEMP%\debug_<timestamp>.txt` | Log file |
| `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` | Start with Windows entry |

## Environment variables

- `IR_GPU=0` forces the CPU conversion path
- `IR_ENCODER=hw|sw|auto` overrides the encoder choice
- `IR_IMPROVED_CONVERT=1` uses the higher-quality two-pass GPU color conversion

## Third-party code

The capture/encoding approach and embedded, compressed shader bytecode are adapted from [mmozeiko/wcap](https://github.com/mmozeiko/wcap), released into the public domain under the Unlicense. `engine/rust/shaders/wcap_shaders.hlsl` is retained as reference source. Runtime code decompresses embedded bytecode with `D3DDecompressShaders`; it does not compile HLSL at runtime. The engine depends only on the Windows Rust bindings and does not use `windows-capture` or a WASAPI wrapper crate.

The Rust DLL keeps the existing `IR_*` ABI and provides `IR_SetCaptureTarget`, `IR_SetPaused`, `IR_GetWindowCount` and `IR_GetWindowInfo`. Set the target before `IR_Start`: HWND 0 means monitor capture. Capture options are 1=client area, 2=cursor, 4=border, 8=keep rounded corners, 16=secondary windows, 32=legacy automatic active-window selection for existing DLL callers, and 64=process audio for the supplied HWND. The frontend uses explicit HWND targets and never sets bit 32. `IR_SetPaused` gates video and app audio; enabled microphone capture continues independently. Window-only options require the corresponding Windows capture APIs; unsupported optional flags are ignored.

## License

MIT, see [LICENSE](LICENSE). The installer displays [installer/terms.txt](installer/terms.txt).

## Donations

USDT (TRC-20): `TBLWg9dJtimMijGJtSKGX61DtmZbYaDbkD`
