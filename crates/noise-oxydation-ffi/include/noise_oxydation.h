#ifndef NOISE_OXYDATION_H
#define NOISE_OXYDATION_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct NoiseOxydationProcessor NoiseOxydationProcessor;

typedef enum NoiseOxydationStatus {
    NOISE_OXYDATION_OK = 0,
    NOISE_OXYDATION_INVALID_ARGUMENT = 1,
    NOISE_OXYDATION_INVALID_PACKET_LENGTH = 2,
    NOISE_OXYDATION_OUTPUT_TOO_SMALL = 3,
    NOISE_OXYDATION_STREAM_DRAINED = 4,
    NOISE_OXYDATION_STREAM_TOO_LONG = 5,
    NOISE_OXYDATION_INTERNAL_ERROR = 6
} NoiseOxydationStatus;

/* Create one independent stream processor; use a handle from one thread. */
NoiseOxydationProcessor *noise_oxydation_processor_new(void);
/* Release a live handle once. Passing null is a no-op. */
void noise_oxydation_processor_free(NoiseOxydationProcessor *processor);
/* Push exactly 160 bytes. On OUTPUT_TOO_SMALL, written receives the required
 * capacity and stream state is unchanged. Packet and output may overlap;
 * written and the handle must not overlap either buffer. */
NoiseOxydationStatus noise_oxydation_processor_push(
    NoiseOxydationProcessor *processor,
    const uint8_t *packet,
    size_t packet_length,
    uint8_t *output,
    size_t output_capacity,
    size_t *written);
/* Drain the tail once. On OUTPUT_TOO_SMALL, written receives the required
 * capacity and stream state is unchanged. The handle, output, and written
 * regions must not overlap. */
NoiseOxydationStatus noise_oxydation_processor_drain(
    NoiseOxydationProcessor *processor,
    uint8_t *output,
    size_t output_capacity,
    size_t *written);
/* Discard pending output and reset one live processor. */
NoiseOxydationStatus noise_oxydation_processor_reset(NoiseOxydationProcessor *processor);

#ifdef __cplusplus
}
#endif

#endif
