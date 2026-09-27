package noiseoxydation

/*
#cgo CFLAGS: -I${SRCDIR}
#include "abi.h"
*/
import "C"

import (
	"fmt"
	"sync"
	"time"
	"unsafe"
)

const PacketBytes = 160
const OutputBytes = 320

type Estimator uint32

const (
	SppMmse Estimator = iota
	Mcra
	Minimum
)

type Config struct {
	LearningDuration time.Duration
	Estimator        Estimator
}

func DefaultConfig() Config { return Config{LearningDuration: 5 * time.Second} }

type Status uint32

const (
	StatusNull Status = iota + 1
	StatusInputLength
	StatusOutputCapacity
	StatusEstimator
	StatusDuration
	StatusDSPInit
	StatusFinished
	StatusPanic
	StatusClosed
)

// Error preserves the C status and the minimum DSP learning interval in samples.
type Error struct {
	Status  Status
	Samples uint64
	Minimum uint64
}

func (e *Error) Error() string {
	return fmt.Sprintf("noise oxydation: status %d (learning samples %d, minimum %d)", e.Status, e.Samples, e.Minimum)
}

func status(r C.NoResult) error {
	if r.status == C.NO_OK {
		return nil
	}
	return &Error{Status: Status(r.status), Samples: uint64(r.samples), Minimum: uint64(r.minimum)}
}

// Call owns one Rust pipeline. Its methods serialize access, including Close.
// Separate calls may be processed concurrently. Keep processing off the Go audio callback.
type Call struct {
	mu     sync.Mutex
	handle *C.NoCall
}

func New(config Config) (*Call, error) {
	if config.LearningDuration <= 0 {
		return nil, &Error{Status: StatusDuration}
	}
	var handle *C.NoCall
	r := C.no_create(C.uint64_t(config.LearningDuration), C.uint32_t(config.Estimator), &handle)
	if err := status(r); err != nil {
		return nil, err
	}
	return &Call{handle: handle}, nil
}

// Batch describes zero to two ordered packets in a caller-owned 320-byte buffer.
// The final packet has FinalValid bytes; earlier packets have PacketBytes bytes.
type Batch struct {
	Count      int
	FinalValid int
}

func batch(r C.NoResult) Batch { return Batch{Count: int(r.count), FinalValid: int(r.final_valid)} }

// Process consumes exactly one packet. The input and output arrays must not overlap.
func (c *Call) Process(input *[PacketBytes]byte, output *[OutputBytes]byte) (Batch, error) {
	if input == nil || output == nil {
		return Batch{}, &Error{Status: StatusNull}
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.handle == nil {
		return Batch{}, &Error{Status: StatusClosed}
	}
	r := C.no_process(c.handle, (*C.uint8_t)(unsafe.Pointer(&input[0])), C.size_t(PacketBytes), (*C.uint8_t)(unsafe.Pointer(&output[0])), C.size_t(OutputBytes))
	return batch(r), status(r)
}

// Finish drains final audio once. Process and a second Finish then return StatusFinished.
func (c *Call) Finish(output *[OutputBytes]byte) (Batch, error) {
	if output == nil {
		return Batch{}, &Error{Status: StatusNull}
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.handle == nil {
		return Batch{}, &Error{Status: StatusClosed}
	}
	r := C.no_finish(c.handle, (*C.uint8_t)(unsafe.Pointer(&output[0])), C.size_t(OutputBytes))
	return batch(r), status(r)
}

// Reset clears all audio and finished state while retaining configuration.
func (c *Call) Reset() error {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.handle == nil {
		return &Error{Status: StatusClosed}
	}
	return status(C.no_reset(c.handle))
}

// Close releases the native handle. Repeated Close is safe.
func (c *Call) Close() error {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.handle == nil {
		return nil
	}
	err := status(C.no_destroy(c.handle))
	c.handle = nil
	return err
}
