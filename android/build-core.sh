#!/usr/bin/env bash
# Builds the Rust core for the phone ABIs the app ships and writes its Kotlin bindings, both
# where Gradle picks them up. Needs cargo-ndk and an NDK in ANDROID_NDK_HOME.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
app="$root/android/app/src/main"
profile="${PROFILE:-release}"

cd "$root"
cargo ndk -t arm64-v8a -t x86_64 -o "$app/jniLibs" build --locked --profile "$profile" -p mobile
cargo run --locked -p mobile --features bindgen --bin uniffi-bindgen -- generate \
  --library "target/aarch64-linux-android/$profile/libdigida_mobile.so" \
  --language kotlin --out-dir "$app/java" --no-format
