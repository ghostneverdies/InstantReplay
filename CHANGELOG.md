# Changelog

All notable changes to Instant Replay are documented in this file.
Newest entries go on top.

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
