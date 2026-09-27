#!/bin/sh
# Build both modes and replay one raw 8 kHz mono G.711 μ-law call through each.
set -eu
if [ "$#" -ne 2 ] || [ ! -f "$1" ]; then
  echo "usage: sh go/try.sh <raw-160-byte-packet-call.mulaw> <new-output-directory>" >&2
  exit 2
fi
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
input=$(CDPATH= cd -- "$(dirname -- "$1")" && pwd)/$(basename -- "$1")
bytes=$(wc -c < "$input" | tr -d ' ')
if [ "$bytes" -eq 0 ] || [ $((bytes % 160)) -ne 0 ]; then
  echo "input must be a nonempty sequence of complete 160-byte μ-law packets" >&2
  exit 2
fi
cargo build --release --locked --manifest-path "$root/Cargo.toml" -p noise-oxydation-ffi
archive="$root/target/release/libnoise_oxydation_ffi.a"
case "$(go env GOOS)" in
  darwin) export CGO_LDFLAGS="$archive" ;;
  linux) export CGO_LDFLAGS="$archive -ldl -lpthread -lm" ;;
  *) echo "this quick check supports macOS and Linux" >&2; exit 2 ;;
esac
export CGO_ENABLED=1
mkdir -- "$2"
output=$(CDPATH= cd -- "$2" && pwd)
cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    rm -f "$output/conservative.mulaw" "$output/experimental.mulaw"
    rmdir "$output" || true
  fi
}
trap cleanup EXIT
cd "$root/go"
go run ./cmd/replay -input "$input" -output "$output/conservative.mulaw" -mode conservative
go run ./cmd/replay -input "$input" -output "$output/experimental.mulaw" -mode experimental
printf 'Outputs: %s/conservative.mulaw and %s/experimental.mulaw\n' "$output" "$output"
