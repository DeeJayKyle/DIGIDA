#!/usr/bin/env bash
# Builds the Rust core for the phone ABIs the app ships and writes its Kotlin bindings, both
# where Gradle picks them up. Needs cargo-ndk and an NDK in ANDROID_NDK_HOME.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
app="$root/android/app/src/main"
profile="${PROFILE:-release}"
# the oldest Android the app runs on, which the NDK also wants spelled into every clang target
api=26

# Bindgen's clang has to be told the API level too, or the NDK headers refuse an unversioned
# target. cargo-ndk sets these variables itself without one, so a forced cargo env wins over it.
sysroot="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot"
config="$(mktemp --suffix=.toml)"
trap 'rm -f "$config"' EXIT
{
  echo "[env]"
  for triple in aarch64-linux-android x86_64-linux-android; do
    args="--sysroot=$sysroot -I$sysroot/usr/include/$triple --target=$triple$api"
    echo "BINDGEN_EXTRA_CLANG_ARGS_${triple//-/_} = { value = \"$args\", force = true }"
  done
} > "$config"

cd "$root"
cargo ndk --platform "$api" -t arm64-v8a -t x86_64 -o "$app/jniLibs" build --locked --config "$config" --profile "$profile" -p mobile
cargo run --locked -p mobile --features bindgen --bin uniffi-bindgen -- generate \
  --library "target/aarch64-linux-android/$profile/libdigida_mobile.so" \
  --language kotlin --out-dir "$app/java" --no-format
