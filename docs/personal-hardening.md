# Personal fork hardening

This fork retains the original noncommercial license and HGhub attribution.

The personal Windows/Android build is version 1.2.2, distinct from the imported 1.2.1 snapshot.

- Windows sessions use current-user DPAPI encryption (`session.dat`) and atomic replacement. Failed saves are reported without switching the active session. Legacy `session.json` is deleted and requires a fresh login. Other platforms keep credentials in memory only, so Android requires login after the process exits.
- Downloads check both declared and decoded body sizes: images 8 MiB, playlists 2 MiB, media segments 32 MiB, business JSON 8 MiB, update/mirror metadata 2 MiB. Four simultaneous media fetches are allowed; queued requests wait up to 20 seconds. Oversize errors are not retried.
- HLS rewriting also limits input lines to 16 KiB and expanded output to 8 MiB. Network target URLs are limited to 8 KiB. Large/expanded malicious manifests are rejected instead of exhausting memory.
- Memory cache payload plus URL keys are capped: images 64 MiB, playlists 8 MiB, keys 8 MiB, segments 64 MiB, in addition to existing entry limits. These limits exclude WebView buffers, in-flight bodies, decoder memory and response copies.
- `debug.log` rotates at 2 MiB, retaining one previous file; each incoming message is truncated safely at 16 KiB. Playback diagnostics retain their existing limits.
- Strict mode rejects opaque `Origin: null`, matching the original security policy. Native compatibility is checked on the target devices; no global bypass is introduced.
- Update checks and the About repository link point to `MichaelHo2520/study-materials-fork`. No release means no offered update; upstream releases are not substituted.
- Cache-clear and persisted-session logout failures are shown rather than reported as success. The cache description no longer suggests that history is cleared.

Validation: frontend type/build checks, browser HTML/security/API regressions, Windows native Rust tests (including DPAPI roundtrip, overwrite, logout, legacy cleanup and failed save), bounded HTTP-body and cache tests, and log rotation tests. Local network fixtures use no real account. Cross-platform Rust tests remain subject to each platform's native environment.

The personal workflow builds Windows and ARM64 Android artifacts, without publishing a release. APK signing is performed locally with `scripts/sign-android.ps1`; the key stays under `%LOCALAPPDATA%/HGhubSigning` and its password is protected with current-user DPAPI. Private signing material is never committed or uploaded. Inherited automatic iOS/macOS publishing on main pushes is disabled; those workflows remain available manually. Upstream availability and real account/playback behavior are separate from these local checks.

Local browser validation on 2026-11-12: 82 HTML/security-setting/logout checks passed (Windows and macOS user-agent branches in Chromium), both API loading/error-retry scenarios passed, and all 5 playback scenarios passed (seek, legacy, real segment-stall timeout/recovery, lifecycle, simulated ManagedMediaSource). The old stall fixture merely paused buffering, which the current player automatically resumes on seek; it was replaced with held segment traffic so the timeout path is actually exercised. Browser fixtures are not native-device playback certification.
