# Instant Replay 1.6

[![Stars](https://img.shields.io/github/stars/ghostneverdies/InstantReplay?style=flat-square)](https://github.com/ghostneverdies/InstantReplay/stargazers)
[![Forks](https://img.shields.io/github/forks/ghostneverdies/InstantReplay?style=flat-square)](https://github.com/ghostneverdies/InstantReplay/forks)
[![Downloads](https://img.shields.io/github/downloads/ghostneverdies/InstantReplay/total?style=flat-square)](https://github.com/ghostneverdies/InstantReplay/releases)
[![Latest release](https://img.shields.io/github/v/release/ghostneverdies/InstantReplay?style=flat-square)](https://github.com/ghostneverdies/InstantReplay/releases/latest)


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
- Capture method: Windows Graphics Capture for games and windows, or DXGI Desktop Duplication for whole-desktop and exclusive-fullscreen capture
- The clip card is drawn inside supported Direct3D 9/11/12 and OpenGL games (fullscreen and windowed); protected anti-cheat games are never injected and use the external popup
- Video encoder (FFmpeg): Auto, NVIDIA NVENC, AMD AMF, Intel Quick Sync, x264 (CPU) or Media Foundation, with CBR/VBR rate control, a speed/quality preset and MP4, MOV, MKV, MPEG-TS or FLV output (AAC audio)
- Gallery with thumbnails, inline playback, optional fullscreen, seeking, volume and "show in folder"
- Home page with a status/hotkey hero card, Save Replay / Start Replay actions, and live CPU and memory cards with sparkline history
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

Run `InstantReplaySetup.exe`. The installer puts the whole app payload in `Program Files\Instant Replay\ship` and adds an `InstantReplay.lnk` launcher next to it, so the install folder holds only the launcher and the `ship` folder. Start Menu and optional desktop shortcuts launch the executable inside `ship\`. It registers start with Windows and does not download prerequisites. Installing over an existing copy replaces app files without opening the uninstaller; settings and saved replays are retained.

```
Program Files\Instant Replay\
├─ InstantReplay.lnk          (launcher for ship\InstantReplay.exe)
└─ ship\                      (entire publish output)
   ├─ InstantReplay.exe
   ├─ Uninstall.exe
   ├─ engine\bin\engine.dll
   ├─ engine\encoder\*.dll
   ├─ engine\hooks\bin\*.dll
   ├─ assets\  mediaplayer\
   └─ WinUI runtime, locales, meta\, symbols\
```

The uninstall entry in Settings > Apps points at `ship\Uninstall.exe`; it removes the launcher, the `ship` folder and the registry entries, and can optionally delete recordings.

Updating an older flat install (app files directly in the install folder) migrates it into `ship\` once and removes the old files from the root.

## Build

Requires a compatible .NET SDK, the x64 MSVC Rust toolchain, Visual Studio C++ Build Tools and the Windows SDK. Cargo must be on PATH.

```
cd engine\engine_core
cargo build --release
copy target\release\engine.dll ..\bin\engine.dll
cd ..\..
dotnet build -c Release -p:Platform=x64
```

The project also runs `cargo build --release` for `engine\engine_core` and `engine\hooks\build.ps1` for the hook DLLs before compiling (`BuildNativeEngine` and `BuildHook` targets), so a normal `dotnet build` rebuilds them when their sources change. The manual cargo step above is only needed when you want to inspect the native output first.

Encoding, audio compression and file writing use FFmpeg 8.1 shared libraries, loaded at runtime (not linked) from `engine\encoder\`. Download the pinned `ffmpeg-n8.1.3-29-gad3b795b72-win64-gpl-shared-8.1.zip` asset from [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds/releases/tag/autobuild-2026-11-01-12-49) and copy `avcodec-62.dll`, `avformat-62.dll`, `avutil-60.dll` and `swresample-6.dll` from its `bin\` folder into `engine\encoder\` before the .NET build, which bundles the whole folder with the app as `engine\encoder\*.dll`. Without them the app reports an error and does not record. The pinned tag is immutable; the CI workflow verifies the archive's SHA-256 before using it.

`engine\encoder\gen-bindings.ps1` regenerates `engine\engine_core\src\ffmpeg_sys.rs` from that build's headers (requires `cargo install bindgen-cli` and `python -m pip install libclang`), and `engine\engine_core\src\ffmpeg.rs` resolves every function from the DLLs by name at runtime rather than linking against them.

The engine must be copied to `engine\bin\engine.dll` before the .NET build, which bundles it with the app. If `CARGO_TARGET_DIR` is set, copy `release\engine.dll` from that directory instead of the local `target` directory. To package the app locally, install NSIS, publish into the directory used by the installer, and pass the version (the defaults are `1.6` / `1.6.0.0`):

```powershell
dotnet publish -c Release -p:Platform=x64 -p:Version=1.6 -o bin\publish\win-x64
cd installer
& 'C:\Program Files (x86)\NSIS\makensis.exe' /DMyAppVersion=1.6 /DMyAppVersion4=1.6.0.0 installer.nsi
```

The publish output is reorganized by `InstantReplay.csproj` so the app folder stays readable:

```
InstantReplay.exe
InstantReplay.dll, *.json, resources.pri, WinUI/WinAppSDK DLLs   (must sit next to the exe)
<locale>\*.mui                                                   (must stay here, see below)
engine\bin\engine.dll
engine\encoder\avcodec-62.dll ...
engine\hooks\bin\capture_hook.dll, overlay_hook.dll
assets\  mediaplayer\  meta\*.winmd  symbols\*.pdb
```

The WinUI and Windows App SDK DLLs, `resources.pri`, the `.deps.json`/`.runtimeconfig.json` files and the `<locale>` folders cannot move: Windows resolves the DLLs and the `.mui` resources from the executable's own directory before any app code runs, and relocating the locale folders makes `Microsoft.UI.Xaml.dll` fail at startup. Everything that is safe to group is grouped, and the engine finds its FFmpeg DLLs one level up in `engine\encoder\`.

This is the publish layout. The installer keeps it intact but nests it under `ship\` in the install root, which is why the app still finds every file relative to its own executable. The portable ZIP keeps the flat layout.

The source tree is grouped by responsibility, and small related types share one file (each file keeps a single namespace):

| Folder | Contents |
|---|---|
| `InstantReplay.GUI` | WinUI pages, shell (`TrayIcon`, `TaskbarBadge`, `ToastCenter`), controls and helpers (`TitleBarButtons` + `Chrome`) |
| `InstantReplay.Core` | Replay engine front end, capture target management and `GameDetector` |
| `InstantReplay.Models` | Settings and shared records/enums: `CaptureModels.cs`, `UpdateModels.cs`, `VideoSettings.cs`, `SaveReplayResult.cs`, `GameRule.cs` |
| `InstantReplay.Services` | Windows integration (`WindowsTheme` + `StartupManager`), updates, update install, audio device watching, window detection and hook injection |
| `InstantReplay.Native` | Engine P/Invoke and audio endpoint COM interop (`IrNative.cs`) |
| `engine` | Rust engine, capture/overlay hook sources, FFmpeg shared libraries |
| `installer` | NSIS script, license terms and installer assets |

This creates `installer\InstantReplaySetup.exe`. The installer does not install .NET or WebView2 automatically.

## Releases

Releases are built on demand. Run the **Release** workflow manually from the Actions tab, or from the CLI:

```
gh workflow run release.yml -f version=1.6.0
```

Run it from the default branch, because the workflow creates the release tag `v<version>` at the commit it was dispatched on. Nothing runs automatically: tag pushes and published releases do not trigger the workflow.

The workflow downloads and checksums the pinned FFmpeg build, runs `cargo build --release`, stages `engine\bin\engine.dll`, runs `dotnet publish` with `-p:Version=<version>`, builds the installer with the same version, and attaches two assets to the GitHub Release:

- `InstantReplay-<version>-win64-portable.zip` — the publish output, including `engine\encoder\*.dll`
- `InstantReplaySetup.exe` — the installer, stamped with the same version (the in-app updater looks for exactly this asset name on the latest release)

Both files are also uploaded as workflow artifacts.

## Usage

1. Open the app. Replay buffering starts automatically; **Start Replay** starts it again after stopping.
2. Press the hotkey (or use **Save Replay**) to save the buffered footage.
3. Replays are saved as `replay_YYYYMMDD_HHMMSS\replay.<ext>` in `Pictures\Instant Replay`, or in the save location you set. The extension follows the container chosen in Advanced settings (`.mp4`, `.mov`, `.mkv`, `.ts` or `.flv`).

Only one app instance owns the replay engine. Additional launches signal the running window and exit before creating another capture session. Startup handles rapid repeated launches and recovers an abandoned instance lock after a crash.

Gallery thumbnails are cached in `%TEMP%\thumbnails`. If Windows cannot supply a thumbnail, the app extracts a frame directly from the replay; unreadable cached thumbnails are regenerated.

Replay saves share immutable encoded video packets with the buffer instead of duplicating the entire recording. MP4 conversion uses one reusable frame buffer, and trimmed audio packets are borrowed while writing. Before saving, the engine checks Windows committed-memory headroom and refuses the save if there is less than a 512 MB reserve plus the estimated audio workspace. A refusal shows the normal failed-clip popup and logs the reason; the existing buffer is retained. This reduces save-time memory spikes but cannot prevent other applications from exhausting Windows memory. A system-managed page file is recommended on machines that encounter virtual-memory exhaustion.

## Settings

The app checks for the WebView2 Runtime at startup. If it is missing, a persistent warning above the application pages links to Microsoft's download page. It has its own layout row and does not cover page content; it is hidden when WebView2 is installed. Capture and replay saving use the native engine independently of WebView2.

The scrollable Settings page puts replay target, video quality, audio and storage first. Encoder, capture method and logging are grouped under Advanced. Changes to recording settings take effect when applied, which restarts an active buffer. Returning to a page resets its scroll position.

Quality changes bitrate only. All encoder choices use the same maximum output bounds of **4K (3840 × 2160)**, preserve the source aspect ratio and do not upscale smaller sources. Auto no longer applies a hidden 900p limit. Actual 4K frame rates depend on encoder and system performance; the source must be 4K to produce 4K output. Presets use one shared mapping for both UI labels and native encoder parameters:

| FPS | Low | Balanced | High |
|---|---|---|---|
| 30 | 6 Mbps | 10 Mbps | 15 Mbps |
| 60 | 12 Mbps | 20 Mbps | 30 Mbps |
| 120 | 24 Mbps | 40 Mbps | 60 Mbps |

These are bitrate targets, not fixed output file sizes; static scenes can use less data. The AVC/H.264 path internally requests more bits per second than the HEVC tiers shown here (about 18 / 28 / 40 Mbps at 60 fps for Low / Balanced / High) so quality stays comparable. Existing `Fast` and `Quality` settings remain compatible with Low and High. Quality does not change the selected encoder or frame-pacing behavior.

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

The offline catalog in `assets/games/games.json` contains 8,460 game/client names and 8,967 Windows executable rules, derived from the [public Discord detectable-applications catalog](https://discord.com/api/v9/applications/detectable) with local Minecraft client rules. This is a recognition catalog, not a popularity ranking. Generic executable names require a matching game title; custom keywords distinguish shared executables such as `javaw.exe`. Launchers, Windows shell/input windows, cloaked/tool windows and Instant Replay itself are excluded. The catalog is loaded once into an immutable executable index. Only the foreground window is checked, custom rules are indexed too, and process names use a bounded cache; polling does not scan the full catalog or enumerate every process. Detection uses ordinary Win32 window/process queries in `InstantReplay.Core/GameDetector.cs`; it does not scan game memory.

Games on the anti-cheat list in `assets/games/anticheat_games.json` are never injected. The list covers kernel-level and injection-detecting anti-cheat (Vanguard, Easy Anti-Cheat, BattlEye, Ricochet, GameGuard, XignCode, Warden and similar); those titles stay on Windows Graphics Capture. If the list cannot be loaded, injection is disabled (fail closed) and the app logs a warning.

Minecraft Java runs inside `java.exe` or `javaw.exe`; its launcher executable is not the capture target. Built-in title rules cover vanilla Minecraft, Fast Client, Silent Client, Lunar Client, Badlion Client, Feather Client, TLauncher and Legacy Launcher, including compact client names where applicable. Built-in Minecraft Java rules also require an `LWJGL`, `GLFW30` or `SDL_app` game-window class so Java launcher/control windows are not selected. Games launched through TLauncher or Legacy Launcher that retain the ordinary Minecraft title use the vanilla rule. The recognized game must still be foreground to start or resume capture, and app audio follows the game process rather than its launcher. Explicit custom rules remain available for other Java applications.

Capture follows the game window in real time. Windowed and borderless-fullscreen games are recorded with Windows Graphics Capture on the game window; exclusive fullscreen is recorded with DXGI desktop duplication of the display the game is running on (the display is detected, so multi-monitor setups work). Both sources feed one encoder through a fixed canvas at the highest resolution the game can show, and smaller sources are centred with black bars, so switching modes mid-game produces one seamless video. `engine\hooks\bin\capture_hook.dll` is no longer used to capture video: it is no longer injected. `engine\hooks\bin\overlay_hook.dll` is loaded when an in-game card can be drawn; it hooks the game's present calls itself (D3D9/11/12/OpenGL), never reads frames, and shows the in-game clip notification. The overlay is loaded with the same anti-cheat compatibility scheme as OBS (a `WH_GETMESSAGE` hook on the game's window thread, no remote threads), and never into games on the anti-cheat list; there the regular popup notification is used.

Direct3D 9, Direct3D 11, Direct3D 12 and OpenGL additionally use `engine\hooks\bin\overlay_hook.dll` to draw the clip card after capture and before the real present, so the card is not recorded. The D3D12 overlay submits its own fenced command list on the tracked direct queue. Vulkan retains the external WinUI notification while its synchronized in-process renderer is unfinished. The overlay hook appends `[DEBUG]` lines to the app's debug log when debug logging is enabled.

Game Capture uses Windows Graphics Capture and, for supported Direct3D 9/11/12 and OpenGL games, loads the overlay into the game process to draw the clip card. Both request square window corners regardless of the stored rounded-corner preference; the previous Windows corner preference is restored when capture stops. Selecting Game Capture with DXGI configured displays a WinUI compatibility dialog and switches to WGC; DXGI remains available for Whole Desktop and exclusive fullscreen. Auto encoding tries NVIDIA NVENC, AMD AMF and Intel Quick Sync, then x264, then Microsoft's Media Foundation H.264 encoder. Process audio requires Windows build 20348 or later (Windows 11 supported); a process-audio failure is reported rather than falling back to system audio.

In hook mode, client-size, window-style, monitor and display-size changes rebuild only API-local readback and overlay resources. The shared frame ring, encoder, audio timeline and replay buffer stay alive, and each frame carries its current dimensions for scaling to the fixed output size. OpenGL context replacement is handled the same way, so games such as Minecraft can switch between windowed and fullscreen without restarting or discarding the buffer. The WGC path detects those changes, pauses while they settle for 600 ms, then recreates the window session and discards the previous buffer. WGC skips stale resize frames before rebuilding its frame pool and processes the newest available frame from its bounded pool after capture stalls, rather than encoding a backlog. Capture startup verifies that frames arrive and retries failed window sessions with a cooldown. Whole Desktop does not switch capture targets or add special recovery for individual games.

**Fullscreen capture limitation:** Windowed and borderless games are captured with Windows Graphics Capture; exclusive fullscreen is captured with DXGI Desktop Duplication. WGC may be unable to capture exclusive fullscreen content when the game owns the display. Video may appear black or frozen while audio continues. Try **Whole Desktop** or use borderless/windowed mode. Protected content may remain unavailable.

Capture follows fresh compositor/desktop timestamps, skips superseded frames and rejects duplicate or out-of-order timestamps. It does not create filler frames on a timer. During a stationary scene, the container holds the last valid image. The FPS setting is a maximum, so static scenes produce fewer encoded frames. Encoders come from FFmpeg, and GPU sample buffers are recycled only after the encoder releases them.

Replay saves show a compact card at the top left with the app icon. Successful saves show **Clipped Last N Seconds**, using the saved duration; failed saves show **Failed To Clip** and **See debug log for more details**, with the reason recorded in the log. When a Direct3D 9, Direct3D 11, Direct3D 12 or OpenGL hook is active the card is drawn inside the game with the same colors, icon, live encode percentage and morph/slide transitions as the WinUI popup; otherwise the popup is a WinUI window over the foreground display. The card slides in from the left, remains visible for four seconds, then slides out without activating or stealing game focus. The WinUI popup requests capture exclusion and is raised above borderless/fullscreen windows on a best-effort basis; legacy exclusive fullscreen can prevent it from appearing, and a popup failure never invalidates the saved clip. Other warnings retain their existing notification path.

Click a gallery replay to play inside the app. The fullscreen button or F switches views; Escape returns from fullscreen. Switching preserves playback position, play/pause state, volume and mute. The inline player follows the recorded aspect ratio with rounded video corners; fullscreen includes an exit control in the corner. Volume waves fade in sequence as volume decreases: outer wave first, then inner wave, then the muted indicator.

## Updates

The **Updates** page sits at the bottom of navigation. **Check for updates** reads the latest stable release from `ghostneverdies/InstantReplay` on GitHub. Numeric version comparison treats `v1.5` and `1.5.0` as equal and offers only newer versions, never a downgrade. Automatic checks run when due at startup and every 24 hours while the app runs; the last attempt is persisted across restarts. Failed checks do not interrupt recording, and manual checks can retry immediately.

When a newer release is found, a WinUI dialog over the app offers **Update now** or **Not now**. The reminder appears at most once per 24 hours, including across restarts and repeated manual checks. Reminders wait until the app is in foreground use rather than bringing it above a game or opening it from the tray. Declining keeps recording and leaves the update available on the Updates page.

**Download and install** asks before restarting, downloads the complete `InstantReplaySetup.exe`, and verifies its byte count and GitHub-provided SHA-256 digest. Recording continues during download. After verification, a temporary local PowerShell helper waits for the app to fully exit, runs the installer with `/S -quiet /UPDATE=1 /D=<current app directory>`, waits for installation to finish, and reopens the app as the original user. Windows administrator approval can still appear. Save any replay you want before installing: the in-memory buffer is cleared on exit. Cancellation or download failure leaves the running app untouched; cancelling elevation reopens the existing app and reports the failure on the Updates page.

Releases are built by manually running the **Release** workflow with a numeric version such as `1.6.0`; it creates the `v1.6.0` tag and release and stamps that version into both assets. Upload `InstantReplaySetup.exe` as a release asset with a SHA-256 digest available through GitHub's API. No token, VPS, patching plugin, or GitHub credentials are embedded in the app. The installer source supports `-quiet` as well as NSIS's `/S`; update mode skips the finish page and browser launch and preserves existing startup and shortcut preferences. It overwrites app files directly instead of launching the uninstaller. Settings and saved recordings are retained.

When preparing a release, keep the committed fallback versions in `InstantReplay.csproj` and `installer/installer.nsi` close to the version you dispatch. The workflow stamps the input version into both, but local builds use the committed values. The updater reads the app's assembly version; uploading an older app under a newer tag would cause it to keep offering that update. There is no automatic installer rollback. A failed installation can be repaired by rerunning the full installer.

## Files the app writes

| Path | Contents |
|---|---|
| `%APPDATA%\InstantReplay\settings.json` | Settings |
| `%APPDATA%\InstantReplay\updates.json` | Last update check, last reminder time and cached release metadata |
| `%APPDATA%\InstantReplay\update-result.json` | Installer result, consumed after reopening |
| `%TEMP%\InstantReplay-Updates\<unique folder>` | Verified installer and temporary local update helper |
| `Pictures\Instant Replay` (default) or your chosen folder | Saved replay folders and video files (MP4, MOV, MKV, MPEG-TS or FLV) |
| `%TEMP%\thumbnails` | Cached replay preview images |
| `%TEMP%\debug_<timestamp>.txt` | Log file |
| `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` | Start with Windows entry |

## Environment variables

- `IR_FFMPEG_DIR=<path>` loads the FFmpeg DLLs from another folder instead of `engine\encoder\`
- `IR_IMPROVED_CONVERT=1` uses the higher-quality two-pass GPU color conversion
- `IR_LEGACY_ENCODER=1` bypasses FFmpeg and uses the previous built-in Media Foundation encoder and MP4 writer (debugging only). `IR_GPU` and `IR_ENCODER` only apply to that legacy path.

## Third-party code

The capture/encoding approach and embedded, compressed shader bytecode are adapted from [mmozeiko/wcap](https://github.com/mmozeiko/wcap), released into the public domain under the Unlicense. `engine/engine_core/shaders/wcap_shaders.hlsl` is retained as reference source. Runtime code decompresses embedded bytecode with `D3DDecompressShaders`; it does not compile HLSL at runtime. The engine depends on the Windows Rust bindings and `libloading` (to load FFmpeg at runtime) and does not use `windows-capture` or a WASAPI wrapper crate.

The Rust DLL keeps the existing `IR_*` ABI and provides `IR_SetCaptureTarget`, `IR_SetPaused`, `IR_GetWindowCount` and `IR_GetWindowInfo`. Set the target before `IR_Start`: HWND 0 means monitor capture. Capture options are 1=client area, 2=cursor, 4=border, 8=keep rounded corners, 16=secondary windows, 32=legacy automatic active-window selection for existing DLL callers, and 64=process audio for the supplied HWND. The frontend uses explicit HWND targets and never sets bit 32. `IR_SetPaused` gates video and app audio; enabled microphone capture continues independently. Window-only options require the corresponding Windows capture APIs; unsupported optional flags are ignored.

### FFmpeg

Instant Replay uses [FFmpeg](https://ffmpeg.org/) for video/audio encoding and container writing. Release builds bundle the four required shared libraries (`avcodec`, `avformat`, `avutil` and `swresample`) under `engine\encoder\`; the Rust engine loads them by name at runtime. They come from the pinned BtbN GPL shared build described in [Build](#build), which includes libx264. The portable archive and installer therefore work offline and do not download FFmpeg on first run. The FFmpeg license and source/build links are retained in [engine/encoder/FFMPEG-LICENSE.txt](engine/encoder/FFMPEG-LICENSE.txt).

- FFmpeg source code: <https://ffmpeg.org/download.html>
- Build scripts and configuration of the distributed DLLs: <https://github.com/BtbN/FFmpeg-Builds>
- x264 source code: <https://code.videolan.org/videolan/x264>

### OBS Studio reference

The in-process graphics interception design was independently implemented for Instant Replay after studying [OBS Studio's Windows graphics-hook sources](https://github.com/obsproject/obs-studio/tree/master/plugins/win-capture/graphics-hook). The Rust `cdylib` implementation is kept in `engine/hooks/capture_hook/src/capture_hook.rs`, uses Instant Replay's own shared-texture, fallback-ring and frame-pacing contracts, and is modified for this recorder rather than copied verbatim. Direct3D 11 and OpenGL use named D3D11 textures with keyed synchronization; OpenGL bridges through `WGL_NV_DX_interop2`. Direct3D 9 and Direct3D 12 retain guarded CPU fallback paths. Vulkan currently has present interception only and remains routed to WGC until its synchronized image/queue path is complete. OBS Studio is copyright its contributors and distributed under the GNU General Public License; see its repository for the authoritative notices and source.

The DXGI cursor compositor in `engine/engine_core/src/capture_dxgi.rs` likewise follows the OBS monitor-capture strategy of retaining the desktop texture and compositing separately cached pointer state. Instant Replay limits CPU readback/upload to the pointer rectangle and preserves DXGI color, masked-color and monochrome semantics.

## License

Instant Replay is licensed under the **GNU General Public License version 3**; see [LICENSE](LICENSE). Bundled third-party components retain their respective upstream licenses and notices. FFmpeg distribution details are described [above](#ffmpeg), and the installer displays [installer/terms.txt](installer/terms.txt).

## Donations

USDT (TRC-20): `TBLWg9dJtimMijGJtSKGX61DtmZbYaDbkD`

