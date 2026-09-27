package noiseox

// #include "noise_oxydation.h"
import "C"

import (
	"errors"
	"fmt"
	"time"
	"unsafe"
)

// Lifecycle and category errors. The messages of the Rust statuses come from the Rust library.
var (
	// ErrInvalidConfig matches every *ConfigError with errors.Is.
	ErrInvalidConfig = errors.New("noiseox: " + statusMessage(C.NOX_STATUS_INVALID_CONFIG))
	// ErrDrained is returned by ProcessPacket and Drain after Drain, until Reset.
	ErrDrained = errors.New("noiseox: " + statusMessage(C.NOX_STATUS_DRAINED))
	// ErrClosed is returned by every method of a Call after Close; Rust is not called.
	ErrClosed = errors.New("noiseox: the call is closed")
	// ErrNilArgument is returned when a nil packet array is passed.
	ErrNilArgument = errors.New("noiseox: " + statusMessage(C.NOX_STATUS_NULL_ARGUMENT))
	// ErrPanic reports a panic inside the Rust library. The panic did not unwind into Go; the Call answers every
	// later method with ErrPanic and should be closed.
	ErrPanic = errors.New("noiseox: " + statusMessage(C.NOX_STATUS_PANIC))
)

func statusMessage(status C.nox_status) string {
	return goString(C.nox_status_message(status))
}

func goString(s C.nox_str) string {
	return C.GoStringN((*C.char)(unsafe.Pointer(s.ptr)), C.int(s.len))
}

// statusError maps a status to its error without allocating; OK maps to nil.
func statusError(status C.nox_status) error {
	switch status {
	case C.NOX_STATUS_OK:
		return nil
	case C.NOX_STATUS_NULL_ARGUMENT:
		return ErrNilArgument
	case C.NOX_STATUS_INVALID_CONFIG:
		return ErrInvalidConfig
	case C.NOX_STATUS_DRAINED:
		return ErrDrained
	case C.NOX_STATUS_PANIC:
		return ErrPanic
	default:
		return fmt.Errorf("noiseox: unknown status %d", uint32(status))
	}
}

// errorFromDetail maps the detail of a failed nox_call_new.
func errorFromDetail(detail *C.nox_error) error {
	if detail.status != C.NOX_STATUS_INVALID_CONFIG {
		return statusError(detail.status)
	}
	return &ConfigError{
		Kind:        ConfigErrorKind(detail.config_error),
		Field:       ConfigField(detail.field),
		SecondField: ConfigField(detail.second_field),
		Value:       float32(detail.value),
		SecondValue: float32(detail.second_value),
		Constraint:  Constraint(detail.constraint),
		Bounds: Bounds{
			Lower:          float32(detail.constraint_lower),
			Upper:          float32(detail.constraint_upper),
			LowerInclusive: detail.constraint_lower_inclusive != 0,
			UpperInclusive: detail.constraint_upper_inclusive != 0,
		},
		Count:              uint16(detail.count_value),
		CountMin:           uint16(detail.count_min),
		CountMax:           uint16(detail.count_max),
		Selector:           Selector(detail.selector),
		SelectorValue:      uint32(detail.selector_value),
		CalibrationSeconds: uint64(detail.duration_secs),
		CalibrationNanos:   uint32(detail.duration_subsec_nanos),
	}
}

// ConfigErrorKind names the variant of a configuration error: one kind per Rust ConfigError variant, plus the checks
// made before Rust sees the configuration.
type ConfigErrorKind uint32

// Configuration error kinds. The comment of each kind lists the ConfigError fields it sets.
const (
	// KindOutOfRange sets Field, Value, Constraint and Bounds. Value may be NaN.
	KindOutOfRange ConfigErrorKind = C.NOX_CONFIG_ERROR_OUT_OF_RANGE
	// KindMinGainAboveMaxGain sets Value to log_mmse.min_gain and SecondValue to log_mmse.max_gain.
	KindMinGainAboveMaxGain ConfigErrorKind = C.NOX_CONFIG_ERROR_MIN_GAIN_ABOVE_MAX_GAIN
	// KindCountOutOfRange sets Field, Count, CountMin and CountMax.
	KindCountOutOfRange ConfigErrorKind = C.NOX_CONFIG_ERROR_COUNT_OUT_OF_RANGE
	// KindStartNotBelowFull sets Field and Value to the start threshold, SecondField and SecondValue to the full one.
	KindStartNotBelowFull ConfigErrorKind = C.NOX_CONFIG_ERROR_START_NOT_BELOW_FULL
	// KindCalibrationTooLong sets CalibrationSeconds and CalibrationNanos. The Rust library rejects durations whose
	// sample count overflows 64 bits; no time.Duration is that long, so Go callers never receive this kind.
	KindCalibrationTooLong ConfigErrorKind = C.NOX_CONFIG_ERROR_CALIBRATION_TOO_LONG
	// KindUnknownSelector sets Selector and SelectorValue, for a NoiseEstimator or Interference without a variant.
	KindUnknownSelector ConfigErrorKind = C.NOX_CONFIG_ERROR_UNKNOWN_SELECTOR
	// KindOther is a Rust ConfigError variant this version of the binding does not describe.
	KindOther ConfigErrorKind = C.NOX_CONFIG_ERROR_OTHER
	// KindNegativeCalibrationDuration sets CalibrationDuration. It is checked in Go: Rust durations cannot be negative.
	KindNegativeCalibrationDuration ConfigErrorKind = 1000
)

func (k ConfigErrorKind) String() string {
	switch k {
	case KindOutOfRange:
		return "out of range"
	case KindMinGainAboveMaxGain:
		return "min gain above max gain"
	case KindCountOutOfRange:
		return "count out of range"
	case KindStartNotBelowFull:
		return "start not below full"
	case KindCalibrationTooLong:
		return "calibration too long"
	case KindUnknownSelector:
		return "unknown selector"
	case KindOther:
		return "other"
	case KindNegativeCalibrationDuration:
		return "negative calibration duration"
	default:
		return fmt.Sprintf("kind(%d)", uint32(k))
	}
}

// ConfigField identifies a validated configuration field. String returns its qualified Rust name, for example
// "spp_mmse.speech_prior".
type ConfigField uint32

// Configuration fields, in the order of the Rust ConfigField, named after its qualified field names. FieldNone marks
// an unset field.
const (
	FieldNone                                   ConfigField = C.NOX_FIELD_NONE
	FieldHighPassCutoffHz                       ConfigField = C.NOX_FIELD_HIGH_PASS_CUTOFF_HZ
	FieldSppMmseNoiseSmoothing                  ConfigField = C.NOX_FIELD_SPP_MMSE_NOISE_SMOOTHING
	FieldSppMmseSppSmoothing                    ConfigField = C.NOX_FIELD_SPP_MMSE_SPP_SMOOTHING
	FieldSppMmseSpeechPrior                     ConfigField = C.NOX_FIELD_SPP_MMSE_SPEECH_PRIOR
	FieldSppMmseFixedPriorSnr                   ConfigField = C.NOX_FIELD_SPP_MMSE_FIXED_PRIOR_SNR
	FieldSppMmseStagnationThreshold             ConfigField = C.NOX_FIELD_SPP_MMSE_STAGNATION_THRESHOLD
	FieldSppMmseMaxSpeechProbability            ConfigField = C.NOX_FIELD_SPP_MMSE_MAX_SPEECH_PROBABILITY
	FieldSppMmseFloor                           ConfigField = C.NOX_FIELD_SPP_MMSE_FLOOR
	FieldDecisionDirectedAlpha                  ConfigField = C.NOX_FIELD_DECISION_DIRECTED_ALPHA
	FieldDecisionDirectedFloor                  ConfigField = C.NOX_FIELD_DECISION_DIRECTED_FLOOR
	FieldLogMmseMinGain                         ConfigField = C.NOX_FIELD_LOG_MMSE_MIN_GAIN
	FieldLogMmseMaxGain                         ConfigField = C.NOX_FIELD_LOG_MMSE_MAX_GAIN
	FieldLogMmseFloor                           ConfigField = C.NOX_FIELD_LOG_MMSE_FLOOR
	FieldLogMmseNoiseOverestimation             ConfigField = C.NOX_FIELD_LOG_MMSE_NOISE_OVERESTIMATION
	FieldMcraSmoothing                          ConfigField = C.NOX_FIELD_MCRA_SMOOTHING
	FieldMcraSpeechSmoothing                    ConfigField = C.NOX_FIELD_MCRA_SPEECH_SMOOTHING
	FieldMcraNoiseSmoothing                     ConfigField = C.NOX_FIELD_MCRA_NOISE_SMOOTHING
	FieldMcraRatioThreshold                     ConfigField = C.NOX_FIELD_MCRA_RATIO_THRESHOLD
	FieldMcraWindowFrames                       ConfigField = C.NOX_FIELD_MCRA_WINDOW_FRAMES
	FieldMcraFloor                              ConfigField = C.NOX_FIELD_MCRA_FLOOR
	FieldMinimumSmoothing                       ConfigField = C.NOX_FIELD_MINIMUM_SMOOTHING
	FieldMinimumWindowFrames                    ConfigField = C.NOX_FIELD_MINIMUM_WINDOW_FRAMES
	FieldMinimumFloor                           ConfigField = C.NOX_FIELD_MINIMUM_FLOOR
	FieldTonalTransientLocalRadius              ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_LOCAL_RADIUS
	FieldTonalTransientTonalStartDb             ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_TONAL_START_DB
	FieldTonalTransientTonalFullDb              ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_TONAL_FULL_DB
	FieldTonalTransientFluxStartDb              ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_FLUX_START_DB
	FieldTonalTransientFluxFullDb               ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_FLUX_FULL_DB
	FieldTonalTransientMovementSearchRadius     ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_MOVEMENT_SEARCH_RADIUS
	FieldTonalTransientMovementStartBins        ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_MOVEMENT_START_BINS
	FieldTonalTransientMovementFullBins         ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_MOVEMENT_FULL_BINS
	FieldTonalTransientMinFrequencyBin          ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_MIN_FREQUENCY_BIN
	FieldTonalTransientMovementMinRelativePower ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_MOVEMENT_MIN_RELATIVE_POWER
	FieldTonalTransientHarmonicToleranceBins    ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_HARMONIC_TOLERANCE_BINS
	FieldTonalTransientHarmonicRelativePower    ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_HARMONIC_RELATIVE_POWER
	FieldTonalTransientStrength                 ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_STRENGTH
	FieldTonalTransientMinGain                  ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_MIN_GAIN
	FieldTonalTransientAttack                   ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_ATTACK
	FieldTonalTransientRelease                  ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_RELEASE
	FieldTonalTransientSpreadRadius             ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_SPREAD_RADIUS
	FieldTonalTransientFloor                    ConfigField = C.NOX_FIELD_TONAL_TRANSIENT_FLOOR
)

func (f ConfigField) String() string {
	if name := goString(C.nox_config_field_name(C.uint32_t(f))); name != "" {
		return name
	}
	return fmt.Sprintf("field(%d)", uint32(f))
}

// Constraint identifies the range a numeric field must lie in. String returns the Rust description, for example
// "in (0, 1)".
type Constraint uint32

// Constraints, in the order of the Rust Constraint. Every range excludes NaN and infinities.
const (
	ConstraintNone              Constraint = C.NOX_CONSTRAINT_NONE
	ConstraintOpenUnit          Constraint = C.NOX_CONSTRAINT_OPEN_UNIT
	ConstraintUnitExcludingOne  Constraint = C.NOX_CONSTRAINT_UNIT_EXCLUDING_ONE
	ConstraintUnitExcludingZero Constraint = C.NOX_CONSTRAINT_UNIT_EXCLUDING_ZERO
	ConstraintClosedUnit        Constraint = C.NOX_CONSTRAINT_CLOSED_UNIT
	ConstraintPositive          Constraint = C.NOX_CONSTRAINT_POSITIVE
	ConstraintNonNegative       Constraint = C.NOX_CONSTRAINT_NON_NEGATIVE
	ConstraintGreaterThanOne    Constraint = C.NOX_CONSTRAINT_GREATER_THAN_ONE
	ConstraintBelowNyquist      Constraint = C.NOX_CONSTRAINT_BELOW_NYQUIST
)

func (c Constraint) String() string {
	if description := goString(C.nox_constraint_description(C.uint32_t(c))); description != "" {
		return description
	}
	return fmt.Sprintf("constraint(%d)", uint32(c))
}

// Bounds are the limits of a Constraint. Upper is +Inf for ranges without an upper bound, which still require finite
// values.
type Bounds struct {
	Lower, Upper                   float32
	LowerInclusive, UpperInclusive bool
}

// Selector names a selector field of Config.
type Selector uint32

// Selectors.
const (
	SelectorNone           Selector = C.NOX_SELECTOR_NONE
	SelectorNoiseEstimator Selector = C.NOX_SELECTOR_NOISE_ESTIMATOR
	SelectorInterference   Selector = C.NOX_SELECTOR_INTERFERENCE
)

func (s Selector) String() string {
	switch s {
	case SelectorNoiseEstimator:
		return "noise_estimator"
	case SelectorInterference:
		return "interference"
	default:
		return "none"
	}
}

// ConfigError is a rejected configuration. Kind names the variant, and the fields listed at the kind hold its data;
// the others are zero. It matches ErrInvalidConfig with errors.Is, and its message is the Rust error's message.
type ConfigError struct {
	Kind ConfigErrorKind
	// Field is the offending field, or the start threshold of KindStartNotBelowFull.
	Field ConfigField
	// SecondField is the full threshold of KindStartNotBelowFull.
	SecondField ConfigField
	// Value is the rejected value, log_mmse.min_gain, or the start value.
	Value float32
	// SecondValue is log_mmse.max_gain or the full value.
	SecondValue float32
	// Constraint and Bounds describe the range Value violated.
	Constraint Constraint
	Bounds     Bounds
	// Count, CountMin and CountMax describe a rejected integer field.
	Count, CountMin, CountMax uint16
	// Selector and SelectorValue describe an unknown selector value.
	Selector      Selector
	SelectorValue uint32
	// CalibrationSeconds and CalibrationNanos are the rejected Rust calibration duration.
	CalibrationSeconds uint64
	CalibrationNanos   uint32
	// CalibrationDuration is the rejected negative duration.
	CalibrationDuration time.Duration
}

// Error returns the Rust message of the error, which the Rust library formats from the error's fields.
func (e *ConfigError) Error() string {
	if e.Kind == KindNegativeCalibrationDuration {
		return fmt.Sprintf("noiseox: calibration_duration must not be negative, got %v", e.CalibrationDuration)
	}
	detail := C.nox_error{
		status:                     C.NOX_STATUS_INVALID_CONFIG,
		config_error:               C.uint32_t(e.Kind),
		field:                      C.uint32_t(e.Field),
		second_field:               C.uint32_t(e.SecondField),
		constraint:                 C.uint32_t(e.Constraint),
		value:                      C.float(e.Value),
		second_value:               C.float(e.SecondValue),
		constraint_lower:           C.float(e.Bounds.Lower),
		constraint_upper:           C.float(e.Bounds.Upper),
		constraint_lower_inclusive: boolByte(e.Bounds.LowerInclusive),
		constraint_upper_inclusive: boolByte(e.Bounds.UpperInclusive),
		count_value:                C.uint16_t(e.Count),
		count_min:                  C.uint16_t(e.CountMin),
		count_max:                  C.uint16_t(e.CountMax),
		selector:                   C.uint32_t(e.Selector),
		selector_value:             C.uint32_t(e.SelectorValue),
		duration_subsec_nanos:      C.uint32_t(e.CalibrationNanos),
		duration_secs:              C.uint64_t(e.CalibrationSeconds),
	}
	buffer := make([]byte, 256)
	for {
		length := int(C.nox_error_message(&detail, (*C.uint8_t)(unsafe.Pointer(&buffer[0])), C.size_t(len(buffer))))
		if length <= len(buffer) {
			return "noiseox: " + string(buffer[:length])
		}
		buffer = make([]byte, length)
	}
}

// Unwrap makes every ConfigError match ErrInvalidConfig.
func (e *ConfigError) Unwrap() error {
	return ErrInvalidConfig
}

func boolByte(value bool) C.uint8_t {
	if value {
		return 1
	}
	return 0
}
