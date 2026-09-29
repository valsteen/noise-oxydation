// Package noiseoxydation streams μ-law packets through the Rust processor.
//
// Build the repository's release static archive before building a Go program
// that imports this package. One Processor is used in packet order by one
// goroutine; separate processors may be used concurrently.
package noiseoxydation

/*
#cgo CFLAGS: -I${SRCDIR}/../../crates/noise-oxydation-ffi/include
#cgo darwin LDFLAGS: ${SRCDIR}/../../target/release/libnoise_oxydation_ffi.a -lm
#cgo linux LDFLAGS: ${SRCDIR}/../../target/release/libnoise_oxydation_ffi.a -ldl -lpthread -lm
#include <stdlib.h>
#include "noise_oxydation.h"
*/
import "C"

import (
	"errors"
	"fmt"
	"unsafe"
)

const (
	// PacketBytes is the exact size of one input μ-law packet.
	PacketBytes = 160
	// MaxPacketOutputBytes is the largest output from one Push.
	MaxPacketOutputBytes = 256
	// MaxDrainOutputBytes is the largest output from Drain.
	MaxDrainOutputBytes = 255
)

var (
	// ErrClosed is returned by methods called after Close.
	ErrClosed = errors.New("noise-oxydation processor is closed")
	// ErrInvalidPacketLength marks a packet whose length is not PacketBytes.
	ErrInvalidPacketLength = errors.New("noise-oxydation packet must contain exactly 160 bytes")
	// ErrOutputTooSmall marks a retryable output-capacity rejection.
	ErrOutputTooSmall = errors.New("noise-oxydation output buffer is too small")
	// ErrStreamDrained marks a push after a successful Drain.
	ErrStreamDrained = errors.New("noise-oxydation stream is drained")
	// ErrStreamTooLong marks exhaustion of the processor's sample counter.
	ErrStreamTooLong = errors.New("noise-oxydation stream is too long")
	// ErrInternal marks a status that should not occur for a valid Go call.
	ErrInternal = errors.New("noise-oxydation native processor failed")
)

// CapacityError reports the exact output length needed by a retry.
type CapacityError struct {
	Required int
}

func (failure *CapacityError) Error() string {
	return fmt.Sprintf("%s: need %d bytes", ErrOutputTooSmall, failure.Required)
}

func (failure *CapacityError) Unwrap() error { return ErrOutputTooSmall }

// Processor owns one Rust processor and its opaque native handle.
//
// Do not call its methods concurrently. Close is idempotent, and methods after
// Close return ErrClosed without entering the freed native handle.
type Processor struct {
	state *processorState
}

type processorState struct {
	handle *C.NoiseOxydationProcessor
	closed bool
}

// New creates a fresh independent stream processor.
func New() *Processor {
	handle := C.noise_oxydation_processor_new()
	return &Processor{state: &processorState{handle: handle, closed: handle == nil}}
}

// Push accepts one exact μ-law packet and writes any ready output into output.
// It returns zero written bytes on error; a CapacityError reports the exact
// required size without consuming packet.
func (processor *Processor) Push(packet, output []byte) (int, error) {
	if processor == nil || processor.state == nil || processor.state.closed || processor.state.handle == nil {
		return 0, ErrClosed
	}
	if len(packet) != PacketBytes {
		return 0, ErrInvalidPacketLength
	}
	var packetPointer *C.uint8_t
	if len(packet) != 0 {
		packetPointer = (*C.uint8_t)(unsafe.Pointer(&packet[0]))
	}
	outputPointer := nativeBytes(output)
	var written C.size_t
	status := C.noise_oxydation_processor_push(processor.state.handle, packetPointer, C.size_t(len(packet)), outputPointer, C.size_t(len(output)), &written)
	if err := translateStatus(status, int(written)); err != nil {
		return 0, err
	}
	return int(written), nil
}

// Drain writes the remaining ordered output and closes the stream. Repeated
// drains succeed with zero output. It returns zero written bytes on error; a
// CapacityError reports the required size without closing the stream.
func (processor *Processor) Drain(output []byte) (int, error) {
	if processor == nil || processor.state == nil || processor.state.closed || processor.state.handle == nil {
		return 0, ErrClosed
	}
	var written C.size_t
	status := C.noise_oxydation_processor_drain(processor.state.handle, nativeBytes(output), C.size_t(len(output)), &written)
	if err := translateStatus(status, int(written)); err != nil {
		return 0, err
	}
	return int(written), nil
}

// Reset discards the pending tail and reopens this stream with fresh state.
func (processor *Processor) Reset() error {
	if processor == nil || processor.state == nil || processor.state.closed || processor.state.handle == nil {
		return ErrClosed
	}
	return translateStatus(C.noise_oxydation_processor_reset(processor.state.handle), 0)
}

// Close releases the native processor. Repeated calls are no-ops.
func (processor *Processor) Close() error {
	if processor == nil || processor.state == nil || processor.state.closed {
		return nil
	}
	if processor.state.handle != nil {
		C.noise_oxydation_processor_free(processor.state.handle)
		processor.state.handle = nil
	}
	processor.state.closed = true
	return nil
}

func nativeBytes(bytes []byte) *C.uint8_t {
	if len(bytes) == 0 {
		return nil
	}
	return (*C.uint8_t)(unsafe.Pointer(&bytes[0]))
}

func translateStatus(status C.NoiseOxydationStatus, required int) error {
	switch status {
	case C.NOISE_OXYDATION_OK:
		return nil
	case C.NOISE_OXYDATION_INVALID_ARGUMENT:
		return ErrInternal
	case C.NOISE_OXYDATION_INVALID_PACKET_LENGTH:
		return ErrInvalidPacketLength
	case C.NOISE_OXYDATION_OUTPUT_TOO_SMALL:
		return &CapacityError{Required: required}
	case C.NOISE_OXYDATION_STREAM_DRAINED:
		return ErrStreamDrained
	case C.NOISE_OXYDATION_STREAM_TOO_LONG:
		return ErrStreamTooLong
	case C.NOISE_OXYDATION_INTERNAL_ERROR:
		return ErrInternal
	default:
		return fmt.Errorf("%w: unknown native status %d", ErrInternal, int(status))
	}
}
