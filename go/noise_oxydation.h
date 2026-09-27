/*
 * C ABI of the noise-oxydation per-call telephony enhancer (crate crates/bindings/noise-oxydation-capi).
 *
 * This header is the single C declaration of the ABI. The Rust crate pins the same sizes, offsets and constants, and
 * its unit tests compare them with this file; the _Static_asserts at the end make the C compiler check the layout.
 *
 * One handle serves one call and is used by one thread at a time. nox_call_new is the only function that
 * allocates; nox_call_process, nox_call_drain, nox_call_reset and nox_call_phase write into caller-owned storage.
 * No panic unwinds across the ABI: every function reports it as NOX_STATUS_PANIC and poisons the handle.
 * Supported on 64-bit targets (macOS arm64 and Linux x86_64 are verified).
 */
#ifndef NOISE_OXYDATION_H
#define NOISE_OXYDATION_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

enum {
    /* Bytes of one 20 ms G.711 mu-law packet. */
    NOX_PACKET_BYTES = 160,
    /* Packets withheld by the two-packet delay; nox_call_drain writes at most this many. */
    NOX_DELAY_PACKETS = 2,
};

/* Status of every operation. */
typedef uint32_t nox_status;
enum {
    NOX_STATUS_OK = 0,
    NOX_STATUS_NULL_ARGUMENT = 1,
    NOX_STATUS_INVALID_CONFIG = 2,
    NOX_STATUS_DRAINED = 3,
    NOX_STATUS_PANIC = 4,
};

/* nox_config.noise_estimator */
enum {
    NOX_NOISE_ESTIMATOR_SPP_MMSE = 1,
    NOX_NOISE_ESTIMATOR_MCRA = 2,
    NOX_NOISE_ESTIMATOR_MINIMUM = 3,
};

/* nox_config.interference */
enum {
    NOX_INTERFERENCE_TONAL_TRANSIENT = 1,
    NOX_INTERFERENCE_DISABLED = 2,
};

/* Result of nox_call_process. */
enum {
    NOX_OUTCOME_PRIMING = 1,
    NOX_OUTCOME_EMITTED = 2,
};

/* Result of nox_call_phase. */
enum {
    NOX_PHASE_CALIBRATING = 1,
    NOX_PHASE_ENHANCING = 2,
    NOX_PHASE_DRAINED = 3,
};

/* nox_error.config_error: one value per Rust ConfigError variant, plus the C-side selector check. */
enum {
    NOX_CONFIG_ERROR_NONE = 0,
    /* field, value, constraint and constraint bounds */
    NOX_CONFIG_ERROR_OUT_OF_RANGE = 1,
    /* value = min_gain, second_value = max_gain */
    NOX_CONFIG_ERROR_MIN_GAIN_ABOVE_MAX_GAIN = 2,
    /* field, count_value, count_min, count_max */
    NOX_CONFIG_ERROR_COUNT_OUT_OF_RANGE = 3,
    /* field and value of the start threshold, second_field and second_value of the full threshold */
    NOX_CONFIG_ERROR_START_NOT_BELOW_FULL = 4,
    /* duration_secs and duration_subsec_nanos */
    NOX_CONFIG_ERROR_CALIBRATION_TOO_LONG = 5,
    /* selector and selector_value */
    NOX_CONFIG_ERROR_UNKNOWN_SELECTOR = 6,
    /* a ConfigError variant this version of the ABI does not describe */
    NOX_CONFIG_ERROR_OTHER = 7,
};

/* nox_error.selector */
enum {
    NOX_SELECTOR_NONE = 0,
    NOX_SELECTOR_NOISE_ESTIMATOR = 1,
    NOX_SELECTOR_INTERFERENCE = 2,
};

/* Configuration fields, in the order of the Rust ConfigField; nox_config_field_name returns their qualified names. */
enum {
    NOX_FIELD_NONE = 0,
    NOX_FIELD_HIGH_PASS_CUTOFF_HZ = 1,
    NOX_FIELD_SPP_MMSE_NOISE_SMOOTHING = 2,
    NOX_FIELD_SPP_MMSE_SPP_SMOOTHING = 3,
    NOX_FIELD_SPP_MMSE_SPEECH_PRIOR = 4,
    NOX_FIELD_SPP_MMSE_FIXED_PRIOR_SNR = 5,
    NOX_FIELD_SPP_MMSE_STAGNATION_THRESHOLD = 6,
    NOX_FIELD_SPP_MMSE_MAX_SPEECH_PROBABILITY = 7,
    NOX_FIELD_SPP_MMSE_FLOOR = 8,
    NOX_FIELD_DECISION_DIRECTED_ALPHA = 9,
    NOX_FIELD_DECISION_DIRECTED_FLOOR = 10,
    NOX_FIELD_LOG_MMSE_MIN_GAIN = 11,
    NOX_FIELD_LOG_MMSE_MAX_GAIN = 12,
    NOX_FIELD_LOG_MMSE_FLOOR = 13,
    NOX_FIELD_LOG_MMSE_NOISE_OVERESTIMATION = 14,
    NOX_FIELD_MCRA_SMOOTHING = 15,
    NOX_FIELD_MCRA_SPEECH_SMOOTHING = 16,
    NOX_FIELD_MCRA_NOISE_SMOOTHING = 17,
    NOX_FIELD_MCRA_RATIO_THRESHOLD = 18,
    NOX_FIELD_MCRA_WINDOW_FRAMES = 19,
    NOX_FIELD_MCRA_FLOOR = 20,
    NOX_FIELD_MINIMUM_SMOOTHING = 21,
    NOX_FIELD_MINIMUM_WINDOW_FRAMES = 22,
    NOX_FIELD_MINIMUM_FLOOR = 23,
    NOX_FIELD_TONAL_TRANSIENT_LOCAL_RADIUS = 24,
    NOX_FIELD_TONAL_TRANSIENT_TONAL_START_DB = 25,
    NOX_FIELD_TONAL_TRANSIENT_TONAL_FULL_DB = 26,
    NOX_FIELD_TONAL_TRANSIENT_FLUX_START_DB = 27,
    NOX_FIELD_TONAL_TRANSIENT_FLUX_FULL_DB = 28,
    NOX_FIELD_TONAL_TRANSIENT_MOVEMENT_SEARCH_RADIUS = 29,
    NOX_FIELD_TONAL_TRANSIENT_MOVEMENT_START_BINS = 30,
    NOX_FIELD_TONAL_TRANSIENT_MOVEMENT_FULL_BINS = 31,
    NOX_FIELD_TONAL_TRANSIENT_MIN_FREQUENCY_BIN = 32,
    NOX_FIELD_TONAL_TRANSIENT_MOVEMENT_MIN_RELATIVE_POWER = 33,
    NOX_FIELD_TONAL_TRANSIENT_HARMONIC_TOLERANCE_BINS = 34,
    NOX_FIELD_TONAL_TRANSIENT_HARMONIC_RELATIVE_POWER = 35,
    NOX_FIELD_TONAL_TRANSIENT_STRENGTH = 36,
    NOX_FIELD_TONAL_TRANSIENT_MIN_GAIN = 37,
    NOX_FIELD_TONAL_TRANSIENT_ATTACK = 38,
    NOX_FIELD_TONAL_TRANSIENT_RELEASE = 39,
    NOX_FIELD_TONAL_TRANSIENT_SPREAD_RADIUS = 40,
    NOX_FIELD_TONAL_TRANSIENT_FLOOR = 41,
    NOX_FIELD_OTHER = 999,
};

/* Ranges of numeric fields, in the order of the Rust Constraint; nox_constraint_description describes them. */
enum {
    NOX_CONSTRAINT_NONE = 0,
    NOX_CONSTRAINT_OPEN_UNIT = 1,
    NOX_CONSTRAINT_UNIT_EXCLUDING_ONE = 2,
    NOX_CONSTRAINT_UNIT_EXCLUDING_ZERO = 3,
    NOX_CONSTRAINT_CLOSED_UNIT = 4,
    NOX_CONSTRAINT_POSITIVE = 5,
    NOX_CONSTRAINT_NON_NEGATIVE = 6,
    NOX_CONSTRAINT_GREATER_THAN_ONE = 7,
    NOX_CONSTRAINT_BELOW_NYQUIST = 8,
    NOX_CONSTRAINT_OTHER = 999,
};

/* A static UTF-8 string owned by the library: len bytes at ptr, not NUL-terminated; ptr is never NULL. */
typedef struct nox_str {
    const uint8_t *ptr;
    size_t len;
} nox_str;

typedef struct nox_high_pass_config {
    float cutoff_hz;
} nox_high_pass_config;

typedef struct nox_spp_mmse_config {
    float noise_smoothing;
    float spp_smoothing;
    float speech_prior;
    float fixed_prior_snr;
    float stagnation_threshold;
    float max_speech_probability;
    float floor;
} nox_spp_mmse_config;

typedef struct nox_mcra_config {
    float smoothing;
    float speech_smoothing;
    float noise_smoothing;
    float ratio_threshold;
    uint16_t window_frames;
    float floor;
} nox_mcra_config;

typedef struct nox_minimum_config {
    float smoothing;
    uint16_t window_frames;
    float floor;
} nox_minimum_config;

typedef struct nox_decision_directed_config {
    float alpha;
    float floor;
} nox_decision_directed_config;

typedef struct nox_log_mmse_config {
    float min_gain;
    float max_gain;
    float floor;
    float noise_overestimation;
} nox_log_mmse_config;

typedef struct nox_tonal_transient_config {
    uint16_t local_radius;
    float tonal_start_db;
    float tonal_full_db;
    float flux_start_db;
    float flux_full_db;
    uint16_t movement_search_radius;
    uint16_t movement_start_bins;
    uint16_t movement_full_bins;
    uint16_t min_frequency_bin;
    float movement_min_relative_power;
    uint16_t harmonic_tolerance_bins;
    float harmonic_relative_power;
    float strength;
    float min_gain;
    float attack;
    float release;
    uint16_t spread_radius;
    float floor;
} nox_tonal_transient_config;

/*
 * The Rust CallConfig. Units, ranges and defaults are those of the Rust fields (docs/algorithms.md). All estimator
 * parameter blocks are present; noise_estimator and interference select which ones are used.
 */
typedef struct nox_config {
    uint64_t calibration_duration_ns;
    nox_high_pass_config high_pass;
    uint32_t noise_estimator;
    nox_spp_mmse_config spp_mmse;
    nox_mcra_config mcra;
    nox_minimum_config minimum;
    nox_decision_directed_config decision_directed;
    nox_log_mmse_config log_mmse;
    uint32_t interference;
    nox_tonal_transient_config tonal_transient;
} nox_config;

/* Detail of a failed operation. Fields not listed for config_error are zero. */
typedef struct nox_error {
    nox_status status;
    uint32_t config_error;
    uint32_t field;
    uint32_t second_field;
    uint32_t constraint;
    float value;
    float second_value;
    float constraint_lower;
    /* +infinity when the range has no upper bound (finite values are still required) */
    float constraint_upper;
    uint8_t constraint_lower_inclusive;
    uint8_t constraint_upper_inclusive;
    uint16_t count_value;
    uint16_t count_min;
    uint16_t count_max;
    uint32_t selector;
    uint32_t selector_value;
    uint32_t duration_subsec_nanos;
    uint64_t duration_secs;
} nox_error;

/* Opaque call handle. */
typedef struct nox_call nox_call;

/* Fills config with the Rust defaults. */
nox_status nox_config_default(nox_config *config);

/*
 * Validates config and creates a handle in *call (NULL on failure). error may be NULL; otherwise it receives the
 * detail of the outcome, including every datum of a rejected configuration.
 */
nox_status nox_call_new(const nox_config *config, nox_call **call, nox_error *error);

/*
 * Processes one NOX_PACKET_BYTES input packet. Writes NOX_OUTCOME_PRIMING (output untouched) or NOX_OUTCOME_EMITTED
 * (output holds the enhanced packet received two packets earlier) to *outcome. input may overlap output.
 * Returns NOX_STATUS_DRAINED after nox_call_drain until nox_call_reset.
 */
nox_status nox_call_process(nox_call *call, const uint8_t *input, uint8_t *output, uint32_t *outcome);

/* Ends the call: writes the withheld packets to output (NOX_DELAY_PACKETS * NOX_PACKET_BYTES bytes) and their count. */
nox_status nox_call_drain(nox_call *call, uint8_t *output, size_t *written);

/* Starts a new call on the handle. */
nox_status nox_call_reset(nox_call *call);

/* Writes the call phase (NOX_PHASE_*). */
nox_status nox_call_phase(const nox_call *call, uint32_t *phase);

/* Releases a handle; NULL is ignored. The handle must not be used again. */
void nox_call_free(nox_call *call);

/* Static message of a status. */
nox_str nox_status_message(nox_status status);

/* Qualified name of a NOX_FIELD_* identifier, for example "spp_mmse.speech_prior"; empty when unknown. */
nox_str nox_config_field_name(uint32_t field);

/* Description of a NOX_CONSTRAINT_* identifier, for example "in (0, 1)"; empty when unknown. */
nox_str nox_constraint_description(uint32_t constraint);

/*
 * Writes the message of an error detail (UTF-8, no NUL, at most capacity bytes) and returns its full length; call
 * again with a larger buffer when the result exceeds capacity. Returns 0 when error is NULL.
 */
size_t nox_error_message(const nox_error *error, uint8_t *buffer, size_t capacity);

_Static_assert(sizeof(void *) == 8, "the ABI is defined for 64-bit targets");
_Static_assert(sizeof(nox_str) == 16, "nox_str size");
_Static_assert(offsetof(nox_str, ptr) == 0, "nox_str.ptr offset");
_Static_assert(offsetof(nox_str, len) == 8, "nox_str.len offset");
_Static_assert(sizeof(nox_high_pass_config) == 4, "nox_high_pass_config size");
_Static_assert(offsetof(nox_high_pass_config, cutoff_hz) == 0, "nox_high_pass_config.cutoff_hz offset");
_Static_assert(sizeof(nox_spp_mmse_config) == 28, "nox_spp_mmse_config size");
_Static_assert(offsetof(nox_spp_mmse_config, noise_smoothing) == 0, "nox_spp_mmse_config.noise_smoothing offset");
_Static_assert(offsetof(nox_spp_mmse_config, spp_smoothing) == 4, "nox_spp_mmse_config.spp_smoothing offset");
_Static_assert(offsetof(nox_spp_mmse_config, speech_prior) == 8, "nox_spp_mmse_config.speech_prior offset");
_Static_assert(offsetof(nox_spp_mmse_config, fixed_prior_snr) == 12, "nox_spp_mmse_config.fixed_prior_snr offset");
_Static_assert(offsetof(nox_spp_mmse_config, stagnation_threshold) == 16, "nox_spp_mmse_config.stagnation_threshold offset");
_Static_assert(offsetof(nox_spp_mmse_config, max_speech_probability) == 20, "nox_spp_mmse_config.max_speech_probability offset");
_Static_assert(offsetof(nox_spp_mmse_config, floor) == 24, "nox_spp_mmse_config.floor offset");
_Static_assert(sizeof(nox_mcra_config) == 24, "nox_mcra_config size");
_Static_assert(offsetof(nox_mcra_config, smoothing) == 0, "nox_mcra_config.smoothing offset");
_Static_assert(offsetof(nox_mcra_config, speech_smoothing) == 4, "nox_mcra_config.speech_smoothing offset");
_Static_assert(offsetof(nox_mcra_config, noise_smoothing) == 8, "nox_mcra_config.noise_smoothing offset");
_Static_assert(offsetof(nox_mcra_config, ratio_threshold) == 12, "nox_mcra_config.ratio_threshold offset");
_Static_assert(offsetof(nox_mcra_config, window_frames) == 16, "nox_mcra_config.window_frames offset");
_Static_assert(offsetof(nox_mcra_config, floor) == 20, "nox_mcra_config.floor offset");
_Static_assert(sizeof(nox_minimum_config) == 12, "nox_minimum_config size");
_Static_assert(offsetof(nox_minimum_config, smoothing) == 0, "nox_minimum_config.smoothing offset");
_Static_assert(offsetof(nox_minimum_config, window_frames) == 4, "nox_minimum_config.window_frames offset");
_Static_assert(offsetof(nox_minimum_config, floor) == 8, "nox_minimum_config.floor offset");
_Static_assert(sizeof(nox_decision_directed_config) == 8, "nox_decision_directed_config size");
_Static_assert(offsetof(nox_decision_directed_config, alpha) == 0, "nox_decision_directed_config.alpha offset");
_Static_assert(offsetof(nox_decision_directed_config, floor) == 4, "nox_decision_directed_config.floor offset");
_Static_assert(sizeof(nox_log_mmse_config) == 16, "nox_log_mmse_config size");
_Static_assert(offsetof(nox_log_mmse_config, min_gain) == 0, "nox_log_mmse_config.min_gain offset");
_Static_assert(offsetof(nox_log_mmse_config, max_gain) == 4, "nox_log_mmse_config.max_gain offset");
_Static_assert(offsetof(nox_log_mmse_config, floor) == 8, "nox_log_mmse_config.floor offset");
_Static_assert(offsetof(nox_log_mmse_config, noise_overestimation) == 12, "nox_log_mmse_config.noise_overestimation offset");
_Static_assert(sizeof(nox_tonal_transient_config) == 64, "nox_tonal_transient_config size");
_Static_assert(offsetof(nox_tonal_transient_config, local_radius) == 0, "nox_tonal_transient_config.local_radius offset");
_Static_assert(offsetof(nox_tonal_transient_config, tonal_start_db) == 4, "nox_tonal_transient_config.tonal_start_db offset");
_Static_assert(offsetof(nox_tonal_transient_config, tonal_full_db) == 8, "nox_tonal_transient_config.tonal_full_db offset");
_Static_assert(offsetof(nox_tonal_transient_config, flux_start_db) == 12, "nox_tonal_transient_config.flux_start_db offset");
_Static_assert(offsetof(nox_tonal_transient_config, flux_full_db) == 16, "nox_tonal_transient_config.flux_full_db offset");
_Static_assert(offsetof(nox_tonal_transient_config, movement_search_radius) == 20, "nox_tonal_transient_config.movement_search_radius offset");
_Static_assert(offsetof(nox_tonal_transient_config, movement_start_bins) == 22, "nox_tonal_transient_config.movement_start_bins offset");
_Static_assert(offsetof(nox_tonal_transient_config, movement_full_bins) == 24, "nox_tonal_transient_config.movement_full_bins offset");
_Static_assert(offsetof(nox_tonal_transient_config, min_frequency_bin) == 26, "nox_tonal_transient_config.min_frequency_bin offset");
_Static_assert(offsetof(nox_tonal_transient_config, movement_min_relative_power) == 28, "nox_tonal_transient_config.movement_min_relative_power offset");
_Static_assert(offsetof(nox_tonal_transient_config, harmonic_tolerance_bins) == 32, "nox_tonal_transient_config.harmonic_tolerance_bins offset");
_Static_assert(offsetof(nox_tonal_transient_config, harmonic_relative_power) == 36, "nox_tonal_transient_config.harmonic_relative_power offset");
_Static_assert(offsetof(nox_tonal_transient_config, strength) == 40, "nox_tonal_transient_config.strength offset");
_Static_assert(offsetof(nox_tonal_transient_config, min_gain) == 44, "nox_tonal_transient_config.min_gain offset");
_Static_assert(offsetof(nox_tonal_transient_config, attack) == 48, "nox_tonal_transient_config.attack offset");
_Static_assert(offsetof(nox_tonal_transient_config, release) == 52, "nox_tonal_transient_config.release offset");
_Static_assert(offsetof(nox_tonal_transient_config, spread_radius) == 56, "nox_tonal_transient_config.spread_radius offset");
_Static_assert(offsetof(nox_tonal_transient_config, floor) == 60, "nox_tonal_transient_config.floor offset");
_Static_assert(sizeof(nox_config) == 176, "nox_config size");
_Static_assert(offsetof(nox_config, calibration_duration_ns) == 0, "nox_config.calibration_duration_ns offset");
_Static_assert(offsetof(nox_config, high_pass) == 8, "nox_config.high_pass offset");
_Static_assert(offsetof(nox_config, noise_estimator) == 12, "nox_config.noise_estimator offset");
_Static_assert(offsetof(nox_config, spp_mmse) == 16, "nox_config.spp_mmse offset");
_Static_assert(offsetof(nox_config, mcra) == 44, "nox_config.mcra offset");
_Static_assert(offsetof(nox_config, minimum) == 68, "nox_config.minimum offset");
_Static_assert(offsetof(nox_config, decision_directed) == 80, "nox_config.decision_directed offset");
_Static_assert(offsetof(nox_config, log_mmse) == 88, "nox_config.log_mmse offset");
_Static_assert(offsetof(nox_config, interference) == 104, "nox_config.interference offset");
_Static_assert(offsetof(nox_config, tonal_transient) == 108, "nox_config.tonal_transient offset");
_Static_assert(sizeof(nox_error) == 64, "nox_error size");
_Static_assert(offsetof(nox_error, status) == 0, "nox_error.status offset");
_Static_assert(offsetof(nox_error, config_error) == 4, "nox_error.config_error offset");
_Static_assert(offsetof(nox_error, field) == 8, "nox_error.field offset");
_Static_assert(offsetof(nox_error, second_field) == 12, "nox_error.second_field offset");
_Static_assert(offsetof(nox_error, constraint) == 16, "nox_error.constraint offset");
_Static_assert(offsetof(nox_error, value) == 20, "nox_error.value offset");
_Static_assert(offsetof(nox_error, second_value) == 24, "nox_error.second_value offset");
_Static_assert(offsetof(nox_error, constraint_lower) == 28, "nox_error.constraint_lower offset");
_Static_assert(offsetof(nox_error, constraint_upper) == 32, "nox_error.constraint_upper offset");
_Static_assert(offsetof(nox_error, constraint_lower_inclusive) == 36, "nox_error.constraint_lower_inclusive offset");
_Static_assert(offsetof(nox_error, constraint_upper_inclusive) == 37, "nox_error.constraint_upper_inclusive offset");
_Static_assert(offsetof(nox_error, count_value) == 38, "nox_error.count_value offset");
_Static_assert(offsetof(nox_error, count_min) == 40, "nox_error.count_min offset");
_Static_assert(offsetof(nox_error, count_max) == 42, "nox_error.count_max offset");
_Static_assert(offsetof(nox_error, selector) == 44, "nox_error.selector offset");
_Static_assert(offsetof(nox_error, selector_value) == 48, "nox_error.selector_value offset");
_Static_assert(offsetof(nox_error, duration_subsec_nanos) == 52, "nox_error.duration_subsec_nanos offset");
_Static_assert(offsetof(nox_error, duration_secs) == 56, "nox_error.duration_secs offset");

#ifdef __cplusplus
}
#endif

#endif /* NOISE_OXYDATION_H */
