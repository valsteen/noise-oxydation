#ifndef NOISE_OXYDATION_H
#define NOISE_OXYDATION_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct NoCall NoCall;

enum {
    NO_OK = 0,
    NO_NULL = 1,
    NO_INPUT_LENGTH = 2,
    NO_OUTPUT_CAPACITY = 3,
    NO_ESTIMATOR = 4,
    NO_DURATION = 5,
    NO_DSP_INIT = 6,
    NO_FINISHED = 7,
    NO_PANIC = 8,
    NO_MODE = 10
};

enum { NO_SPP_MMSE = 0, NO_MCRA = 1, NO_MINIMUM = 2 };
enum { NO_CONSERVATIVE = 0, NO_EXPERIMENTAL_LOW_DELAY = 1 };
enum { NO_PACKET_BYTES = 160, NO_OUTPUT_BYTES = 320 };

typedef struct NoResult {
    uint32_t status;
    uint32_t count;
    uint32_t final_valid;
    uint64_t samples;
    uint64_t minimum;
} NoResult;

/* The caller owns valid storage, keeps input/output distinct from the handle,
   and serializes all uses of one raw handle. No buffer pointer is retained.
   Process and finish require a non-null 320-byte output buffer even when the
   result contains fewer bytes. NoResult.count is 0..2; each returned packet
   is 160 bytes except the last, whose valid length is final_valid. */
NoResult no_create(uint64_t learning_duration_ns, uint32_t estimator, NoCall **out);
NoResult no_create_with_mode(uint64_t learning_duration_ns, uint32_t estimator,
                             uint32_t mode, NoCall **out);
NoResult no_process(NoCall *call, const uint8_t *input, size_t input_len,
                    uint8_t *output, size_t output_capacity);
NoResult no_finish(NoCall *call, uint8_t *output, size_t output_capacity);
NoResult no_reset(NoCall *call);
NoResult no_destroy(NoCall *call);

#ifdef __cplusplus
}
#endif

#endif
