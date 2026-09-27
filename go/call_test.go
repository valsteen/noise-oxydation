package noiseoxydation

import (
	"errors"
	"sync"
	"testing"
	"time"
)

func wantStatus(t *testing.T, err error, expected Status) {
	t.Helper()
	var typed *Error
	if !errors.As(err, &typed) || typed.Status != expected {
		t.Fatalf("got %v, want status %d", err, expected)
	}
}

func collect(t *testing.T, call *Call, packets int) []byte {
	t.Helper()
	var input [PacketBytes]byte
	var output [OutputBytes]byte
	result := make([]byte, 0, packets*PacketBytes)
	add := func(batch Batch) {
		for i := 0; i < batch.Count; i++ {
			valid := PacketBytes
			if i+1 == batch.Count {
				valid = batch.FinalValid
			}
			result = append(result, output[i*PacketBytes:i*PacketBytes+valid]...)
		}
	}
	for i := 0; i < packets; i++ {
		for j := range input {
			input[j] = byte(i + j)
		}
		batch, err := call.Process(&input, &output)
		if err != nil {
			t.Fatal(err)
		}
		if packets == 3 && batch.Count != map[bool]int{true: 1, false: 0}[i == 2] {
			t.Fatalf("packet %d emitted %d", i, batch.Count)
		}
		if packets == 8 && i == 3 && batch.Count != 2 {
			t.Fatalf("packet %d emitted %d, want 2", i, batch.Count)
		}
		add(batch)
	}
	batch, err := call.Finish(&output)
	if err != nil {
		t.Fatal(err)
	}
	add(batch)
	if len(result) != packets*PacketBytes {
		t.Fatalf("got %d bytes, want %d", len(result), packets*PacketBytes)
	}
	return result
}

func TestLifecycleAndIndependentCalls(t *testing.T) {
	for _, estimator := range []Estimator{SppMmse, Mcra, Minimum} {
		call, err := New(Config{LearningDuration: time.Second, Estimator: estimator})
		if err != nil {
			t.Fatal(err)
		}
		for _, packets := range []int{0, 1, 2, 3, 8} {
			first := collect(t, call, packets)
			var input [PacketBytes]byte
			var output [OutputBytes]byte
			_, err = call.Process(&input, &output)
			wantStatus(t, err, StatusFinished)
			_, err = call.Finish(&output)
			wantStatus(t, err, StatusFinished)
			if err := call.Reset(); err != nil {
				t.Fatal(err)
			}
			second := collect(t, call, packets)
			if string(first) != string(second) {
				t.Fatal("reset changed output")
			}
			if err := call.Reset(); err != nil {
				t.Fatal(err)
			}
		}
		other, err := New(Config{LearningDuration: time.Second, Estimator: estimator})
		if err != nil {
			t.Fatal(err)
		}
		if string(collect(t, call, 8)) != string(collect(t, other, 8)) {
			t.Fatal("independent handles differ")
		}
		_ = other.Close()
		_ = call.Close()
		if err := call.Close(); err != nil {
			t.Fatal(err)
		}
		wantStatus(t, call.Reset(), StatusClosed)
	}
}

func TestConstructorErrorsAndConcurrentClose(t *testing.T) {
	_, err := New(Config{})
	wantStatus(t, err, StatusDuration)
	_, err = New(Config{LearningDuration: 20 * time.Millisecond})
	var typed *Error
	if !errors.As(err, &typed) || typed.Status != StatusDSPInit || typed.Samples != 160 || typed.Minimum != 256 {
		t.Fatalf("short duration: %v", err)
	}
	_, err = New(Config{LearningDuration: time.Second, Estimator: 99})
	wantStatus(t, err, StatusEstimator)
	call, err := New(DefaultConfig())
	if err != nil {
		t.Fatal(err)
	}
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		var input [PacketBytes]byte
		var output [OutputBytes]byte
		for i := 0; i < 100; i++ {
			_, err := call.Process(&input, &output)
			if err != nil {
				return
			}
		}
	}()
	if err := call.Close(); err != nil {
		t.Fatal(err)
	}
	wg.Wait()
	var input [PacketBytes]byte
	var output [OutputBytes]byte
	_, err = call.Process(&input, &output)
	wantStatus(t, err, StatusClosed)
}

func TestEstimatorsShareIntroAndDivergeAfterLearning(t *testing.T) {
	var results [3][]byte
	for i, estimator := range []Estimator{SppMmse, Mcra, Minimum} {
		call, err := New(Config{LearningDuration: time.Second, Estimator: estimator})
		if err != nil {
			t.Fatal(err)
		}
		results[i] = collect(t, call, 110)
		_ = call.Close()
	}
	for i := 1; i < len(results); i++ {
		if string(results[0][:10*PacketBytes]) != string(results[i][:10*PacketBytes]) {
			t.Fatal("intro differs")
		}
		if string(results[0]) == string(results[i]) {
			t.Fatal("estimator selection did not affect audio")
		}
	}
}

func TestExperimentalModeAndInvalidSelector(t *testing.T) {
	_, err := New(Config{LearningDuration: time.Second, Mode: ProcessingMode(99)})
	wantStatus(t, err, StatusMode)
	for _, mode := range []ProcessingMode{Conservative, ExperimentalLowDelay} {
		call, err := New(Config{LearningDuration: time.Second, Mode: mode})
		if err != nil {
			t.Fatal(err)
		}
		var input [PacketBytes]byte
		var output [OutputBytes]byte
		for i := 0; i < 3; i++ {
			batch, err := call.Process(&input, &output)
			if err != nil {
				t.Fatal(err)
			}
			if i == 1 && batch.Count != map[ProcessingMode]int{Conservative: 0, ExperimentalLowDelay: 1}[mode] {
				t.Fatalf("mode %d emitted %d packets on input 2", mode, batch.Count)
			}
		}
		if err := call.Close(); err != nil {
			t.Fatal(err)
		}
	}
}
