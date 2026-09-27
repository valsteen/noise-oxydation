#!/bin/sh
# Build and exercise the package from a separate Go module without a download.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
archive="$root/target/release/libnoise_oxydation_ffi.a"
test -f "$archive" || { echo "build the Rust staticlib first: cargo build --release --locked -p noise-oxydation-ffi" >&2; exit 1; }
case "$(go env GOOS)" in
  darwin) export CGO_LDFLAGS="$archive" ;;
  linux) export CGO_LDFLAGS="$archive -ldl -lpthread -lm" ;;
  *) echo "this integration check supports macOS and Linux" >&2; exit 1 ;;
esac
export CGO_ENABLED=1
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
export GOCACHE="$tmp/gocache"
cd "$tmp"
go mod init example.com/noise-oxydation-consumer
go mod edit -require=github.com/valsteen/noise-oxydation/go@v0.0.0
go mod edit -replace=github.com/valsteen/noise-oxydation/go="$root/go"
cat > main.go <<'EOF'
package main
import (
  "fmt"
  noise "github.com/valsteen/noise-oxydation/go"
)
func run(mode noise.ProcessingMode) {
  config := noise.DefaultConfig()
  config.Mode = mode
  call, err := noise.New(config)
  if err != nil { panic(err) }
  defer call.Close()
  var packet [noise.PacketBytes]byte
  var output [noise.OutputBytes]byte
  valid := 0
  for i := 0; i < 400; i++ {
    for j := range packet { if i < 250 { packet[j] = 0xff } else { packet[j] = byte((i+j)*31) } }
    batch, err := call.Process(&packet, &output)
    if err != nil { panic(err) }
    valid += batch.Count * noise.PacketBytes
    if batch.Count > 0 { valid -= noise.PacketBytes - batch.FinalValid }
  }
  batch, err := call.Finish(&output)
  if err != nil { panic(err) }
  valid += batch.Count * noise.PacketBytes
  if batch.Count > 0 { valid -= noise.PacketBytes - batch.FinalValid }
  if valid != 64000 { panic(fmt.Sprintf("valid output %d", valid)) }
  fmt.Printf("external consumer mode %d: %d valid bytes\n", mode, valid)
}
func runPCM(mode noise.ProcessingMode) {
  config := noise.DefaultConfig()
  config.Mode = mode
  processor, err := noise.NewPCMProcessor(config)
  if err != nil { panic(err) }
  defer processor.Close()
  packet := make([]float32, noise.PacketBytes)
  valid := 0
  for i := 0; i < 400; i++ {
    for j := range packet { if i < 250 { packet[j] = 0 } else { packet[j] = float32((i+j)%256-128) / 32768 } }
    output, err := processor.Process(packet)
    if err != nil { panic(err) }
    valid += len(output)
  }
  tail, err := processor.Finish()
  if err != nil { panic(err) }
  valid += len(tail)
  if valid != 64000 { panic(fmt.Sprintf("valid PCM output %d", valid)) }
  fmt.Printf("external PCM consumer mode %d: %d valid samples\n", mode, valid)
}
func main() {
  run(noise.Conservative); run(noise.ExperimentalLowDelay)
  runPCM(noise.Conservative); runPCM(noise.ExperimentalLowDelay)
}
EOF
go test ./...
go run .
python3 -c 'from pathlib import Path; Path("input.mulaw").write_bytes(bytes([255])*40000 + bytes((i*31)%256 for i in range(24000)))'
go run github.com/valsteen/noise-oxydation/go/cmd/replay -input input.mulaw -output output.mulaw
test "$(wc -c < output.mulaw | tr -d ' ')" = 64000
go run github.com/valsteen/noise-oxydation/go/cmd/replay -input input.mulaw -output output-experimental.mulaw -mode experimental
test "$(wc -c < output-experimental.mulaw | tr -d ' ')" = 64000
