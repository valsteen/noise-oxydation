package noiseox

// #include "noise_oxydation.h"
import "C"

import "time"

// NoiseEstimator selects the noise power estimator of a call.
type NoiseEstimator uint32

// Noise estimators.
const (
	// NoiseEstimatorSppMmse is speech-presence-probability MMSE estimation, configured by Config.SppMmse (default).
	NoiseEstimatorSppMmse NoiseEstimator = C.NOX_NOISE_ESTIMATOR_SPP_MMSE
	// NoiseEstimatorMcra is minimum-controlled recursive averaging, configured by Config.Mcra.
	NoiseEstimatorMcra NoiseEstimator = C.NOX_NOISE_ESTIMATOR_MCRA
	// NoiseEstimatorMinimum is the exact sliding-window minimum, configured by Config.Minimum.
	NoiseEstimatorMinimum NoiseEstimator = C.NOX_NOISE_ESTIMATOR_MINIMUM
)

// Interference selects the foreground interference suppression applied after Log-MMSE.
type Interference uint32

// Interference settings.
const (
	// InterferenceTonalTransient is tonal transient suppression, configured by Config.TonalTransient (default).
	InterferenceTonalTransient Interference = C.NOX_INTERFERENCE_TONAL_TRANSIENT
	// InterferenceDisabled synthesizes the Log-MMSE output directly.
	InterferenceDisabled Interference = C.NOX_INTERFERENCE_DISABLED
)

// Config configures one call. It mirrors the Rust CallConfig field by field; units, ranges and defaults are those of
// the Rust fields, documented in docs/algorithms.md of the repository. Start from DefaultConfig and change what you
// need: the zero Config is invalid.
//
// Every estimator's parameters are present; NoiseEstimator and Interference select which are used, and only the
// selected ones are validated.
type Config struct {
	// CalibrationDuration is the quiet intro used to learn the noise; zero disables calibration. Default 5 s.
	CalibrationDuration time.Duration
	HighPass            HighPassConfig
	NoiseEstimator      NoiseEstimator
	SppMmse             SppMmseConfig
	Mcra                McraConfig
	Minimum             MinimumConfig
	DecisionDirected    DecisionDirectedConfig
	LogMmse             LogMmseConfig
	Interference        Interference
	TonalTransient      TonalTransientConfig
}

// HighPassConfig mirrors the Rust HighPassConfig.
type HighPassConfig struct {
	CutoffHz float32
}

// SppMmseConfig mirrors the Rust SppMmseConfig.
type SppMmseConfig struct {
	NoiseSmoothing       float32
	SppSmoothing         float32
	SpeechPrior          float32
	FixedPriorSnr        float32
	StagnationThreshold  float32
	MaxSpeechProbability float32
	Floor                float32
}

// McraConfig mirrors the Rust McraConfig.
type McraConfig struct {
	Smoothing       float32
	SpeechSmoothing float32
	NoiseSmoothing  float32
	RatioThreshold  float32
	WindowFrames    uint16
	Floor           float32
}

// MinimumConfig mirrors the Rust MinimumConfig.
type MinimumConfig struct {
	Smoothing    float32
	WindowFrames uint16
	Floor        float32
}

// DecisionDirectedConfig mirrors the Rust DecisionDirectedConfig.
type DecisionDirectedConfig struct {
	Alpha float32
	Floor float32
}

// LogMmseConfig mirrors the Rust LogMmseConfig.
type LogMmseConfig struct {
	MinGain             float32
	MaxGain             float32
	Floor               float32
	NoiseOverestimation float32
}

// TonalTransientConfig mirrors the Rust TonalTransientConfig.
type TonalTransientConfig struct {
	LocalRadius              uint16
	TonalStartDb             float32
	TonalFullDb              float32
	FluxStartDb              float32
	FluxFullDb               float32
	MovementSearchRadius     uint16
	MovementStartBins        uint16
	MovementFullBins         uint16
	MinFrequencyBin          uint16
	MovementMinRelativePower float32
	HarmonicToleranceBins    uint16
	HarmonicRelativePower    float32
	Strength                 float32
	MinGain                  float32
	Attack                   float32
	Release                  float32
	SpreadRadius             uint16
	Floor                    float32
}

// DefaultConfig returns the Rust defaults: CallConfig::default() with SPP-MMSE and tonal transient suppression, and
// the Rust defaults of the MCRA and minimum estimator parameters.
func DefaultConfig() Config {
	var native C.nox_config
	if status := C.nox_config_default(&native); status != C.NOX_STATUS_OK {
		panic("noiseox: nox_config_default failed: " + statusError(status).Error())
	}
	return configFromC(&native)
}

func (c *Config) toC() (C.nox_config, error) {
	if c.CalibrationDuration < 0 {
		return C.nox_config{}, &ConfigError{Kind: KindNegativeCalibrationDuration, CalibrationDuration: c.CalibrationDuration}
	}
	return C.nox_config{
		calibration_duration_ns: C.uint64_t(c.CalibrationDuration),
		high_pass:               C.nox_high_pass_config{cutoff_hz: C.float(c.HighPass.CutoffHz)},
		noise_estimator:         C.uint32_t(c.NoiseEstimator),
		spp_mmse: C.nox_spp_mmse_config{
			noise_smoothing:        C.float(c.SppMmse.NoiseSmoothing),
			spp_smoothing:          C.float(c.SppMmse.SppSmoothing),
			speech_prior:           C.float(c.SppMmse.SpeechPrior),
			fixed_prior_snr:        C.float(c.SppMmse.FixedPriorSnr),
			stagnation_threshold:   C.float(c.SppMmse.StagnationThreshold),
			max_speech_probability: C.float(c.SppMmse.MaxSpeechProbability),
			floor:                  C.float(c.SppMmse.Floor),
		},
		mcra: C.nox_mcra_config{
			smoothing:        C.float(c.Mcra.Smoothing),
			speech_smoothing: C.float(c.Mcra.SpeechSmoothing),
			noise_smoothing:  C.float(c.Mcra.NoiseSmoothing),
			ratio_threshold:  C.float(c.Mcra.RatioThreshold),
			window_frames:    C.uint16_t(c.Mcra.WindowFrames),
			floor:            C.float(c.Mcra.Floor),
		},
		minimum: C.nox_minimum_config{
			smoothing:     C.float(c.Minimum.Smoothing),
			window_frames: C.uint16_t(c.Minimum.WindowFrames),
			floor:         C.float(c.Minimum.Floor),
		},
		decision_directed: C.nox_decision_directed_config{
			alpha: C.float(c.DecisionDirected.Alpha),
			floor: C.float(c.DecisionDirected.Floor),
		},
		log_mmse: C.nox_log_mmse_config{
			min_gain:             C.float(c.LogMmse.MinGain),
			max_gain:             C.float(c.LogMmse.MaxGain),
			floor:                C.float(c.LogMmse.Floor),
			noise_overestimation: C.float(c.LogMmse.NoiseOverestimation),
		},
		interference: C.uint32_t(c.Interference),
		tonal_transient: C.nox_tonal_transient_config{
			local_radius:                C.uint16_t(c.TonalTransient.LocalRadius),
			tonal_start_db:              C.float(c.TonalTransient.TonalStartDb),
			tonal_full_db:               C.float(c.TonalTransient.TonalFullDb),
			flux_start_db:               C.float(c.TonalTransient.FluxStartDb),
			flux_full_db:                C.float(c.TonalTransient.FluxFullDb),
			movement_search_radius:      C.uint16_t(c.TonalTransient.MovementSearchRadius),
			movement_start_bins:         C.uint16_t(c.TonalTransient.MovementStartBins),
			movement_full_bins:          C.uint16_t(c.TonalTransient.MovementFullBins),
			min_frequency_bin:           C.uint16_t(c.TonalTransient.MinFrequencyBin),
			movement_min_relative_power: C.float(c.TonalTransient.MovementMinRelativePower),
			harmonic_tolerance_bins:     C.uint16_t(c.TonalTransient.HarmonicToleranceBins),
			harmonic_relative_power:     C.float(c.TonalTransient.HarmonicRelativePower),
			strength:                    C.float(c.TonalTransient.Strength),
			min_gain:                    C.float(c.TonalTransient.MinGain),
			attack:                      C.float(c.TonalTransient.Attack),
			release:                     C.float(c.TonalTransient.Release),
			spread_radius:               C.uint16_t(c.TonalTransient.SpreadRadius),
			floor:                       C.float(c.TonalTransient.Floor),
		},
	}, nil
}

func configFromC(n *C.nox_config) Config {
	return Config{
		CalibrationDuration: time.Duration(n.calibration_duration_ns),
		HighPass:            HighPassConfig{CutoffHz: float32(n.high_pass.cutoff_hz)},
		NoiseEstimator:      NoiseEstimator(n.noise_estimator),
		SppMmse: SppMmseConfig{
			NoiseSmoothing:       float32(n.spp_mmse.noise_smoothing),
			SppSmoothing:         float32(n.spp_mmse.spp_smoothing),
			SpeechPrior:          float32(n.spp_mmse.speech_prior),
			FixedPriorSnr:        float32(n.spp_mmse.fixed_prior_snr),
			StagnationThreshold:  float32(n.spp_mmse.stagnation_threshold),
			MaxSpeechProbability: float32(n.spp_mmse.max_speech_probability),
			Floor:                float32(n.spp_mmse.floor),
		},
		Mcra: McraConfig{
			Smoothing:       float32(n.mcra.smoothing),
			SpeechSmoothing: float32(n.mcra.speech_smoothing),
			NoiseSmoothing:  float32(n.mcra.noise_smoothing),
			RatioThreshold:  float32(n.mcra.ratio_threshold),
			WindowFrames:    uint16(n.mcra.window_frames),
			Floor:           float32(n.mcra.floor),
		},
		Minimum: MinimumConfig{
			Smoothing:    float32(n.minimum.smoothing),
			WindowFrames: uint16(n.minimum.window_frames),
			Floor:        float32(n.minimum.floor),
		},
		DecisionDirected: DecisionDirectedConfig{
			Alpha: float32(n.decision_directed.alpha),
			Floor: float32(n.decision_directed.floor),
		},
		LogMmse: LogMmseConfig{
			MinGain:             float32(n.log_mmse.min_gain),
			MaxGain:             float32(n.log_mmse.max_gain),
			Floor:               float32(n.log_mmse.floor),
			NoiseOverestimation: float32(n.log_mmse.noise_overestimation),
		},
		Interference: Interference(n.interference),
		TonalTransient: TonalTransientConfig{
			LocalRadius:              uint16(n.tonal_transient.local_radius),
			TonalStartDb:             float32(n.tonal_transient.tonal_start_db),
			TonalFullDb:              float32(n.tonal_transient.tonal_full_db),
			FluxStartDb:              float32(n.tonal_transient.flux_start_db),
			FluxFullDb:               float32(n.tonal_transient.flux_full_db),
			MovementSearchRadius:     uint16(n.tonal_transient.movement_search_radius),
			MovementStartBins:        uint16(n.tonal_transient.movement_start_bins),
			MovementFullBins:         uint16(n.tonal_transient.movement_full_bins),
			MinFrequencyBin:          uint16(n.tonal_transient.min_frequency_bin),
			MovementMinRelativePower: float32(n.tonal_transient.movement_min_relative_power),
			HarmonicToleranceBins:    uint16(n.tonal_transient.harmonic_tolerance_bins),
			HarmonicRelativePower:    float32(n.tonal_transient.harmonic_relative_power),
			Strength:                 float32(n.tonal_transient.strength),
			MinGain:                  float32(n.tonal_transient.min_gain),
			Attack:                   float32(n.tonal_transient.attack),
			Release:                  float32(n.tonal_transient.release),
			SpreadRadius:             uint16(n.tonal_transient.spread_radius),
			Floor:                    float32(n.tonal_transient.floor),
		},
	}
}
