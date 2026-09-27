package noiseox_test

import (
	"bytes"
	"errors"
	"fmt"
	"math"
	"runtime"
	"sync"
	"testing"
	"time"

	noiseox "github.com/valsteen/noise-oxydation/go"
)

type packet = [noiseox.PacketSize]byte

// rng is the xorshift64* generator of the Rust C ABI tests (crates/bindings/noise-oxydation-capi/tests/integration/
// support.rs); both sides must produce the same bytes.
type rng uint64

func newRng(seed uint64) rng {
	return rng(seed*0x9E3779B97F4A7C15 | 1)
}

func (r *rng) next() uint32 {
	x := uint64(*r)
	x ^= x >> 12
	x ^= x << 25
	x ^= x >> 27
	*r = rng(x)
	return uint32((x * 0x2545F4914F6CDD1D) >> 32)
}

// syntheticPackets is the synthetic call audio of the Rust C ABI tests: white noise at a low level for the first
// second, then alternating half-second bursts at two louder levels.
func syntheticPackets(count int, seed uint64) []packet {
	r := newRng(seed)
	packets := make([]packet, count)
	for index := range packets {
		level := uint32(110)
		switch {
		case index < 50:
			level = 30
		case (index/25)%2 == 0:
			level = 60
		}
		for i := range packets[index] {
			draw := r.next()
			magnitude := byte(draw % (level + 1))
			var sign byte
			if draw>>31 == 1 {
				sign = 0x80
			}
			packets[index][i] = ^(sign | magnitude)
		}
	}
	return packets
}

// fnv1a is the 64-bit FNV-1a digest of the Rust C ABI tests.
func fnv1a(data []byte) uint64 {
	hash := uint64(0xCBF29CE484222325)
	for _, b := range data {
		hash ^= uint64(b)
		hash *= 0x100000001B3
	}
	return hash
}

func mustNew(t testing.TB, config noiseox.Config) *noiseox.Call {
	t.Helper()
	call, err := noiseox.New(config)
	if err != nil {
		t.Fatalf("New: %v", err)
	}
	t.Cleanup(func() { call.Close() })
	return call
}

// enhance streams packets through call, drains it, and returns the complete output.
func enhance(t testing.TB, call *noiseox.Call, packets []packet) []byte {
	t.Helper()
	output, err := enhanceCall(call, packets)
	if err != nil {
		t.Fatal(err)
	}
	return output
}

// enhanceCall streams packets through call and drains it. It reports failures as errors rather than through a
// testing.TB so that it can run on goroutines other than the test's own.
func enhanceCall(call *noiseox.Call, packets []packet) ([]byte, error) {
	output := make([]byte, 0, len(packets)*noiseox.PacketSize)
	var out packet
	for i := range packets {
		emitted, err := call.ProcessPacket(&packets[i], &out)
		if err != nil {
			return nil, fmt.Errorf("ProcessPacket %d: %w", i, err)
		}
		if emitted {
			output = append(output, out[:]...)
		}
	}
	var tail [noiseox.DelayPackets]packet
	n, err := call.Drain(&tail)
	if err != nil {
		return nil, fmt.Errorf("drain: %w", err)
	}
	for i := range n {
		output = append(output, tail[i][:]...)
	}
	return output, nil
}

func TestDefaultConfigIsTheRustDefault(t *testing.T) {
	config := noiseox.DefaultConfig()
	// Reference-documented defaults (docs/algorithms.md): 5 s calibration, SPP-MMSE, tonal transient suppression,
	// 80 Hz high-pass pole, minimum window of 50 frames.
	if config.CalibrationDuration != 5*time.Second || config.NoiseEstimator != noiseox.NoiseEstimatorSppMmse ||
		config.Interference != noiseox.InterferenceTonalTransient || config.HighPass.CutoffHz != 80 ||
		config.Minimum.WindowFrames != 50 {
		t.Fatalf("unexpected default %+v", config)
	}
	for _, estimator := range []noiseox.NoiseEstimator{
		noiseox.NoiseEstimatorSppMmse, noiseox.NoiseEstimatorMcra, noiseox.NoiseEstimatorMinimum,
	} {
		for _, interference := range []noiseox.Interference{noiseox.InterferenceTonalTransient, noiseox.InterferenceDisabled} {
			config.NoiseEstimator, config.Interference = estimator, interference
			mustNew(t, config)
		}
	}
}

func TestPacketTimingContract(t *testing.T) {
	for count := range 6 {
		call := mustNew(t, noiseox.DefaultConfig())
		packets := syntheticPackets(count, uint64(count))
		var out packet
		emittedCount := 0
		for i := range packets {
			emitted, err := call.ProcessPacket(&packets[i], &out)
			if err != nil {
				t.Fatal(err)
			}
			if emitted != (i >= noiseox.DelayPackets) {
				t.Fatalf("packet %d of %d: emitted = %v", i, count, emitted)
			}
			if emitted {
				emittedCount++
			}
		}
		var tail [noiseox.DelayPackets]packet
		n, err := call.Drain(&tail)
		if err != nil {
			t.Fatal(err)
		}
		if n != min(count, noiseox.DelayPackets) || emittedCount+n != count {
			t.Fatalf("%d packets: %d emitted and %d drained", count, emittedCount, n)
		}
	}
}

func TestLifecyclePhasesAndDrainedErrors(t *testing.T) {
	config := noiseox.DefaultConfig()
	config.CalibrationDuration = 100 * time.Millisecond
	call := mustNew(t, config)
	packets := syntheticPackets(10, 1)
	var out packet
	var tail [noiseox.DelayPackets]packet

	phase := func() noiseox.Phase {
		p, err := call.Phase()
		if err != nil {
			t.Fatal(err)
		}
		return p
	}
	if phase() != noiseox.PhaseCalibrating {
		t.Fatalf("phase %v, want calibrating", phase())
	}
	for i := range packets {
		if _, err := call.ProcessPacket(&packets[i], &out); err != nil {
			t.Fatal(err)
		}
	}
	// 100 ms calibrates frames 0 to 4; frame 5 completes with packet 6, so ten packets are enhancing.
	if phase() != noiseox.PhaseEnhancing {
		t.Fatalf("phase %v, want enhancing", phase())
	}
	if _, err := call.Drain(&tail); err != nil {
		t.Fatal(err)
	}
	if phase() != noiseox.PhaseDrained {
		t.Fatalf("phase %v, want drained", phase())
	}
	if _, err := call.ProcessPacket(&packets[0], &out); !errors.Is(err, noiseox.ErrDrained) {
		t.Fatalf("ProcessPacket after Drain: %v", err)
	}
	if _, err := call.Drain(&tail); !errors.Is(err, noiseox.ErrDrained) {
		t.Fatalf("Drain after Drain: %v", err)
	}
	if err := call.Reset(); err != nil {
		t.Fatal(err)
	}
	if phase() != noiseox.PhaseCalibrating {
		t.Fatalf("phase after Reset %v", phase())
	}
	if emitted, err := call.ProcessPacket(&packets[0], &out); err != nil || emitted {
		t.Fatalf("first packet after Reset: %v %v", emitted, err)
	}
}

func TestNilArraysAreRejected(t *testing.T) {
	call := mustNew(t, noiseox.DefaultConfig())
	var in, out packet
	if _, err := call.ProcessPacket(nil, &out); !errors.Is(err, noiseox.ErrNilArgument) {
		t.Fatalf("nil input: %v", err)
	}
	if _, err := call.ProcessPacket(&in, nil); !errors.Is(err, noiseox.ErrNilArgument) {
		t.Fatalf("nil output: %v", err)
	}
	if _, err := call.Drain(nil); !errors.Is(err, noiseox.ErrNilArgument) {
		t.Fatalf("nil drain buffer: %v", err)
	}
}

func TestInPlaceProcessingEqualsSeparateArrays(t *testing.T) {
	packets := syntheticPackets(40, 3)
	separate := mustNew(t, noiseox.DefaultConfig())
	inPlace := mustNew(t, noiseox.DefaultConfig())
	var out packet
	for i := range packets {
		emitted, err := separate.ProcessPacket(&packets[i], &out)
		if err != nil {
			t.Fatal(err)
		}
		buffer := packets[i]
		emittedInPlace, err := inPlace.ProcessPacket(&buffer, &buffer)
		if err != nil || emittedInPlace != emitted {
			t.Fatalf("packet %d: %v %v", i, emittedInPlace, err)
		}
		if emitted && buffer != out {
			t.Fatalf("packet %d differs in place", i)
		}
	}
}

func TestResetEqualsAFreshCall(t *testing.T) {
	packets := syntheticPackets(120, 20260927)
	fresh := enhance(t, mustNew(t, noiseox.DefaultConfig()), packets)

	reused := mustNew(t, noiseox.DefaultConfig())
	var out packet
	for _, p := range syntheticPackets(90, 7) {
		if _, err := reused.ProcessPacket(&p, &out); err != nil {
			t.Fatal(err)
		}
	}
	if err := reused.Reset(); err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(enhance(t, reused, packets), fresh) {
		t.Fatal("output after Reset differs from a fresh call")
	}
}

func TestCloseIsIdempotentAndUseAfterCloseFails(t *testing.T) {
	call, err := noiseox.New(noiseox.DefaultConfig())
	if err != nil {
		t.Fatal(err)
	}
	if err := call.Close(); err != nil {
		t.Fatal(err)
	}
	if err := call.Close(); err != nil {
		t.Fatalf("second Close: %v", err)
	}
	var in, out packet
	var tail [noiseox.DelayPackets]packet
	if _, err := call.ProcessPacket(&in, &out); !errors.Is(err, noiseox.ErrClosed) {
		t.Fatalf("ProcessPacket: %v", err)
	}
	if _, err := call.Drain(&tail); !errors.Is(err, noiseox.ErrClosed) {
		t.Fatalf("Drain: %v", err)
	}
	if err := call.Reset(); !errors.Is(err, noiseox.ErrClosed) {
		t.Fatalf("Reset: %v", err)
	}
	if _, err := call.Phase(); !errors.Is(err, noiseox.ErrClosed) {
		t.Fatalf("Phase: %v", err)
	}
	var nilCall *noiseox.Call
	if _, err := nilCall.Phase(); !errors.Is(err, noiseox.ErrClosed) {
		t.Fatalf("nil Call: %v", err)
	}
}

func TestCleanupReleasesForgottenCallsAndCloseStopsIt(t *testing.T) {
	// A forgotten Call is freed by its cleanup; a closed one must not be freed again. A double free or a use after
	// free here aborts the test binary.
	for range 64 {
		if _, err := noiseox.New(noiseox.DefaultConfig()); err != nil {
			t.Fatal(err)
		}
		closed, err := noiseox.New(noiseox.DefaultConfig())
		if err != nil {
			t.Fatal(err)
		}
		closed.Close()
	}
	for range 3 {
		runtime.GC()
		time.Sleep(10 * time.Millisecond)
	}
}

func configError(t *testing.T, config noiseox.Config) *noiseox.ConfigError {
	t.Helper()
	call, err := noiseox.New(config)
	if err == nil {
		call.Close()
		t.Fatal("New accepted an invalid configuration")
	}
	if !errors.Is(err, noiseox.ErrInvalidConfig) {
		t.Fatalf("%v does not match ErrInvalidConfig", err)
	}
	var configErr *noiseox.ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("%v is not a *ConfigError", err)
	}
	return configErr
}

func TestConfigErrorKindsCarryTheRustDetails(t *testing.T) {
	t.Run("out of range", func(t *testing.T) {
		config := noiseox.DefaultConfig()
		config.SppMmse.SpeechPrior = 1.5
		got := *configError(t, config)
		want := noiseox.ConfigError{
			Kind: noiseox.KindOutOfRange, Field: noiseox.FieldSppMmseSpeechPrior, Value: 1.5,
			Constraint: noiseox.ConstraintOpenUnit, Bounds: noiseox.Bounds{Lower: 0, Upper: 1},
		}
		if got != want {
			t.Fatalf("got %+v, want %+v", got, want)
		}
		if msg := got.Error(); msg != "noiseox: spp_mmse.speech_prior must be in (0, 1), got 1.5" {
			t.Fatalf("message %q", msg)
		}
		if got.Field.String() != "spp_mmse.speech_prior" || got.Constraint.String() != "in (0, 1)" {
			t.Fatalf("names %q %q", got.Field, got.Constraint)
		}
	})
	t.Run("out of range with an unbounded constraint and NaN", func(t *testing.T) {
		config := noiseox.DefaultConfig()
		config.LogMmse.NoiseOverestimation = float32(math.NaN())
		got := configError(t, config)
		if got.Kind != noiseox.KindOutOfRange || got.Field != noiseox.FieldLogMmseNoiseOverestimation ||
			!math.IsNaN(float64(got.Value)) || got.Constraint != noiseox.ConstraintPositive ||
			!math.IsInf(float64(got.Bounds.Upper), 1) || got.Bounds.LowerInclusive {
			t.Fatalf("got %+v", got)
		}
		if msg := got.Error(); msg != "noiseox: log_mmse.noise_overestimation must be finite and greater than 0, got NaN" {
			t.Fatalf("message %q", msg)
		}
	})
	t.Run("min gain above max gain", func(t *testing.T) {
		config := noiseox.DefaultConfig()
		config.LogMmse.MinGain, config.LogMmse.MaxGain = 0.9, 0.2
		got := *configError(t, config)
		want := noiseox.ConfigError{Kind: noiseox.KindMinGainAboveMaxGain, Value: 0.9, SecondValue: 0.2}
		if got != want {
			t.Fatalf("got %+v, want %+v", got, want)
		}
		if msg := got.Error(); msg != "noiseox: log_mmse.min_gain (0.9) must not exceed log_mmse.max_gain (0.2)" {
			t.Fatalf("message %q", msg)
		}
	})
	t.Run("count out of range", func(t *testing.T) {
		config := noiseox.DefaultConfig()
		config.NoiseEstimator = noiseox.NoiseEstimatorMinimum
		config.Minimum.WindowFrames = 1001
		got := *configError(t, config)
		want := noiseox.ConfigError{
			Kind: noiseox.KindCountOutOfRange, Field: noiseox.FieldMinimumWindowFrames,
			Count: 1001, CountMin: 1, CountMax: 1000,
		}
		if got != want {
			t.Fatalf("got %+v, want %+v", got, want)
		}
		if msg := got.Error(); msg != "noiseox: minimum.window_frames must be in [1, 1000], got 1001" {
			t.Fatalf("message %q", msg)
		}
	})
	t.Run("start not below full", func(t *testing.T) {
		config := noiseox.DefaultConfig()
		config.TonalTransient.FluxStartDb = 20
		got := *configError(t, config)
		want := noiseox.ConfigError{
			Kind: noiseox.KindStartNotBelowFull, Field: noiseox.FieldTonalTransientFluxStartDb, Value: 20,
			SecondField: noiseox.FieldTonalTransientFluxFullDb, SecondValue: 18,
		}
		if got != want {
			t.Fatalf("got %+v, want %+v", got, want)
		}
		want2 := "noiseox: tonal_transient.flux_start_db (20) must be less than tonal_transient.flux_full_db (18)"
		if msg := got.Error(); msg != want2 {
			t.Fatalf("message %q", msg)
		}
	})
	t.Run("unknown selector", func(t *testing.T) {
		config := noiseox.DefaultConfig()
		config.Interference = 9
		got := *configError(t, config)
		want := noiseox.ConfigError{
			Kind: noiseox.KindUnknownSelector, Selector: noiseox.SelectorInterference, SelectorValue: 9,
		}
		if got != want {
			t.Fatalf("got %+v, want %+v", got, want)
		}
		if msg := got.Error(); msg != "noiseox: interference must name a variant, got 9" {
			t.Fatalf("message %q", msg)
		}
		if got := configError(t, noiseox.Config{}); got.Kind != noiseox.KindUnknownSelector {
			t.Fatalf("the zero Config gave %+v", got)
		}
	})
	t.Run("negative calibration duration", func(t *testing.T) {
		config := noiseox.DefaultConfig()
		config.CalibrationDuration = -time.Second
		got := *configError(t, config)
		want := noiseox.ConfigError{Kind: noiseox.KindNegativeCalibrationDuration, CalibrationDuration: -time.Second}
		if got != want {
			t.Fatalf("got %+v, want %+v", got, want)
		}
		if msg := got.Error(); msg != "noiseox: calibration_duration must not be negative, got -1s" {
			t.Fatalf("message %q", msg)
		}
	})
	t.Run("calibration too long", func(t *testing.T) {
		// Unreachable through New (see KindCalibrationTooLong); the message still comes from Rust.
		err := &noiseox.ConfigError{
			Kind: noiseox.KindCalibrationTooLong, CalibrationSeconds: math.MaxUint64, CalibrationNanos: 999999999,
		}
		if !errors.Is(err, noiseox.ErrInvalidConfig) ||
			err.Error() != "noiseox: calibration_duration 18446744073709551615.999999999s has too many samples at 8 kHz" {
			t.Fatalf("message %q", err.Error())
		}
	})
}

// TestEveryFieldIsCarriedToRust sets each validated field to an invalid value and checks that Rust names that field,
// which proves that every Config field reaches its Rust counterpart.
func TestEveryFieldIsCarriedToRust(t *testing.T) {
	type fieldCase struct {
		field  noiseox.ConfigField
		name   string
		count  bool
		mutate func(*noiseox.Config)
	}
	mcra := func(c *noiseox.Config) { c.NoiseEstimator = noiseox.NoiseEstimatorMcra }
	minimum := func(c *noiseox.Config) { c.NoiseEstimator = noiseox.NoiseEstimatorMinimum }
	cases := []fieldCase{
		{noiseox.FieldHighPassCutoffHz, "high_pass.cutoff_hz", false, func(c *noiseox.Config) { c.HighPass.CutoffHz = -1 }},
		{noiseox.FieldSppMmseNoiseSmoothing, "spp_mmse.noise_smoothing", false, func(c *noiseox.Config) { c.SppMmse.NoiseSmoothing = -1 }},
		{noiseox.FieldSppMmseSppSmoothing, "spp_mmse.spp_smoothing", false, func(c *noiseox.Config) { c.SppMmse.SppSmoothing = -1 }},
		{noiseox.FieldSppMmseSpeechPrior, "spp_mmse.speech_prior", false, func(c *noiseox.Config) { c.SppMmse.SpeechPrior = -1 }},
		{noiseox.FieldSppMmseFixedPriorSnr, "spp_mmse.fixed_prior_snr", false, func(c *noiseox.Config) { c.SppMmse.FixedPriorSnr = -1 }},
		{noiseox.FieldSppMmseStagnationThreshold, "spp_mmse.stagnation_threshold", false, func(c *noiseox.Config) { c.SppMmse.StagnationThreshold = -1 }},
		{noiseox.FieldSppMmseMaxSpeechProbability, "spp_mmse.max_speech_probability", false, func(c *noiseox.Config) { c.SppMmse.MaxSpeechProbability = -1 }},
		{noiseox.FieldSppMmseFloor, "spp_mmse.floor", false, func(c *noiseox.Config) { c.SppMmse.Floor = -1 }},
		{noiseox.FieldDecisionDirectedAlpha, "decision_directed.alpha", false, func(c *noiseox.Config) { c.DecisionDirected.Alpha = -1 }},
		{noiseox.FieldDecisionDirectedFloor, "decision_directed.floor", false, func(c *noiseox.Config) { c.DecisionDirected.Floor = -1 }},
		{noiseox.FieldLogMmseMinGain, "log_mmse.min_gain", false, func(c *noiseox.Config) { c.LogMmse.MinGain = -1 }},
		{noiseox.FieldLogMmseMaxGain, "log_mmse.max_gain", false, func(c *noiseox.Config) { c.LogMmse.MaxGain = -1 }},
		{noiseox.FieldLogMmseFloor, "log_mmse.floor", false, func(c *noiseox.Config) { c.LogMmse.Floor = -1 }},
		{noiseox.FieldLogMmseNoiseOverestimation, "log_mmse.noise_overestimation", false, func(c *noiseox.Config) { c.LogMmse.NoiseOverestimation = -1 }},
		{noiseox.FieldMcraSmoothing, "mcra.smoothing", false, func(c *noiseox.Config) { mcra(c); c.Mcra.Smoothing = -1 }},
		{noiseox.FieldMcraSpeechSmoothing, "mcra.speech_smoothing", false, func(c *noiseox.Config) { mcra(c); c.Mcra.SpeechSmoothing = -1 }},
		{noiseox.FieldMcraNoiseSmoothing, "mcra.noise_smoothing", false, func(c *noiseox.Config) { mcra(c); c.Mcra.NoiseSmoothing = -1 }},
		{noiseox.FieldMcraRatioThreshold, "mcra.ratio_threshold", false, func(c *noiseox.Config) { mcra(c); c.Mcra.RatioThreshold = -1 }},
		{noiseox.FieldMcraWindowFrames, "mcra.window_frames", true, func(c *noiseox.Config) { mcra(c); c.Mcra.WindowFrames = 0 }},
		{noiseox.FieldMcraFloor, "mcra.floor", false, func(c *noiseox.Config) { mcra(c); c.Mcra.Floor = -1 }},
		{noiseox.FieldMinimumSmoothing, "minimum.smoothing", false, func(c *noiseox.Config) { minimum(c); c.Minimum.Smoothing = -1 }},
		{noiseox.FieldMinimumWindowFrames, "minimum.window_frames", true, func(c *noiseox.Config) { minimum(c); c.Minimum.WindowFrames = 0 }},
		{noiseox.FieldMinimumFloor, "minimum.floor", false, func(c *noiseox.Config) { minimum(c); c.Minimum.Floor = -1 }},
		{noiseox.FieldTonalTransientLocalRadius, "tonal_transient.local_radius", true, func(c *noiseox.Config) { c.TonalTransient.LocalRadius = 0 }},
		{noiseox.FieldTonalTransientTonalStartDb, "tonal_transient.tonal_start_db", false, func(c *noiseox.Config) { c.TonalTransient.TonalStartDb = -1 }},
		{noiseox.FieldTonalTransientTonalFullDb, "tonal_transient.tonal_full_db", false, func(c *noiseox.Config) { c.TonalTransient.TonalFullDb = -1 }},
		{noiseox.FieldTonalTransientFluxStartDb, "tonal_transient.flux_start_db", false, func(c *noiseox.Config) { c.TonalTransient.FluxStartDb = -1 }},
		{noiseox.FieldTonalTransientFluxFullDb, "tonal_transient.flux_full_db", false, func(c *noiseox.Config) { c.TonalTransient.FluxFullDb = -1 }},
		{noiseox.FieldTonalTransientMovementSearchRadius, "tonal_transient.movement_search_radius", true, func(c *noiseox.Config) { c.TonalTransient.MovementSearchRadius = 0 }},
		{noiseox.FieldTonalTransientMovementStartBins, "tonal_transient.movement_start_bins", true, func(c *noiseox.Config) { c.TonalTransient.MovementStartBins = 0 }},
		{noiseox.FieldTonalTransientMovementFullBins, "tonal_transient.movement_full_bins", true, func(c *noiseox.Config) { c.TonalTransient.MovementFullBins = 0 }},
		{noiseox.FieldTonalTransientMinFrequencyBin, "tonal_transient.min_frequency_bin", true, func(c *noiseox.Config) { c.TonalTransient.MinFrequencyBin = 0 }},
		{noiseox.FieldTonalTransientMovementMinRelativePower, "tonal_transient.movement_min_relative_power", false, func(c *noiseox.Config) { c.TonalTransient.MovementMinRelativePower = -1 }},
		{noiseox.FieldTonalTransientHarmonicToleranceBins, "tonal_transient.harmonic_tolerance_bins", true, func(c *noiseox.Config) { c.TonalTransient.HarmonicToleranceBins = 17 }},
		{noiseox.FieldTonalTransientHarmonicRelativePower, "tonal_transient.harmonic_relative_power", false, func(c *noiseox.Config) { c.TonalTransient.HarmonicRelativePower = -1 }},
		{noiseox.FieldTonalTransientStrength, "tonal_transient.strength", false, func(c *noiseox.Config) { c.TonalTransient.Strength = -1 }},
		{noiseox.FieldTonalTransientMinGain, "tonal_transient.min_gain", false, func(c *noiseox.Config) { c.TonalTransient.MinGain = -1 }},
		{noiseox.FieldTonalTransientAttack, "tonal_transient.attack", false, func(c *noiseox.Config) { c.TonalTransient.Attack = -1 }},
		{noiseox.FieldTonalTransientRelease, "tonal_transient.release", false, func(c *noiseox.Config) { c.TonalTransient.Release = -1 }},
		{noiseox.FieldTonalTransientSpreadRadius, "tonal_transient.spread_radius", true, func(c *noiseox.Config) { c.TonalTransient.SpreadRadius = 65 }},
		{noiseox.FieldTonalTransientFloor, "tonal_transient.floor", false, func(c *noiseox.Config) { c.TonalTransient.Floor = -1 }},
	}
	if len(cases) != 41 {
		t.Fatalf("%d cases, want one per Rust field (41)", len(cases))
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			config := noiseox.DefaultConfig()
			c.mutate(&config)
			got := configError(t, config)
			wantKind := noiseox.KindOutOfRange
			if c.count {
				wantKind = noiseox.KindCountOutOfRange
			}
			if got.Field != c.field || got.Field.String() != c.name || got.Kind != wantKind {
				t.Fatalf("got %+v (%s), want %s", got, got.Field, c.name)
			}
		})
	}
}

// Digests of the complete output of the Rust C ABI equality test (crates/bindings/noise-oxydation-capi/tests/
// integration/equality.rs), which checks them against the Rust API: 400 synthetic packets, seed 20260927, 1 s
// calibration.
var rustDigests = []struct {
	name   string
	digest uint64
	config func() noiseox.Config
}{
	{"spp-mmse", 0x75F120C1C543C7FD, func() noiseox.Config {
		return equalityConfig(noiseox.NoiseEstimatorSppMmse, noiseox.InterferenceTonalTransient)
	}},
	{"mcra-no-interference", 0x8854DF407A8F2FB5, func() noiseox.Config { return equalityConfig(noiseox.NoiseEstimatorMcra, noiseox.InterferenceDisabled) }},
	{"minimum", 0x9507EAB0E56C8221, func() noiseox.Config {
		return equalityConfig(noiseox.NoiseEstimatorMinimum, noiseox.InterferenceTonalTransient)
	}},
}

func equalityConfig(estimator noiseox.NoiseEstimator, interference noiseox.Interference) noiseox.Config {
	config := noiseox.DefaultConfig()
	config.CalibrationDuration = time.Second
	config.NoiseEstimator = estimator
	config.Interference = interference
	return config
}

func TestOutputEqualsTheRustOutput(t *testing.T) {
	packets := syntheticPackets(400, 20260927)
	for _, c := range rustDigests {
		output := enhance(t, mustNew(t, c.config()), packets)
		if len(output) != len(packets)*noiseox.PacketSize {
			t.Fatalf("%s: %d output bytes", c.name, len(output))
		}
		if got := fnv1a(output); got != c.digest {
			t.Fatalf("%s: digest %#x, want %#x", c.name, got, c.digest)
		}
	}
}

func TestParallelCallsEqualSequentialCalls(t *testing.T) {
	const calls = 12
	inputs := make([][]packet, calls)
	sequential := make([][]byte, calls)
	for i := range calls {
		inputs[i] = syntheticPackets(300, uint64(100+i))
		sequential[i] = enhance(t, mustNew(t, rustDigests[i%len(rustDigests)].config()), inputs[i])
	}
	parallel := make([][]byte, calls)
	errs := make([]error, calls)
	var wg sync.WaitGroup
	for i := range calls {
		call := mustNew(t, rustDigests[i%len(rustDigests)].config())
		wg.Add(1)
		go func() {
			defer wg.Done()
			parallel[i], errs[i] = enhanceCall(call, inputs[i])
		}()
	}
	wg.Wait()
	for i := range calls {
		if errs[i] != nil {
			t.Fatalf("parallel call %d: %v", i, errs[i])
		}
		if !bytes.Equal(parallel[i], sequential[i]) {
			t.Fatalf("call %d differs between parallel and sequential runs", i)
		}
	}
}

func TestHotPathAllocatesNothing(t *testing.T) {
	call := mustNew(t, noiseox.DefaultConfig())
	packets := syntheticPackets(64, 5)
	in, out := new(packet), new(packet)
	var tail [noiseox.DelayPackets]packet
	next := 0
	process := testing.AllocsPerRun(2000, func() {
		*in = packets[next%len(packets)]
		next++
		if _, err := call.ProcessPacket(in, out); err != nil {
			panic(err)
		}
	})
	lifecycle := testing.AllocsPerRun(200, func() {
		for i := range 3 {
			if _, err := call.ProcessPacket(&packets[i], out); err != nil {
				panic(err)
			}
		}
		if _, err := call.Drain(&tail); err != nil {
			panic(err)
		}
		if _, err := call.Phase(); err != nil {
			panic(err)
		}
		if err := call.Reset(); err != nil {
			panic(err)
		}
	})
	if process != 0 || lifecycle != 0 {
		t.Fatalf("allocations per run: ProcessPacket %v, lifecycle %v", process, lifecycle)
	}
}

func BenchmarkProcessPacket(b *testing.B) {
	call := mustNew(b, noiseox.DefaultConfig())
	packets := syntheticPackets(2500, 11)
	var out packet
	b.ReportAllocs()
	i := 0
	for b.Loop() {
		if _, err := call.ProcessPacket(&packets[i], &out); err != nil {
			b.Fatal(err)
		}
		if i++; i == len(packets) {
			i = 0
			if err := call.Reset(); err != nil {
				b.Fatal(err)
			}
		}
	}
}

// BenchmarkPhase measures a Go-to-Rust call that does almost no work: the cgo crossing cost.
func BenchmarkPhase(b *testing.B) {
	call := mustNew(b, noiseox.DefaultConfig())
	b.ReportAllocs()
	for b.Loop() {
		if _, err := call.Phase(); err != nil {
			b.Fatal(err)
		}
	}
}
