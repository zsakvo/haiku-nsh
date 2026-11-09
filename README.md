# Haiku

NSH 0.4.0 mixed application for macOS. `App.tsx` imports the Rust API from
`haiku/native`; `native/app` owns that API, and `native/Cargo.toml` owns the
workspace. The domain crates under `native/crates` are placeholders except for
the isolated CJRT echo probe in `haiku-plugins`.

Run `nsh check`, `nsh build`, `nsh test`, and `nsh run` from this directory.
Package with `nsh package --target macos`. Run Rust workspace checks with
`cargo test --manifest-path native/Cargo.toml --workspace`.

The M0 screen only verifies the native bridge. It uses NSH's default semantic
theme tokens, the standard spacing/type scales, and macOS window chrome.
The complete product design system belongs to M1.

S1 passed before CJRT was added. The current S2 integration is intentionally
left for review: the generated NSH host cannot compile CJRT's older RustCrypto
API against NSH's newer dependencies, and a separate debug/release link probe
demonstrates duplicate QuickJS symbols resolving to one implementation.
The echo button and 1000-call test are implemented but have not run.

`native/Cargo.lock` retains CJRT revision `400cb024` and the reference app's
compatible prerelease crypto dependency versions. NSH creates a separate Cargo
workspace under `.nsh/rust`; seeding its lock from `native/Cargo.lock` was tried
and cannot satisfy both crypto dependency sets. Do not treat the passing native
workspace check as proof that the NSH application builds.

Verification evidence and known gaps are recorded in `specs/STATUS.md`.
Generated hosts and output live in `.nsh/`; local evidence lives in `.artifacts/`.
