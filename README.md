# Haiku

NSH 0.4.0 mixed application for macOS. `App.tsx` imports the Rust API from
`haiku/native`; `native/app` owns that API, and `native/Cargo.toml` owns the
workspace. The domain crates under `native/crates` own playback, local library,
storage, audio and the isolated plugin runtime.

Run `nsh check`, `nsh build`, `nsh test`, and `nsh run` from this directory.
Package with `nsh package --target macos`. Run Rust workspace checks with
`cargo test --manifest-path native/Cargo.toml --workspace`.

On this machine (macOS 27), disable stripping for build-time dependencies before
running Cargo or NSH commands:

```sh
export CARGO_PROFILE_DEV_BUILD_OVERRIDE_STRIP=false
export CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP=false
nsh check
nsh build
mkdir -p /tmp/haiku-m1-03-fixtures
HAIKU_DATA_DIR="$(mktemp -d /tmp/haiku-nsh-test.XXXXXX)" HAIKU_NO_KEYCHAIN=1 nsh test
nsh run App.tsx --target macos
```

These environment variables also reach NSH's generated host workspace under
`.nsh/rust`. With the default `strip=debuginfo`, this machine's dyld rejects a
proc-macro dylib with `mis-aligned LINKEDIT string pool`, surfaced by rustc as
`E0463: can't find crate for rquickjs_macro`. Clearing caches does not fix that
failure. Keep one toolchain throughout validation; no generated host or dependency
source changes are needed.

The local-library Headless fixture uses the explicitly granted
`/tmp/haiku-m1-03-fixtures` directory (create it before testing); the light/dark
previews use `.artifacts/m1-03`. These filesystem grants in `nsh.json` keep
generated audio separate from normal music and application data. Headless
modules have `asset:/` URLs, so their fixture root cannot use `import.meta.url`.
The test removes its generated tracks and scan roots after completion.

The formal entry loads the app's theme and confirmed shell. Local library rows
are virtualized in Rust; TSX holds page and editor state. Tests and previews must
use an isolated `HAIKU_DATA_DIR` to avoid changing the normal application data.

Verification evidence and known gaps are recorded in `specs/STATUS.md`.
Generated hosts and output live in `.nsh/`; local evidence lives in `.artifacts/`.
