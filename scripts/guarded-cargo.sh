#!/bin/bash
set -euo pipefail
export CARGO_HOME=/Volumes/KIOXIA/Developments/cargo-home
export CARGO_TARGET_DIR=/Volumes/KIOXIA/Developments/cargo-target/rsomics-sketch
export TMPDIR=/Volumes/KIOXIA/Developments/tmp
export RUSTUP_HOME=/Volumes/KIOXIA/Developments/rustup-home
export XDG_CACHE_HOME=/Volumes/KIOXIA/Developments/tmp/cache
export GH_CACHE_DIR=/Volumes/KIOXIA/Developments/tmp/gh-cache
export PYTHONDONTWRITEBYTECODE=1
toolchain=/Volumes/KIOXIA/Developments/rustup-home/toolchains/1.91.0-aarch64-apple-darwin/bin
export CARGO="$toolchain/cargo"
export RUSTC="$toolchain/rustc"
export RUSTDOC="$toolchain/rustdoc"
export CARGO_BUILD_JOBS=2
export CARGO_INCREMENTAL=0
export PATH="$toolchain:$PATH"
for argument in "$@"; do
  case "$argument" in --target-dir|--target-dir=*|--config|--config=*) echo "Storage overrides are not supported by this guard: $argument" >&2; exit 1;; esac
done
/usr/sbin/diskutil info -plist /Volumes/KIOXIA | /usr/bin/python3 -B -c '
import plistlib, sys
volume = plistlib.loads(sys.stdin.buffer.read())
if volume.get("MountPoint") != "/Volumes/KIOXIA" or volume.get("Internal") is not False:
    sys.exit("KIOXIA is not mounted as an external volume")
print("verified external volume:", volume["DeviceNode"], file=sys.stderr)
'
external_device=$(stat -f %d /Volumes/KIOXIA)
for path in "$CARGO_HOME" "$CARGO_TARGET_DIR" "$TMPDIR" "$RUSTUP_HOME" "$toolchain"; do
  resolved=$(cd "$path" && pwd -P)
  case "$resolved" in /Volumes/KIOXIA/Developments/*) ;; *) echo "Forbidden build path: $resolved" >&2; exit 1;; esac
  test "$(stat -f %d "$resolved")" = "$external_device" || { echo "Project path is not on the verified external volume: $resolved" >&2; exit 1; }
  echo "verified build path: $resolved" >&2
done
for path in "$CARGO" "$RUSTC" "$RUSTDOC" "$toolchain/cargo-clippy"; do
  test -x "$path" && test "$(stat -f %d "$path")" = "$external_device" || { echo "Tool is not on the verified external volume: $path" >&2; exit 1; }
done
for path in "$XDG_CACHE_HOME" "$GH_CACHE_DIR"; do
  if [[ ! -e "$path" && ! -L "$path" ]]; then mkdir "$path"; fi
  resolved=$(cd "$path" && pwd -P)
  case "$resolved" in /Volumes/KIOXIA/Developments/*) ;; *) echo "Forbidden cache path: $resolved" >&2; exit 1;; esac
  test "$(stat -f %d "$resolved")" = "$external_device" || { echo "Project cache is not on the verified external volume: $resolved" >&2; exit 1; }
done
available_kib=$(df -Pk /Volumes/KIOXIA | awk 'NR == 2 {print $4}')
test "$available_kib" -ge 8388608 || { echo "External disk has less than 8 GiB independent headroom" >&2; exit 1; }
df -h /Volumes/KIOXIA >&2
target=$("$CARGO" metadata --locked --offline --no-deps --format-version 1 | /usr/bin/python3 -B -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
test "$target" = "$CARGO_TARGET_DIR"
echo "Cargo resolved target: $target" >&2
if [[ "${1:-}" == "--exec" ]]; then
  shift
  exec "$@"
fi
if [[ "${1:-}" == "clippy" ]]; then
  shift
  exec "$toolchain/cargo-clippy" clippy "$@"
fi
exec "$CARGO" "$@"
