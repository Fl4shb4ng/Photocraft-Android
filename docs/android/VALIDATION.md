# Android delivery validation — 2026-10-07

Baseline: `727daf5d6745ba084d1feef02014779354ab4893` (PhotoCraft 0.3.0).
These are local checks, not a certification of all desktop workflows on Android.
The APK SHA and build checks below describe the previously delivered APK; the
immersive-mode fix was added to source after that build and still needs a rebuild.

## Completed

- Native **ARM64** APK compiled, Java bridge compiled, DEX and manifest packaged.
  Rust 1.95.0, JDK 17, Android platform/build tools 35 and NDK 28.2.13676358.
  The delivered APK uses the optimized **dev/debug** profile, stripped debug
  information and a development signing certificate. A release APK was not built
  in this session; the script and CI default to the release profile.
- APK signature verified (v2/v3), package and launchable activity inspected,
  min SDK 26 / target SDK 35 confirmed. ZIP alignment checked with `zipalign -P 16`;
  every native ELF LOAD segment has `0x4000` (16 KiB) alignment.
- An x86_64 development APK also compiled for emulator use. It was built before
  the final toolbar label/icon correction; the ARM64 delivery includes that fix.
- Shared UI library: **755 passed, 0 failed, 3 ignored**. This includes six mobile
  gesture/save/layout tests and the asynchronous save-before-close regression.
- Text library: **78 passed, 0 failed, 3 ignored**.
- `cargo xtask layers`: **29 crates, no violations**.
- `cargo xtask wasm`: all platform-neutral workspace checks passed, including
  the UI and codecs with the HEIF feature. Existing unused-item warnings remain.
- Formatting and shell syntax checks passed.
- Actual egui/wgpu offscreen renders inspected at **390 × 844** and **844 × 390**,
  with portrait bottom sheets, landscape side sheets and a scrollable New Document
  dialog whose action buttons remain visible. Rendering used Mesa llvmpipe Vulkan
  on the host. These screenshots are not Android-device screenshots.
- A user-provided screenshot confirms the delivered APK launched to PhotoCraft's
  welcome screen on a **Motorola Edge 40 Pro, Android 16**. The screenshot also
  reproduced the visible status bar. It verifies launch only, not image editing,
  gestures, keyboard, or file-picker behavior.

APK SHA-256:

```text
fcb88f87e7ecb40ec7e5d0ad0a199b57c89860f26b3d7f06e98cb2d84cecc3bb
```

## Checks with limits

- Strict Clippy with `-D warnings` encounters pre-existing `collapsible_match`
  findings in `plugins/src/manifest.rs`, `ui-egui/src/analysis_ui.rs` and an existing
  gradient arm in `ui-egui/src/canvas.rs`. The Android seam and shared UI were also
  checked separately from dependency lints. Shared UI Clippy passed with
  `--all-targets --no-deps -- -D warnings -A clippy::collapsible_match`, suppressing
  only that pre-existing lint category. Android Clippy passed with
  `--target aarch64-linux-android --all-targets --no-deps -- -D warnings`.
- `cargo xtask test-corpus --changed` was attempted. The first fetch encountered
  archive ownership unsupported by this container. Retrying with
  `TAR_OPTIONS=--no-same-owner` successfully fetched PngSuite and the pinned PSD
  corpora, but a pinned HEIF archive download was truncated (`Unexpected EOF`).
  The corpus suite therefore **did not run**. No corpus floor was changed.
- The Android emulator was attempted without KVM acceleration but did not reach
  a usable boot during the test window. The physical launch was later confirmed
  by the user's screenshot. This workspace no longer has the Android SDK/NDK,
  ADB, emulator, or Rust toolchain, so the immersive-mode source fix below has
  **not yet been compiled into an updated APK**.

## Device testing still required

Rebuild and verify immersive mode; GPU fallback/driver behaviour; software keyboard and text editing;
system insets across devices; rotation, suspend/resume and process death;
SAF import/export and provider failures; recovery after process termination;
performance and memory consumption on large files. The asynchronous picker
cancel/error and document-targeting logic has synthetic host tests, but still
needs system-provider testing on Android.
