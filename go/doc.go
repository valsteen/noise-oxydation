// Package noiseox enhances 8 kHz G.711 mu-law telephony calls with the Rust noise-oxydation library.
//
// Create one [Call] per call with [New], feed it every 160-byte packet in order with [Call.ProcessPacket], forward
// each emitted packet, and end the call with [Call.Drain], which returns the packets withheld by the fixed
// two-packet (40 ms) delay. [Call.Reset] starts another call on the same handle and [Call.Close] releases it.
//
//	call, err := noiseox.New(noiseox.DefaultConfig())
//	if err != nil {
//		return err
//	}
//	defer call.Close()
//	var in, out [noiseox.PacketSize]byte
//	for receive(&in) {
//		emitted, err := call.ProcessPacket(&in, &out)
//		if err != nil {
//			return err
//		}
//		if emitted {
//			send(&out)
//		}
//	}
//	var tail [noiseox.DelayPackets][noiseox.PacketSize]byte
//	n, err := call.Drain(&tail)
//	for i := range n {
//		send(&tail[i])
//	}
//
// ProcessPacket, Drain, Reset and Phase allocate nothing on the Go or the Rust side: they pass pointers to the
// caller's fixed-size arrays, and the library writes into them.
//
// A Call is not safe for concurrent use: one goroutine drives it at a time. Independent calls share nothing and run in
// parallel goroutines, one Call each.
//
// Errors are typed. Configuration errors are [*ConfigError] values carrying every detail of the Rust error and
// matching [ErrInvalidConfig] with [errors.Is]; lifecycle errors are the sentinels [ErrDrained], [ErrClosed],
// [ErrNilArgument] and [ErrPanic].
//
// The package links the static library built by
//
//	cargo build --locked --release -p noise-oxydation-capi
//
// from the repository root. Inside the repository the linker finds it in target/release; a program that imports the
// module from elsewhere passes its directory with CGO_LDFLAGS=-L<dir>. docs/go-integration.md in the repository gives
// the complete macOS and Linux instructions.
package noiseox
