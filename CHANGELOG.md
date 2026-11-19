# Changelog

All notable changes to Instant Replay are documented in this file.
Newest entries go on top.

## Unreleased

### New

- **C++20 engine.** `engine_core` is built with MSVC through `engine\engine_core\build.bat` (no CMake, no external encoder SDK). The `IR_*` ABI is unchanged. Video and audio encoding, muxing and MP4 output all use Windows Media Foundation directly.
- **GPU video pipeline.** Scaling and NV12 conversion use static compute-shader bytecode captured from the wcap pipeline, rendered through a video-thread-owned canvas, a scale pass and a staging ring read back one frame late. Output is limited-range BT.709 (BT.601 below 720p) and the stream is tagged to match. Hardware MFTs consume the NV12 textures directly; system-memory MFTs take frames from the same pipeline.
- **Encoder detection.** The first launch (and any launch where `settings.json` has no encoder data) probes every Media Foundation MFT by pushing real frames through it, while a blurred "Detecting Encoders" overlay covers the window and blocks input. The result is stored in `settings.json` under `encoders` as true/false per codec and vendor (`h264_mf_qsv`, `hevc_mf_qsv`, `h264_mf_vp12`, `hevc_mf_vp12`, `h264_mf_nvenc`, `hevc_mf_nvenc`, `h264_mf_amf`, `hevc_mf_amf`, and the `av1_mf_*` / `*_mf_software` keys). **Advanced > Refresh encoders** runs it again, and encoders that failed detection are greyed out in the encoder list.
- **Auto fallback chain.** Auto uses only MFTs that passed detection, in vendor order: Intel Quick Sync, Intel Gen12 / Arc (VP12), NVIDIA, AMD, then the system-memory software MFT. An explicitly chosen encoder never falls back.
- **Media Foundation encoder options.** Rate control (CBR or peak-constrained VBR), mean and maximum bitrate, GOP size, B-frames and low-latency mode are configured through `ICodecAPI`.

- **Reworked home page.** A single hero card now carries the status pill, the hotkey pill, the monitor/refresh artwork and the **Save Replay** / **Start Replay** actions. Live CPU and memory cards underneath adopt the system accent colour and keep a short sparkline history; the sparklines switch to the alert colour at 90% or above.

### Fixed

- **Settings page could crash while scrolling.** Every visit to Settings re-applied a staggered entrance animation that put a `CompositeTransform` on each card. That animating layer clashed with mouse-wheel scrolling inside the page's `ScrollViewer` and could take the process down with a stowed exception in `Microsoft.UI.Xaml.dll`. Settings now uses only the page-level fade.

### Changed

- **MP4 output only.** A single `IMFSinkWriter` muxes compressed H.264/HEVC/AV1 video and AAC audio into MP4. Saved clips always use the `.mp4` extension; the Advanced container picker is fixed to MP4.
- **MIT license.** The project is now licensed under the MIT License. The installer notice and documentation were updated to match.
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

- Stored encoder preferences from older builds migrate to **Auto**, and saved probe results are discarded once so the new Media Foundation encoder set is detected fresh. Existing saved replays remain compatible.
- Debug logs report the native capture buffer's memory usage, not process-wide memory usage.

## 1.6 - 2026-11-04

### New

- **In-game graphics capture.** Supported Direct3D 9, Direct3D 11, Direct3D 12 and OpenGL games (including Minecraft Java) are captured inside the game process: `capture_hook.dll` reads the back buffer after the game renders and before it presents. Exclusive fullscreen and windowed play both work without a Windows Graphics Capture session, and switching windowed/fullscreen no longer restarts the buffer. Vulkan and any game where the hook cannot start use WGC until a synchronized direct path is complete.
- **Anti-cheat-aware injection.** Games protected by kernel-level or injection-detecting anti-cheat are never injected. The data-driven list lives in `assets/games/anticheat_games.json` (Vanguard, Easy Anti-Cheat, BattlEye, Ricochet, GameGuard, XignCode, Warden and similar); those titles stay on WGC. If the list cannot be loaded, injection is disabled and a warning is logged.
- **In-game clip card.** `overlay_hook.dll` draws the same card as the WinUI popup (colors, icon, live encode percentage, morph to success/error, entrance and exit slides) directly into the game's back buffer after the frame has been read, so the card is never recorded. It installs no keyboard or mouse hooks and does not change game focus. Without the hook, the existing WinUI popup is used.
- **Hook diagnostics.** When debug logging is enabled, both hooks append `[DEBUG]` lines to the app's debug log (load, hook enablement, frame-ring creation, capture size, card show/hide and errors).
- **Advanced settings:** *Video encoder* (Auto plus hardware and software Media Foundation MFTs), *Rate control* (CBR or VBR) and *Encoder preset* (Speed, Balanced, Quality). Encoder, rate control and preset apply when recording restarts.
- **AAC audio.** Saved replays contain AAC audio instead of uncompressed PCM, which makes files considerably smaller.
- **Clear errors.** If no encodable Media Foundation MFT can be opened, recording does not start and the app says why. There is no silent fallback.
- **Tag-driven releases.** `.github/workflows/release.yml` builds the portable ZIP and the installer on every version tag, stamps the tag version into the app and installer, and attaches both files to the GitHub Release.

### Fixed

- **Output bitrate was lower than the selected quality preset.** Encoders now use constant bitrate (CBR) by default instead of variable bitrate, which was undershooting. Very static scenes can still come in under target.
- **Exclusive-fullscreen hooked games.** The in-game hook removes the WGC black/frozen-frame problem for supported Direct3D 9, Direct3D 11, Direct3D 12 and OpenGL titles; WGC remains the fallback for unsupported or protected games.
- **Hook debug lines no longer overwrite app log lines.** Both processes append with `FILE_APPEND_DATA`, so `[DEBUG]` lines from the injected hooks interleave correctly with the app's own log.
- **Clip notification lifetime** is four seconds for success and failure, matching the WinUI card.

### Notes

- Clips may begin up to one second (one keyframe interval) before the requested start, with audio covering the same span, so audio and video always stay in sync.
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
