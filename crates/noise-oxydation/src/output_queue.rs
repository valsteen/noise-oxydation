//! Fixed-capacity FIFO of encoded output samples that are final but not yet emitted.
//!
//! The queue also enforces exact sample accounting: it counts the input samples received and the output samples
//! finalized, and drops finalized samples beyond the end of the input (analysis padding at drain).
//!
//! The packet timing contract bounds its content: after the samples finalized by packet `p` are queued it holds at
//! most `(160p − 128) − 160(p − 3) = 352` bytes before that packet's output is popped, and at most 320 bytes of
//! withheld output at drain. The capacity leaves headroom above both bounds.

use crate::{geometry::PACKET_SAMPLES, mulaw};

const CAPACITY: usize = 512;

#[derive(Debug, Clone)]
pub(crate) struct OutputQueue {
    bytes: [u8; CAPACITY],
    head: usize,
    len: usize,
    samples_received: u64,
    samples_finalized: u64,
}

impl OutputQueue {
    pub(crate) fn new() -> Self {
        Self { bytes: [0; CAPACITY], head: 0, len: 0, samples_received: 0, samples_finalized: 0 }
    }

    /// Records that one input packet was received.
    pub(crate) fn receive_packet(&mut self) {
        self.samples_received += PACKET_SAMPLES as u64;
    }

    /// Encodes and queues final output samples in order, dropping those beyond the end of the input received so far.
    pub(crate) fn finalize(&mut self, samples: &[f32]) {
        for &sample in samples {
            if self.samples_finalized < self.samples_received {
                self.push(mulaw::encode_sample(sample));
            }
            self.samples_finalized += 1;
        }
    }

    /// Moves the oldest packet's worth of bytes into `packet`. Returns `false` and leaves `packet` untouched when fewer
    /// than one packet is queued.
    pub(crate) fn pop_packet(&mut self, packet: &mut [u8; PACKET_SAMPLES]) -> bool {
        if self.len < PACKET_SAMPLES {
            return false;
        }
        for (offset, byte) in packet.iter_mut().enumerate() {
            *byte = self.bytes[(self.head + offset) % CAPACITY];
        }
        self.head = (self.head + PACKET_SAMPLES) % CAPACITY;
        self.len -= PACKET_SAMPLES;
        true
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(crate) fn reset(&mut self) {
        self.head = 0;
        self.len = 0;
        self.samples_received = 0;
        self.samples_finalized = 0;
    }

    fn push(&mut self, byte: u8) {
        debug_assert!(self.len < CAPACITY, "the timing contract bounds the queue below its capacity");
        self.bytes[(self.head + self.len) % CAPACITY] = byte;
        self.len += 1;
    }
}
