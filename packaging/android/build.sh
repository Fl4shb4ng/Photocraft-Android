#!/usr/bin/env bash
# Build a native Rust/egui editor and package the small Android system-dialog bridge.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
sdk="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}"
if [[ -z "$sdk" ]]; then echo 'Set ANDROID_SDK_ROOT to your Android SDK directory.' >&2; exit 1; fi
ndk="${ANDROID_NDK_HOME:-$sdk/ndk/28.2.13676358}"
api="${PHOTOCRAFT_ANDROID_API:-35}"
build_tools="$sdk/build-tools/35.0.0"
abi="${PHOTOCRAFT_ANDROID_ABI:-arm64-v8a}"
case "$abi" in
  arm64-v8a) target=aarch64-linux-android; compiler=aarch64-linux-android26-clang; cargo_key=AARCH64_LINUX_ANDROID ;;
  x86_64) target=x86_64-linux-android; compiler=x86_64-linux-android26-clang; cargo_key=X86_64_LINUX_ANDROID ;;
  *) echo "Unsupported ABI: $abi (use arm64-v8a or x86_64)." >&2; exit 1 ;;
esac
case "$(uname -s)" in
  Linux) host=linux-x86_64 ;;
  Darwin) host=darwin-x86_64 ;;
  *) echo 'Run this script on Linux/macOS (or WSL2).' >&2; exit 1 ;;
esac
llvm="$ndk/toolchains/llvm/prebuilt/$host/bin"
for required in "$llvm/$compiler" "$build_tools/aapt2" "$build_tools/d8" "$build_tools/zipalign" "$build_tools/apksigner" "$sdk/platforms/android-$api/android.jar"; do
  [[ -f "$required" ]] || { echo "Missing SDK/NDK component: $required" >&2; exit 1; }
done
command -v cargo >/dev/null || { echo 'Install Rust 1.95+ and put cargo on PATH.' >&2; exit 1; }
export "CARGO_TARGET_${cargo_key}_LINKER=$llvm/$compiler"
export "CC_${target//-/_}=$llvm/$compiler"
export "AR_${target//-/_}=$llvm/llvm-ar"
export "CARGO_TARGET_${cargo_key}_RUSTFLAGS=-C link-arg=-Wl,-z,max-page-size=16384"
export CARGO_PROFILE_DEV_CODEGEN_UNITS="${CARGO_PROFILE_DEV_CODEGEN_UNITS:-1}"
export CARGO_PROFILE_DEV_BUILD_OVERRIDE_CODEGEN_UNITS="${CARGO_PROFILE_DEV_BUILD_OVERRIDE_CODEGEN_UNITS:-1}"
export CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_CODEGEN_UNITS="${CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_CODEGEN_UNITS:-1}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
profile="${PHOTOCRAFT_ANDROID_PROFILE:-release}"
case "$profile" in release) cargo_profile=(--release); output_profile=release ;; debug) cargo_profile=(); output_profile=debug ;; *) echo 'Profile must be release or debug.' >&2; exit 1 ;; esac
cargo build --locked -p photocraft-android --target "$target" "${cargo_profile[@]}"
work="$root/target/android-package-$abi"
rm -rf "$work"
mkdir -p "$work/classes" "$work/dex" "$work/apk/lib/$abi" "$work/apk/assets/legal" "$work/res/drawable-nodpi" "$root/dist/android"
cp "$root/LICENSE-MIT" "$root/LICENSE-APACHE" "$root/NOTICE" "$root/ATTRIBUTION.md" "$work/apk/assets/legal/"
rm -f "$work/apk/classes.dex"
javac --release 8 -classpath "$sdk/platforms/android-$api/android.jar" -d "$work/classes" \
  "$root/packaging/android/java/ai/storyteller/photocraft/PhotoCraftActivity.java"
jar cf "$work/activity.jar" -C "$work/classes" .
"$build_tools/d8" --min-api 26 --lib "$sdk/platforms/android-$api/android.jar" --output "$work/dex" "$work/activity.jar"
cp "$work/dex/classes.dex" "$work/apk/classes.dex"
cp "${CARGO_TARGET_DIR:-$root/target}/$target/$output_profile/libphotocraft_android.so" "$work/apk/lib/$abi/"
"$llvm/llvm-strip" --strip-debug "$work/apk/lib/$abi/libphotocraft_android.so"
cp "$root/assets/app-icon/photocraft-1024.png" "$work/res/drawable-nodpi/photocraft.png"
"$build_tools/aapt2" compile --dir "$work/res" -o "$work/compiled-res.zip"
"$build_tools/aapt2" link -o "$work/base.apk" --manifest "$root/packaging/android/AndroidManifest.xml" -I "$sdk/platforms/android-$api/android.jar" -R "$work/compiled-res.zip"
python3 - "$work/base.apk" "$work/unsigned.apk" "$work/apk" <<'PY'
import pathlib,sys,zipfile
base,out,tree=map(pathlib.Path,sys.argv[1:])
with zipfile.ZipFile(base) as source, zipfile.ZipFile(out,'w') as dest:
    for item in source.infolist(): dest.writestr(item,source.read(item.filename))
    for path in sorted(tree.rglob('*')):
        if path.is_file(): dest.write(path,path.relative_to(tree).as_posix(),compress_type=zipfile.ZIP_STORED)
PY
"$build_tools/zipalign" -f -P 16 4 "$work/unsigned.apk" "$work/aligned.apk"
# Local debug certificate for test installation only. Never commit the private key.
# Supply all PHOTOCRAFT_ANDROID_KEY* settings for a production signing key.
keystore="${PHOTOCRAFT_ANDROID_KEYSTORE:-$root/target/photocraft-android-debug.keystore}"
alias="${PHOTOCRAFT_ANDROID_KEY_ALIAS:-androiddebugkey}"
storepass="${PHOTOCRAFT_ANDROID_STORE_PASSWORD:-android}"
keypass="${PHOTOCRAFT_ANDROID_KEY_PASSWORD:-android}"
if [[ ! -f "$keystore" ]]; then
  [[ -z "${PHOTOCRAFT_ANDROID_KEYSTORE:-}" ]] || { echo 'Configured keystore does not exist.' >&2; exit 1; }
  keytool -genkeypair -keystore "$keystore" -storepass "$storepass" -keypass "$keypass" -alias "$alias" -keyalg RSA -keysize 2048 -validity 10000 -dname 'CN=PhotoCraft Android Development'
fi
export PHOTOCRAFT_SIGN_STORE_PASS="$storepass" PHOTOCRAFT_SIGN_KEY_PASS="$keypass"
output="$root/dist/android/PhotoCraft-$abi.apk"
"$build_tools/apksigner" sign --ks "$keystore" --ks-key-alias "$alias" --ks-pass env:PHOTOCRAFT_SIGN_STORE_PASS --key-pass env:PHOTOCRAFT_SIGN_KEY_PASS --out "$output" "$work/aligned.apk"
"$build_tools/apksigner" verify --verbose "$output"
"$build_tools/zipalign" -c -P 16 4 "$output"
python3 - "$output" <<'PY'
import hashlib,pathlib,sys
p=pathlib.Path(sys.argv[1]); p.with_suffix('.apk.sha256').write_text(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+p.name+'\n')
print(p)
PY
