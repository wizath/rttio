#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/build-release.sh <target> [--features FEATURES] [--profile PROFILE]

Targets:
  macos       aarch64-apple-darwin
  linux       x86_64-unknown-linux-gnu
  linux-arm   aarch64-unknown-linux-gnu

Examples:
  scripts/build-release.sh macos
  scripts/build-release.sh linux --features serial,control
  scripts/build-release.sh linux-arm
USAGE
}

if [[ $# -lt 1 ]]; then
  usage >&2
  exit 2
fi

case "$1" in
  macos) target="aarch64-apple-darwin" ;;
  linux) target="x86_64-unknown-linux-gnu" ;;
  linux-arm) target="aarch64-unknown-linux-gnu" ;;
  -h|--help) usage; exit 0 ;;
  *)
    echo "unknown release target: $1" >&2
    usage >&2
    exit 2
    ;;
esac
shift

features="default"
profile="release"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --features)
      [[ $# -ge 2 ]] || { echo "--features requires a value" >&2; exit 2; }
      features="$2"
      shift 2
      ;;
    --profile)
      [[ $# -ge 2 ]] || { echo "--profile requires a value" >&2; exit 2; }
      profile="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$features" in
  default) ;;
  none) set -- --no-default-features ;;
  *) set -- --no-default-features --features "$features" ;;
esac

cargo build --locked --profile "$profile" --target "$target" "$@" --bin rttio

version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)"
bin="target/$target/$profile/rttio"
out_dir="dist"
name="rttio-$version-$target"

mkdir -p "$out_dir/$name"
cp "$bin" "$out_dir/$name/rttio"
cp README.md LICENSE CHANGELOG.md "$out_dir/$name/"

tar -C "$out_dir" -czf "$out_dir/$name.tar.gz" "$name"
rm -rf "$out_dir/$name"

echo "$out_dir/$name.tar.gz"
