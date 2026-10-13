# Security

Instant Replay is a free, open-source replay recorder. This document describes the app's capture behavior, update access, local data and installer permissions.

## What the app does

- Captures the screen and audio continuously while recording is running, and keeps the most recent 30, 60 or 120 seconds in memory.
- Writes an MP4 to disk only when you save a replay (hotkey, button or tray menu). Existing output files are not overwritten or deleted by a failed save.
- Whole Desktop captures the selected monitor and system audio, without game detection or focus rules.
- Game Capture captures eligible foreground games from an offline catalog and user-added executable/title rules. Changing to another eligible window clears the old buffer and changes process-audio capture to that window's process tree. Unlisted foreground apps pause game video/audio; returning to the same game resumes the retained buffer. Closing the captured window clears its buffer and returns to waiting.
- Shell/input, Explorer, cloaked/tool/untitled windows and the recorder are excluded. These are selection rules, not a security boundary. Custom executable rules match filenames, not signed identities or full executable paths.
- An enabled microphone keeps recording during game pauses. Saved footage holds the last game image during that interval to preserve audio synchronization. Stop replay or disable microphone recording when microphone capture is unwanted.
- Game Capture uses Windows process-loopback audio (build 20348+). It never silently substitutes system audio when process capture fails. Process trees can include audio from child processes. Whole Desktop audio can include every application.
- Game Capture recreates its session and discards the buffered replay after a settled window-mode, size or display change. It rechecks foreground eligibility before restarting and does not switch to desktop capture when a game session fails.
- Detection checks only the foreground window, using an immutable executable index and bounded process-name cache. It reads window metadata, executable paths and window properties using Win32 APIs. It does not read game memory, inject DLLs, run selected executables or require elevated privileges.
- Built-in Minecraft Java/client rules match `java.exe` or `javaw.exe`, a recognized game/client title, and a native `LWJGL`, `GLFW30` or `SDL_app` game-window class. A launcher title or Java executable alone does not authorize capture. Audio is scoped to the actual game process tree. Window classes and titles are selection hints, not signed application identities; explicit custom rules retain their user-defined scope.
- Registers one global hotkey (default `Ctrl + Numpad *`). It does not install a keyboard hook and does not read keystrokes.
- Runs as a normal user process (`asInvoker`), with a tray icon. Save successes and failures show a transient popup above other windows without activating it. It requests exclusion from screen captures, although Windows capture exclusion is not a security guarantee. The popup does not inject into games; exclusive fullscreen visibility is best effort.
- A named mutex allows one running instance. Additional launches signal the existing window and exit before creating a capture engine. An abandoned mutex can be acquired after a crash; the app does not periodically terminate other processes by name.
- Optionally starts with Windows through a per-user `Run` registry entry (`--tray`).
- Shows a memory warning if usage stays high, and writes a log file to `%TEMP%`.

## What the app does not do

- No runtime network access for recording or game detection. The bundled recognition catalog is read locally; it is not fetched or updated from Discord at runtime. Recordings are not uploaded.
- No telemetry, analytics or tracking.
- No background services or scheduled tasks.
- No modification of system files or Windows settings beyond its own registry entries.
- The capture engine does not use FFmpeg, game injection, Java helpers or virtual audio drivers. The app bundles its WinUI runtime and requires WebView2 for local replay playback.

The GitHub button opens the project's page in your browser; an interactive installer can also open it after installation. When the WebView2 Runtime is missing, a persistent warning has a **Download WebView2** button that opens Microsoft's official Runtime download page. The app does not automatically download or install WebView2.

## Updates and network access

The app contacts GitHub's public Releases API for `ghostneverdies/InstantReplay` when you request an update check and automatically at most once per 24 hours while running. Manual checks may run more often. It sends the app version in its User-Agent and uses no embedded access token. GitHub receives the connection's public IP address. Release metadata and check times are cached locally; failures do not stop recording. Update checks do not upload settings, game information, audio, or recordings.

Installing requires an explicit **Download and install** or **Update now** action. The app accepts only newer numeric stable versions and the expected `InstantReplaySetup.exe` asset from this repository's HTTPS release URLs. Download redirects must remain on supported GitHub asset hosts. It checks download size and SHA-256 before handing off, and the helper rechecks the hash before execution. These checks detect corrupted or mismatched downloads; they rely on GitHub and the repository owner's account and are not an independent publisher signature. The updater refuses automatic installation when the release lacks a valid checksum.

A WinUI reminder can also offer **Update now** and **Not now**, at most once every 24 hours. Its timestamp is persisted independently of update checks. It waits until the app is in foreground use. Declining performs no download, installation, or capture restart; choosing **Update now** authorizes the same verified installer flow.

A locally generated, temporary PowerShell helper runs without a visible console, waits for the app to exit, requests elevation for the NSIS installer, and reopens the app under the original user after the installer exits. The helper uses `-NoProfile -NonInteractive -ExecutionPolicy Bypass` for its own locally written script; it does not change the machine's execution-policy settings or download scripts. Administrator approval remains controlled by Windows. Installation clears the current in-memory replay buffer. Download cancellation leaves capture running. No updater service or scheduled task is installed.

## Capture implementation

The native Rust DLL uses Windows Graphics Capture, DXGI, D3D11, Media Foundation and WASAPI directly. Shaders come from wcap as embedded compressed bytecode; source shaders are not compiled at runtime. Only fresh source frames are encoded, and idle presentation time is represented in the MP4 timeline. GPU sample buffers are tracked until Media Foundation releases them.

WGC requests the same 1 ms update interval as original wcap where the Windows API supports it. Unsupported Windows versions retain their capture default. Both capture methods allow bounded timestamp jitter while rejecting duplicate and stale timestamps. Capture stalls do not trigger artificial catch-up frames. These changes do not inject code, require capture elevation, bypass protected content, or guarantee a particular frame rate.

Gallery previews are generated locally through Windows thumbnail services or direct extraction of a video frame. Unreadable cached previews are regenerated. Thumbnail generation does not upload video or preview images.

Saving retains shared references to immutable encoded video packets while the ring continues capturing. MP4 conversion uses a reusable per-frame buffer and borrows clipped audio bytes, avoiding full-video payload copies. The engine queries Windows committed-memory availability before taking a save snapshot; insufficient headroom returns a low-memory failure before writing replay data. This guard preserves the replay buffer and does not change Windows page-file settings, terminate other applications, or guarantee recovery from system-wide exhaustion. Failure details stay in the local log.

The app arms replay on startup and can continue while minimized to the tray. Game Capture waits for an eligible foreground target; Whole Desktop begins recording the selected monitor. Stop replay buffering from the app when capture is no longer wanted. Window filtering does not remove sensitive content inside an eligible application, and microphone input may contain ambient audio. Protected content and some fullscreen games may not be available through Windows capture APIs.

## Data on disk

| Location | Contents |
|---|---|
| `%APPDATA%\InstantReplay\settings.json` | Settings |
| `%APPDATA%\InstantReplay\updates.json` | Check time, reminder time and cached release metadata |
| `%APPDATA%\InstantReplay\update-result.json` | Result consumed after restarting |
| `%TEMP%\InstantReplay-Updates\<unique folder>` | Downloaded installer and local helper, removed after completion when possible |
| `Pictures\Instant Replay` (default) or your chosen folder | Saved replay folders and MP4 files |
| `%TEMP%\thumbnails` | Cached video-frame previews |
| `%TEMP%\debug_<timestamp>.txt` | Log file |

Preview images can contain the same sensitive content as their source recordings. Local logs can include replay paths, capture target names, microphone device names and failure details. Review logs before sharing them publicly.

## Installer

`InstantReplaySetup.exe` is built with NSIS from `installer/installer.nsi` and requires administrator rights. It:

1. Copies the app to `Program Files\Instant Replay`
2. Registers an Instant Replay uninstall entry and an `.ir` file type entry under `HKLM`
3. Adds a start-with-Windows entry under `HKCU`
4. Creates Start Menu and optional desktop shortcuts
5. Opens the project's GitHub page after an interactive install

Existing-install detection and the destructive rollback script have been removed. Installation writes directly into the selected directory. Silent update mode (`/S -quiet /UPDATE=1`) shows no installer wizard or finish page, preserves startup and shortcut preferences, and leaves restart to the non-elevated update helper. It does not uninstall the previous version or delete settings and recordings. If installation fails partway through, there is no automatic rollback; rerunning the full installer can repair the app files.

The uninstaller stops the app, removes the registry entries, shortcuts and install folder, and can optionally delete recordings in the default `Pictures\Instant Replay` folder. It leaves `%APPDATA%\InstantReplay` in place.

## Why antivirus software may flag it

Screen capture, a global hotkey, and a startup entry are patterns also seen in malware. Everything the app does is in this repository.

## Reporting

Open an issue on the GitHub repository.
