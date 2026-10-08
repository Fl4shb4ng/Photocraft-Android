# PhotoCraft Android — experimental native port

This port keeps the existing Rust document model, commands, codecs and egui editor.
It adds a separate Android NativeActivity entry point, a touch shell and a small Java
bridge for Android's Storage Access Framework. There is no webview or Java editor UI.

Upstream baseline: `727daf5d6745ba084d1feef02014779354ab4893`.

## Build an APK

Use Linux/macOS (or WSL2), Rust 1.95+, JDK 17, Python 3 and an Android SDK.

```sh
rustup target add aarch64-linux-android
sdkmanager 'platforms;android-35' 'build-tools;35.0.0' 'ndk;28.2.13676358'
export ANDROID_SDK_ROOT=/path/to/android-sdk
packaging/android/build.sh
```

Output: `dist/android/PhotoCraft-arm64-v8a.apk` and its SHA-256 file.
The script defaults to a release build, signs with a local development certificate,
checks its signature and verifies 16 KiB ZIP/native-library alignment.
The NDK compiler/linker and archiver are configured by the script, not guessed from
host tools. The minimum OS is Android 8 (API 26); target SDK is API 35.

For a faster development build:

```sh
PHOTOCRAFT_ANDROID_PROFILE=debug packaging/android/build.sh
```

The GitHub Actions workflow `.github/workflows/android.yml` produces an ARM64 APK.
For an x86_64 emulator, add that Rust target and set `PHOTOCRAFT_ANDROID_ABI=x86_64`.

A release signing key can be supplied using `PHOTOCRAFT_ANDROID_KEYSTORE`,
`PHOTOCRAFT_ANDROID_KEY_ALIAS`, `PHOTOCRAFT_ANDROID_STORE_PASSWORD` and
`PHOTOCRAFT_ANDROID_KEY_PASSWORD`. Private keys stay outside source control.
The default debug key is generated under the ignored `target/` directory.

## Use it

1. Open an image/PSD with **Open** or create a 1080 × 1080 canvas.
2. Select a tool in the bottom strip. **Tools** contains every engine tool.
3. One finger uses the selected tool; two fingers pan and pinch to zoom.
   The navigation gesture owns the remaining finger until all contacts lift,
   preventing an accidental brush stroke on release.
4. Open **Layers** for selection, visibility, opacity and pixel-mask targeting.
   **Options**, **Color**, **History** and **Adjustments** expose further controls.
5. The menu button opens searchable existing commands. Long desktop dialogs have
   a bounded scrollable body so their confirmation buttons remain on screen.
6. Use the **Save** drawer to choose PSD, PCraft, PNG or JPEG, then select a
   destination in Android's system file picker. PSD/PCraft preserve layers.

The source build hides Android's status and navigation bars in immersive mode.
An edge swipe reveals them temporarily; returning from the system file picker
reapplies immersive mode. The screenshot-confirmed APK predates this source fix
and must be rebuilt before it takes effect on the phone.

The system picker supplies scoped content URIs: no all-files/storage or network
permission is requested. Incoming files are copied into private temporary storage
on a worker thread, capped at 256 MiB and removed after import. File size alone does
not bound the decoded document's memory use.

Saving is asynchronous: picker cancellation and provider errors leave the document
dirty. A result belongs to the document that opened the picker, even if another tab
is active when it arrives. Actual writes use ContentResolver. The source photo is
not overwritten merely by opening it.

Preferences and recovery checkpoints use Android's private app directory. While
foregrounded, dirty documents are checkpointed every 10 seconds if autosave is
turned on. A killed process can still lose changes since its last completed
checkpoint. Uninstalling/clearing app data deletes private recovery data.

## Development verification

```sh
cargo fmt --all --check
cargo test -p photocraft-ui-egui mobile --lib
cargo clippy -p photocraft-ui-egui --all-targets -- -D warnings
cargo clippy -p photocraft-android --target aarch64-linux-android -- -D warnings
cargo xtask layers
cargo xtask wasm
cargo run -p photocraft-ui-egui --example snapshot -- \
  --mobile --size 390x844 --scale 1 --out android-ui.png \
  --script '[["engine.execute", {"id":"file.new", "params":{"width":1080,"height":1080}}]]'
```

See `VALIDATION.md` for the checks actually completed for this delivery.

## Current limits

This is a first port, not a claim that every desktop workflow has been certified on
Android. It uses NativeActivity; Android screen-reader support is not available in
this backend. Software-keyboard behaviour, GPU drivers, lifecycle interruptions,
performance on large PSDs and provider-specific write failures need device testing.

Use the mobile save drawer for exports. Desktop workflows that require a synchronous
file picker (placing linked smart objects, importing presets/scripts and some Export
As paths) are not wired to the asynchronous Android picker yet. OS image clipboard,
filesystem-based Open Recent and desktop automation integrations are also not wired.
The desktop and web frontends keep their existing platform services.

SAF providers do not guarantee transactional replacement of an existing destination;
a write that fails midway may leave a partial file at that destination. The document
remains open and dirty so it can be saved to another destination.

No upstream branch, remote repository or release was modified.
