//! A safe test-side wrapper around one handle, and the synthetic input shared with the Go tests.

use std::ptr;

use noise_oxydation_capi::{
    NOX_DELAY_PACKETS, NOX_PACKET_BYTES, NOX_STATUS_OK, NoxCall, NoxConfig, NoxError, nox_call_drain, nox_call_free,
    nox_call_new, nox_call_phase, nox_call_process, nox_call_reset, nox_config_default, nox_error_message,
};

pub type Packet = [u8; NOX_PACKET_BYTES];

/// The default C configuration.
pub fn default_config() -> NoxConfig {
    let mut config = std::mem::MaybeUninit::<NoxConfig>::uninit();
    // SAFETY: `config` is writable, aligned storage for one `NoxConfig`.
    let status = unsafe { nox_config_default(config.as_mut_ptr()) };
    assert_eq!(status, NOX_STATUS_OK);
    // SAFETY: `nox_config_default` returned OK, so it wrote the whole value.
    unsafe { config.assume_init() }
}

/// The detail `nox_call_new` reports for `config`, and the handle it returned (null on failure).
pub fn create(config: &NoxConfig) -> (u32, *mut NoxCall, NoxError) {
    let mut call = ptr::dangling_mut::<NoxCall>();
    let mut error = std::mem::MaybeUninit::<NoxError>::uninit();
    // SAFETY: every pointer refers to live, aligned storage of the right type.
    let status = unsafe { nox_call_new(config, &raw mut call, error.as_mut_ptr()) };
    // SAFETY: `error` is not null, so `nox_call_new` always writes it.
    (status, call, unsafe { error.assume_init() })
}

/// The message `nox_error_message` writes for `error`.
pub fn message(error: &NoxError) -> String {
    let mut buffer = vec![0_u8; 8];
    // SAFETY: `error` is a live `NoxError`; `buffer` holds `buffer.len()` writable bytes.
    let length = unsafe { nox_error_message(error, buffer.as_mut_ptr(), buffer.len()) };
    if length > buffer.len() {
        buffer.resize(length, 0);
        // SAFETY: as above, with the larger buffer.
        let again = unsafe { nox_error_message(error, buffer.as_mut_ptr(), buffer.len()) };
        assert_eq!(again, length);
    }
    buffer.truncate(length);
    String::from_utf8(buffer).expect("messages are UTF-8")
}

/// One live handle, freed on drop.
pub struct Handle(*mut NoxCall);

impl Handle {
    pub fn new(config: &NoxConfig) -> Self {
        let (status, call, error) = create(config);
        assert_eq!(status, NOX_STATUS_OK, "{}", message(&error));
        assert!(!call.is_null());
        Self(call)
    }

    /// Status and outcome of processing `input`.
    pub fn process(&mut self, input: &Packet, output: &mut Packet) -> (u32, u32) {
        let mut outcome = 0;
        // SAFETY: the handle is live and used by this thread only; the packets and `outcome` are live storage.
        let status = unsafe { nox_call_process(self.0, input.as_ptr(), output.as_mut_ptr(), &raw mut outcome) };
        (status, outcome)
    }

    /// Status and packet count of draining into `output`.
    pub fn drain(&mut self, output: &mut [Packet; NOX_DELAY_PACKETS]) -> (u32, usize) {
        let mut written = usize::MAX;
        // SAFETY: the handle is live and used by this thread only; `output` holds 320 writable bytes.
        let status = unsafe { nox_call_drain(self.0, output.as_mut_ptr().cast(), &raw mut written) };
        (status, written)
    }

    pub fn reset(&mut self) -> u32 {
        // SAFETY: the handle is live and used by this thread only.
        unsafe { nox_call_reset(self.0) }
    }

    /// Status and phase.
    pub fn phase(&self) -> (u32, u32) {
        let mut phase = 0;
        // SAFETY: the handle is live and not mutated during the call; `phase` is live storage.
        let status = unsafe { nox_call_phase(self.0, &raw mut phase) };
        (status, phase)
    }

    pub fn raw(&self) -> *mut NoxCall {
        self.0
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from `nox_call_new` and is freed exactly once, here.
        unsafe { nox_call_free(self.0) };
    }
}

/// Deterministic xorshift64* generator. The Go tests implement the same sequence.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        u32::try_from(self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32).expect("32 bits")
    }
}

/// Synthetic call audio: white noise at a low level for the first second (the quiet intro), then alternating
/// half-second bursts at two louder levels. Each sample has a random sign and a uniformly random μ-law magnitude index
/// up to the packet's level. The Go tests generate the same bytes.
pub fn synthetic_packets(count: usize, seed: u64) -> Vec<Packet> {
    let mut rng = Rng::new(seed);
    (0..count)
        .map(|index| {
            let level: u32 = if index < 50 {
                30
            } else if (index / 25) % 2 == 0 {
                60
            } else {
                110
            };
            let mut packet = [0; NOX_PACKET_BYTES];
            for byte in &mut packet {
                let draw = rng.next_u32();
                let magnitude = u8::try_from(draw % (level + 1)).expect("levels fit a byte");
                let sign = if draw >> 31 == 1 { 0x80 } else { 0 };
                *byte = !(sign | magnitude);
            }
            packet
        })
        .collect()
}

/// 64-bit FNV-1a, the digest the Go tests compare with.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xCBF2_9CE4_8422_2325, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01B3))
}
