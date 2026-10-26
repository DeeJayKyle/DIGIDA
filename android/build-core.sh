#!/usr/bin/env bash
# Builds the Rust core for the phone ABIs the app ships and writes its Kotlin bindings, both
# where Gradle picks them up. Needs cargo-ndk and an NDK in ANDROID_NDK_HOME.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
app="$root/android/app/src/main"
profile="${PROFILE:-release}"
# the oldest Android the app runs on, which the NDK also wants spelled into every clang target
api=26
export BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android="--target=aarch64-linux-android$api"
export BINDGEN_EXTRA_CLANG_ARGS_x86_64_linux_android="--target=x86_64-linux-android$api"

cd "$root"
cargo ndk --platform "$api" -t arm64-v8a -t x86_64 -o "$app/jniLibs" build --locked --profile "$profile" -p mobile
cargo run --locked -p mobile --features bindgen --bin uniffi-bindgen -- generate \
  --library "target/aarch64-linux-android/$profile/libdigida_mobile.so" \
  --language kotlin --out-dir "$app/java" --no-format
