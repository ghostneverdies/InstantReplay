# Changelog

All notable changes to Instant Replay are documented in this file.
Newest entries go on top.

## Unreleased

### New

- **C++ engine.** `engine_core` is rewritten from scratch in C++17 and built with MSVC through `engine\engine_core\build.bat` (no CMake, no Rust toolchain for the engine). The `IR_*` ABI is unchanged apart from stricter pre-start semantics for `IR_SetCaptureTarget` and `IR_SetCanvasHint`. FFmpeg is linked through its own headers and import libraries and delay-loaded, so there are no generated bindings.
- **OBS video pipeline.** Scaling and NV12 conversion now use shaders hand-translated from OBS Studio's `.effect` files (bicubic scale, `NV12_Y` and `NV12_UV`), rendered through a canvas, a scale pass and a staging ring read back one frame late. Output is limited-range BT.709 (BT.601 below 720p) like OBS, and the stream is tagged to match. The Media Foundation fallback now takes frames from the same pipeline through system memory rather than GPU textures.
- **Encoder detection.** The first launch (and any launch where `settings.json` has no encoder data) probes every encoder by pushing real frames through it, while a blurred "Detecting Encoders" overlay covers the window and blocks input. The result is stored in `settings.json` under `encoders` as true/false per encoder (`h264_nvenc`, `hevc_nvenc`, `h264_amf`, `hevc_amf`, `h264_qsv`, `hevc_qsv`, `libx264`, `libx265`, `h264_mf`, `hevc_mf`). **Advanced > Refresh encoders** runs it again, and encoders that failed detection are greyed out in the encoder list.
- **Auto fallback chain.** Auto uses only encoders that passed detection, in this order: NVIDIA NVENC, AMD AMF, Intel Quick Sync, the hardware Media Foundation encoder (needed for 13th-gen and later Intel graphics and Arc, which FFmpeg 5.1's Quick Sync cannot drive), then x264. An explicitly chosen encoder never falls back and is attempted regardless of the stored detection result.
- **Media Foundation is no longer a choice.** It is driven directly through the Media Foundation API (not FFmpeg) and is used only as an Auto fallback. A saved `MediaFoundation` choice migrates to Auto.

- **Reworked home page.** A single hero card now carries the status pill, the hotkey pill, the monitor/refresh artwork and the **Save Replay** / **Start Replay** actions. Live CPU and memory cards underneath adopt the system accent colour and keep a short sparkline history; the sparklines switch to the alert colour at 90% or above.

### Fixed

- **Settings page could crash while scrolling.** Every visit to Settings re-applied a staggered entrance animation that put a `CompositeTransform` on each card. That animating layer clashed with mouse-wheel scrolling inside the page's `ScrollViewer` and could take the process down with a stowed exception in `Microsoft.UI.Xaml.dll`. Settings now uses only the page-level fade.

### Changed

- **FFmpeg 5.1.2.** The engine now loads `avcodec-59`, `avformat-59`, `avutil-57` and `swresample-4` from the pinned gyan.dev 5.1.2 shared build (libmfx Quick Sync works on 4th- through 12th-gen Intel CPUs). `engine\encoder\fetch-ffmpeg.ps1` replaces old binaries with it, `ffmpeg_sys.rs` is generated from its headers, and the release workflow downloads and verifies it.
- **Installer keeps the install folder clean.** The whole publish payload now lives in `<install dir>\ship`, and the install root holds only an `InstantReplay.lnk` launcher and that folder; the uninstaller moved to `ship\Uninstall.exe`. Start Menu and desktop shortcuts launch `ship\InstantReplay.exe`, start with Windows and `.ir` associations use the same path, and updates keep the layout. The hook DLLs now ship and load from `engine\hooks\bin\`. Updating an older flat install migrates it into `ship` once and removes the old root files.
- **Source layout.** Small related files were consolidated into cohesive files, each still using a single namespace:
  - `InstantReplay.Models/CaptureModels.cs` — `CaptureMethod`, `CaptureSource`, `WindowMode`, `CaptureSurface`, `CaptureWindow`, `WindowSnapshot`
  - `InstantReplay.Models/UpdateModels.cs` — `UpdateRelease`, `UpdateCheck`, `UpdateInstallResult`
  - `InstantReplay.Models/VideoSettings.cs` — `EncoderPreference`, `VideoQualityProfile`
  - `InstantReplay.Models/SaveReplayResult.cs` — now also contains `SaveReplayOutcome`
  - `InstantReplay.GUI/Shell/TaskbarBadge.cs` — now also contains `BadgeKind`
  - `InstantReplay.GUI/Helpers/TitleBarButtons.cs` — now also contains `Chrome`
  - `InstantReplay.Native/IrNative.cs` — now also contains the audio endpoint COM interop (`MMDeviceEnumeratorComObject`, `IMMDeviceEnumerator`, `PropertyKey`, `IMMNotificationClient`)
  - `InstantReplay.Services/WindowsIntegration.cs` — `WindowsTheme` and `StartupManager`
- Temporary first-chance and `AppDomain` crash-diagnostic hooks were removed from `App.xaml.cs`.
- **Releases are manual.** `.github/workflows/release.yml` now runs only from `workflow_dispatch`; tag pushes and published releases no longer start it. A run takes the version as input, stamps it into the app and installer, creates tag `v<version>` at the dispatched commit and publishes the GitHub Release, so the in-app updater keeps reading the latest stable release.

### Notes

- No settings-format or saved-file changes; existing `settings.json` files and replays are compatible.

## 1.6 - 2026-11-04

### New

- **In-game graphics capture.** Supported Direct3D 9, Direct3D 11, Direct3D 12 and OpenGL games (including Minecraft Java) are captured inside the game process: `capture_hook.dll` reads the back buffer after the game renders and before it presents. Exclusive fullscreen and windowed play both work without a Windows Graphics Capture session, and switching windowed/fullscreen no longer restarts the buffer. Vulkan and any game where the hook cannot start use WGC until a synchronized direct path is complete.
- **Anti-cheat-aware injection.** Games protected by kernel-level or injection-detecting anti-cheat are never injected. The data-driven list lives in `assets/games/anticheat_games.json` (Vanguard, Easy Anti-Cheat, BattlEye, Ricochet, GameGuard, XignCode, Warden and similar); those titles stay on WGC. If the list cannot be loaded, injection is disabled and a warning is logged.
- **In-game clip card.** `overlay_hook.dll` draws the same card as the WinUI popup (colors, icon, live encode percentage, morph to success/error, entrance and exit slides) directly into the game's back buffer after the frame has been read, so the card is never recorded. It installs no keyboard or mouse hooks and does not change game focus. Without the hook, the existing WinUI popup is used.
- **Hook diagnostics.** When debug logging is enabled, both hooks append `[DEBUG]` lines to the app's debug log (load, hook enablement, frame-ring creation, capture size, card show/hide and errors).
- **FFmpeg-based encoding and file writing.** Video encoding, audio compression and the final file are now handled by FFmpeg 8.1, loaded at runtime from the `encoder` folder next to the app (no static linking). The encoder is chosen automatically (NVIDIA NVENC, then AMD AMF, Intel Quick Sync, x264 and finally Media Foundation) and works on any GPU vendor.
- **Advanced settings:** *Video encoder* (Auto, NVIDIA NVENC, AMD AMF, Intel Quick Sync, x264 CPU, Media Foundation), *Rate control* (CBR or VBR), *Encoder preset* (Speed, Balanced, Quality) and *Video container* (MP4, MOV, MKV, MPEG-TS or FLV). Encoder, rate control and preset apply when recording restarts; the container applies to the next saved replay.
- **Per-encoder option mapping.** Every Advanced setting is translated to the private options each encoder actually exposes (for example NVENC `p1`–`p7` presets and dashed `forced-idr`, AMF `usage`/`quality`/`forced_idr`, QSV `low_delay_brc`, Media Foundation `quality`/`scenario`, x264 `nal-hrd`), so choosing an encoder produces a matching bitrate, latency and keyframe policy instead of silently ignored options.
- **AAC audio.** Saved replays now contain AAC audio instead of uncompressed PCM, which makes files considerably smaller.
- **Clear errors.** If the FFmpeg DLLs are missing, are the wrong version, or no H.264 encoder can be opened, recording does not start and the app says why. There is no silent fallback.
- **FFmpeg license and acknowledgment** in the README, the installer terms and `encoder\FFMPEG-LICENSE.txt` (GPL build; source links included).
- **Tag-driven releases.** `.github/workflows/release.yml` builds the portable ZIP and the installer on every version tag, stamps the tag version into the app and installer, verifies the pinned FFmpeg archive's SHA-256, and attaches both files to the GitHub Release.

### Fixed

- **Output bitrate was lower than the selected quality preset.** Encoders now use constant bitrate (CBR) by default instead of variable bitrate, which was undershooting. Very static scenes can still come in under target.
- **Exclusive-fullscreen hooked games.** The in-game hook removes the WGC black/frozen-frame problem for supported Direct3D 9, Direct3D 11, Direct3D 12 and OpenGL titles; WGC remains the fallback for unsupported or protected games.
- **Hook debug lines no longer overwrite app log lines.** Both processes append with `FILE_APPEND_DATA`, so `[DEBUG]` lines from the injected hooks interleave correctly with the app's own log.
- **Clip notification lifetime** is four seconds for success and failure, matching the WinUI card.

### Notes

- **FFmpeg DLLs are required.** Place `avcodec-62.dll`, `avformat-62.dll`, `avutil-60.dll` and `swresample-6.dll` from the pinned FFmpeg 8.1 GPL shared build in `engine\encoder\` before building (see README, *Build*). They are bundled as `encoder\*.dll`.
- The previous Media Foundation encoder, the direct NVENC encoder and the built-in MP4 writer remain in the source but are unused. `IR_LEGACY_ENCODER=1` re-enables the Media Foundation path and MP4 writer for debugging.
- Clips may begin up to one second (one keyframe interval) before the requested start, with audio covering the same span, so audio and video always stay in sync.
- MKV playback depends on the system and the in-app player; MP4 is the safest choice for sharing.
- Preset bitrates are unchanged, but because CBR now holds them as a target, clips can be somewhat larger than in 1.5 on simple scenes.

## 1.5 - 2026-11-01

### New

- **NVIDIA NVENC hardware encoding.** On NVIDIA GPUs, Instant Replay encodes H.264 directly through the NVENC SDK. Frames go from capture to the encoder entirely on the GPU, with no CPU copies. Requires NVIDIA driver 522.25 or newer.
- **Automatic fallback.** If NVENC isn't available (non-NVIDIA GPU, old driver, or an error), the app falls back to Media Foundation as before.
- **Better diagnostics.** The log shows which encoder is active, and the failure code if NVENC couldn't start.
- **Advanced options (environment variables):**
  - `IR_NVENC=0` turns NVENC off.
  - `IR_NVENC_PRESET=1..7` picks the speed/quality preset (default `5`).

### Fixed

- **Window opened at the wrong size when restored from the tray.** Double-clicking the tray icon now restores the window at its normal size.
- **"We couldn't create the data directory" WebView2 error.** WebView2 no longer writes to `Program Files`. It now uses `%TEMP%\InstantReplay\WebView2` and recreates it if it gets cleaned up.
- **Title bar close button misplaced.** The close button now sits flush with the window edge and follows the rounded corner.

### Performance

- **Zero-copy DXGI capture.** Previously every frame was copied from the GPU to the CPU and back, causing frame drops at high resolutions. Frames now stay on the GPU, and only the small area around the mouse cursor touches the CPU. This applies when capture and encoding run on the same GPU; otherwise the previous path is used. Set `IR_DXGI_READBACK=1` to force the previous path.
- **Constant frame rate output (DXGI and WGC).** If no new frame arrives within a frame interval, the previous frame is repeated. Clips now report the target frame rate (for example 60 fps) instead of a lower average. Repeats stop while recording is paused.

### Notes

- Preset bitrates are unchanged (High at 60 fps is 9000 kbps at any resolution).
- Static scenes use slightly more CPU/GPU than before, because a repeated frame is encoded for every interval with no new frame.
- AMD AMF support is not included yet.
