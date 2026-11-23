# Android build

RCommander shares its whole engine (`rc-core`, `rc-viewer`, `rc-archive`,
`rc-i18n`, `rc-help`, `rc-agent`) with the desktop build; only the window and
input shell come from `eframe`/`winit`, which supports Android through
`NativeActivity`.

Because the Android toolchain is not installed in the current development
environment, the APK build is configured but not exercised here. The steps below
are the intended, supported route.

## Prerequisites

- Rust targets: `rustup target add aarch64-linux-android`
- Android SDK (`ANDROID_HOME`) and NDK (`ANDROID_NDK_HOME`), NDK r26 or newer
- `cargo-apk`: `cargo install cargo-apk`

## Build

```bash
export ANDROID_HOME=$HOME/Android/Sdk
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/<version>
cargo apk build --release --target aarch64-linux-android -p rcommander
```

`Cargo.toml` of the application crate already declares the metadata
`cargo-apk` reads (`package.metadata.android`).

## Device behaviour notes

- The panel split becomes a single-panel-with-tabs layout on narrow screens;
  `Tab` on a hardware keyboard still switches panels.
- Touch: tap selects, long-press opens the context menu, double-tap enters a
  directory.
- The storage roots enumerated by `rc_core::drive::volumes()` include
  `/storage/emulated/0`, `/sdcard` and `/storage`.
- The MCP server needs the `http` transport on Android, because a mobile app
  cannot be spawned as a stdio child by another process.

## Configuration on Android

Settings and language packs live in the app's private directory; the bundled
`locales/` and `Docs/` directories are copied into the APK assets and read from
there. Users can still add language packs by placing them into the app's external
files directory (`.../Android/data/<package>/files/`).
