package noiseox

/*
// Link flags: the static library built by `cargo build --locked --release -p noise-oxydation-capi`, found in the
// repository's target/release (a program outside the repository adds CGO_LDFLAGS=-L<directory>), plus the system
// libraries that `rustc --print native-static-libs` lists for the target. On macOS that list is -lSystem -lc -lm; Go's
// linker already passes -lSystem, so it is left out to avoid a duplicate-library warning.
//
// noescape and nocallback tell the compiler that the packet functions neither retain Go pointers nor call back into
// Go, so the caller's arrays and the out-parameters stay on the stack and the hot path allocates nothing.
#cgo CFLAGS: -I${SRCDIR}
#cgo LDFLAGS: -L${SRCDIR}/../target/release -lnoise_oxydation_capi
#cgo darwin LDFLAGS: -lc -lm
#cgo linux LDFLAGS: -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc
#cgo noescape nox_call_process
#cgo nocallback nox_call_process
#cgo noescape nox_call_drain
#cgo nocallback nox_call_drain
#cgo noescape nox_call_phase
#cgo nocallback nox_call_phase
#cgo nocallback nox_call_reset
#cgo nocallback nox_call_free
#include "noise_oxydation.h"
*/
import "C"

import (
	"runtime"
	"unsafe"
)

const (
	// PacketSize is the size of one 20 ms G.711 mu-law packet in bytes (and samples).
	PacketSize = C.NOX_PACKET_BYTES
	// DelayPackets is the fixed algorithmic delay in packets: the first DelayPackets packets of a call emit nothing,
	// and Drain returns at most DelayPackets packets.
	DelayPackets = C.NOX_DELAY_PACKETS
)

// Phase is the lifecycle phase of a call.
type Phase uint32

// Call phases.
const (
	// PhaseCalibrating: frames pass through un-enhanced while the noise estimator learns the quiet intro.
	PhaseCalibrating Phase = C.NOX_PHASE_CALIBRATING
	// PhaseEnhancing: frames are enhanced.
	PhaseEnhancing Phase = C.NOX_PHASE_ENHANCING
	// PhaseDrained: the call has been drained; only Reset and Close are accepted.
	PhaseDrained Phase = C.NOX_PHASE_DRAINED
)

func (p Phase) String() string {
	switch p {
	case PhaseCalibrating:
		return "calibrating"
	case PhaseEnhancing:
		return "enhancing"
	case PhaseDrained:
		return "drained"
	default:
		return "unknown phase"
	}
}

// Call enhances one telephony call. It owns one Rust enhancer; its methods must not be called concurrently.
//
// Close releases the enhancer. A Call that becomes unreachable without Close is released by a runtime cleanup, but
// that happens only after a garbage collection, so close calls explicitly.
type Call struct {
	handle  *C.nox_call
	cleanup runtime.Cleanup
}

// New validates config and creates the enhancer of one call. It is the only function of the package that allocates
// in Rust: the enhancer's state is sized here and reused by every later method.
//
// An invalid configuration returns a *ConfigError, which matches ErrInvalidConfig.
func New(config Config) (*Call, error) {
	native, err := config.toC()
	if err != nil {
		return nil, err
	}
	var handle *C.nox_call
	var detail C.nox_error
	status := C.nox_call_new(&native, &handle, &detail)
	if status != C.NOX_STATUS_OK {
		return nil, errorFromDetail(&detail)
	}
	call := &Call{handle: handle}
	call.cleanup = runtime.AddCleanup(call, freeHandle, handle)
	return call, nil
}

// freeHandle is the cleanup of a Call that was never closed. It must not reference the Call.
func freeHandle(handle *C.nox_call) {
	C.nox_call_free(handle)
}

// ProcessPacket consumes one input packet. The first DelayPackets packets of a call return emitted == false and
// leave out untouched; every later packet writes the enhanced packet received DelayPackets packets earlier to out and
// returns emitted == true. in and out may be the same array.
//
// It returns ErrDrained after Drain until Reset, ErrClosed after Close, and ErrNilArgument for a nil array. It
// allocates nothing.
func (c *Call) ProcessPacket(in, out *[PacketSize]byte) (emitted bool, err error) {
	if c == nil || c.handle == nil {
		return false, ErrClosed
	}
	var outcome C.uint32_t
	status := C.nox_call_process(c.handle, (*C.uint8_t)(unsafe.Pointer(in)), (*C.uint8_t)(unsafe.Pointer(out)), &outcome)
	runtime.KeepAlive(c)
	if status != C.NOX_STATUS_OK {
		return false, statusError(status)
	}
	return outcome == C.NOX_OUTCOME_EMITTED, nil
}

// Drain ends the call: it writes the withheld packets, in order, to the front of out and returns how many it wrote,
// min(packets received, DelayPackets). Together with the emitted packets the call's output then has exactly the
// length of its input.
//
// It returns ErrDrained if the call was already drained, ErrClosed after Close, and ErrNilArgument for a nil array. It
// allocates nothing.
func (c *Call) Drain(out *[DelayPackets][PacketSize]byte) (int, error) {
	if c == nil || c.handle == nil {
		return 0, ErrClosed
	}
	var written C.size_t
	status := C.nox_call_drain(c.handle, (*C.uint8_t)(unsafe.Pointer(out)), &written)
	runtime.KeepAlive(c)
	if status != C.NOX_STATUS_OK {
		return 0, statusError(status)
	}
	return int(written), nil
}

// Reset starts a new call on the same enhancer: it discards buffered input and withheld output and restores the
// initial phase, reusing all storage. It returns ErrClosed after Close and allocates nothing.
func (c *Call) Reset() error {
	if c == nil || c.handle == nil {
		return ErrClosed
	}
	status := C.nox_call_reset(c.handle)
	runtime.KeepAlive(c)
	return statusError(status)
}

// Phase returns the call's lifecycle phase, or ErrClosed after Close.
func (c *Call) Phase() (Phase, error) {
	if c == nil || c.handle == nil {
		return 0, ErrClosed
	}
	var phase C.uint32_t
	status := C.nox_call_phase(c.handle, &phase)
	runtime.KeepAlive(c)
	if status != C.NOX_STATUS_OK {
		return 0, statusError(status)
	}
	return Phase(phase), nil
}

// Close releases the enhancer. It is idempotent and always returns nil; every other method returns ErrClosed
// afterwards, without calling into Rust.
func (c *Call) Close() error {
	if c == nil || c.handle == nil {
		return nil
	}
	c.cleanup.Stop()
	C.nox_call_free(c.handle)
	c.handle = nil
	return nil
}
