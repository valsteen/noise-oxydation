#[cfg(feature = "performance-analysis")]
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Instant,
};
use std::{
    env, fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use noise_oxydation_core::{MAX_DRAIN_OUTPUT_BYTES, MAX_PACKET_OUTPUT_BYTES, PACKET_BYTES, Processor, SAMPLE_RATE};

const SOURCE_SAMPLE_COUNT: usize = 133_240;
const CALIBRATION_SAMPLES: usize = 40_000;
const XORSHIFT32_SEED: u32 = 0x4e4f_4953;
const PCM16_MAX: f64 = 32_767.0;
const SPEECH_RATE: u32 = 8_000;

#[cfg(feature = "performance-analysis")]
const PACKET_CADENCE_NANOS: u128 = 20_000_000;

#[cfg(feature = "performance-analysis")]
static TRACK_ALLOCATIONS: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "performance-analysis")]
static ALLOCATION_COUNT: AtomicUsize = AtomicUsize::new(0);

#[cfg(feature = "performance-analysis")]
struct CountingAllocator;

#[cfg(feature = "performance-analysis")]
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[cfg(feature = "performance-analysis")]
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: the original layout is passed unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: the original layout is passed unchanged to the system allocator.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the pointer and layout are returned unchanged to the system allocator.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        // SAFETY: the pointer, layout, and new size are passed unchanged to the system allocator.
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[cfg(feature = "performance-analysis")]
fn count_allocation() {
    if TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
        ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

struct Options {
    input: PathBuf,
    quiet: bool,
    #[cfg(feature = "performance-analysis")]
    profile: bool,
}

struct GeneratedAudio {
    clean_reference: Vec<i16>,
    noisy_input: Vec<i16>,
    noise_rms_scale: f64,
    shared_gain: f64,
}

struct Metrics {
    snr_db: f64,
    si_sdr_db: f64,
}

#[cfg(feature = "performance-analysis")]
struct Profile {
    push_nanos: Vec<u128>,
    drain_nanos: u128,
}

struct ProcessedAudio {
    samples: Vec<u8>,
    #[cfg(feature = "performance-analysis")]
    profile: Option<Profile>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let options = parse_args()?;
    if !options.quiet {
        eprintln!("Reading and validating {}", options.input.display());
    }
    let clean = read_clean_wav(&options.input)?;
    let generated = generate_audio(&clean)?;
    let mulaw_input: Vec<u8> = generated.noisy_input.iter().copied().map(encode_mulaw).collect();
    let noisy_input: Vec<i16> = mulaw_input.iter().copied().map(decode_mulaw).collect();
    let stream_samples = noisy_input.len();
    let speech_offset = CALIBRATION_SAMPLES;
    let speech_end = speech_offset + clean.len();
    if generated.clean_reference.len() != clean.len()
        || stream_samples != speech_end
        || noisy_input[..CALIBRATION_SAMPLES].iter().all(|sample| *sample == 0)
        || speech_end - speech_offset != generated.clean_reference.len()
    {
        return Err("the generated noise prefix or clean/noisy sample mapping is invalid".into());
    }

    let packets = make_packets(&mulaw_input);
    let padded_samples =
        packets.len().checked_mul(PACKET_BYTES).ok_or_else(|| "packet padding length overflowed".to_owned())?;
    let padding_samples = padded_samples - stream_samples;
    if padding_samples >= PACKET_BYTES {
        return Err("only the final incomplete packet may be padded".into());
    }
    if !options.quiet {
        eprintln!("Processing {stream_samples} samples in {} ordered packets", packets.len());
    }

    #[cfg(feature = "performance-analysis")]
    let processed = process_packets(&packets, padded_samples, options.profile)?;
    #[cfg(not(feature = "performance-analysis"))]
    let processed = process_packets(&packets, padded_samples)?;

    if processed.samples.len() != padded_samples {
        return Err(format!(
            "processor returned {} samples for {padded_samples} padded input samples",
            processed.samples.len()
        ));
    }
    let valid_output_samples = processed.samples.len() - padding_samples;
    if valid_output_samples != stream_samples {
        return Err(format!(
            "processor returned {valid_output_samples} valid samples for {stream_samples} input samples"
        ));
    }
    let speech_range = speech_offset..speech_end;
    let noisy_speech = &noisy_input[speech_range.clone()];
    let enhanced_speech: Vec<i16> = processed.samples[speech_range].iter().copied().map(decode_mulaw).collect();
    if noisy_speech.len() != clean.len() || enhanced_speech.len() != clean.len() {
        return Err("speech output does not match the clean reference sample count".into());
    }

    let noisy_metrics = calculate_metrics(&generated.clean_reference, noisy_speech)?;
    let enhanced_metrics = calculate_metrics(&generated.clean_reference, &enhanced_speech)?;
    let output_root = external_temp_root()?;
    let output_dir = write_outputs(&output_root, noisy_speech, &enhanced_speech)?;

    println!("Source samples: {} at {SAMPLE_RATE} Hz", clean.len());
    println!("Noise-only prefix: {CALIBRATION_SAMPLES} samples (5 seconds)");
    println!("Speech mapping: clean[0..{}) = noisy/enhanced[{speech_offset}..{speech_end})", clean.len());
    println!("xorshift32 seed: 0x{XORSHIFT32_SEED:08x}");
    println!("Noise RMS scale: {:.8}", generated.noise_rms_scale);
    println!("Shared anti-clipping gain: {:.8}", generated.shared_gain);
    println!("Final packet padding removed: {padding_samples} samples");
    println!("Noisy speech: SNR {:.2} dB, SI-SDR {:.2} dB", noisy_metrics.snr_db, noisy_metrics.si_sdr_db);
    println!("Enhanced speech: SNR {:.2} dB, SI-SDR {:.2} dB", enhanced_metrics.snr_db, enhanced_metrics.si_sdr_db);
    println!("Noisy WAV: {}", output_dir.join("noisy.wav").display());
    println!("Enhanced WAV: {}", output_dir.join("enhanced.wav").display());

    #[cfg(feature = "performance-analysis")]
    if let Some(profile) = &processed.profile {
        let allocations = measure_allocations(&packets)?;
        report_profile(profile, &allocations)?;
    }
    Ok(())
}

fn parse_args() -> Result<Options, String> {
    let mut input = None;
    let mut quiet = false;
    #[cfg(feature = "performance-analysis")]
    let mut profile = false;

    for argument in env::args().skip(1) {
        match argument.as_str() {
            "--quiet" => quiet = true,
            "--profile" => {
                #[cfg(feature = "performance-analysis")]
                {
                    profile = true;
                }
                #[cfg(not(feature = "performance-analysis"))]
                return Err("--profile requires the performance-analysis Cargo feature".into());
            }
            _ if argument.starts_with('-') => {
                return Err(format!("unknown option {argument:?}; usage: replay <clean.wav> [--quiet] [--profile]"));
            }
            _ if input.is_none() => input = Some(PathBuf::from(argument)),
            _ => return Err("provide one clean WAV path".into()),
        }
    }

    Ok(Options {
        input: input.ok_or_else(|| "provide one clean WAV path".to_owned())?,
        quiet,
        #[cfg(feature = "performance-analysis")]
        profile,
    })
}

fn read_clean_wav(path: &Path) -> Result<Vec<i16>, String> {
    let bytes = fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("input must be a RIFF/WAVE file".into());
    }
    let riff_bytes = usize::try_from(u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]))
        .map_err(|_| "RIFF size is not representable on this platform".to_owned())?;
    let riff_end = 8_usize.checked_add(riff_bytes).ok_or_else(|| "RIFF size overflowed".to_owned())?;
    if riff_end != bytes.len() || riff_end < 12 {
        return Err("RIFF size does not match the complete input file".into());
    }

    let mut position = 12_usize;
    let mut format = None;
    let mut sample_data = None;
    while position < riff_end {
        if riff_end - position < 8 {
            return Err("truncated WAV chunk header".into());
        }
        let chunk_id = &bytes[position..position + 4];
        let chunk_len = usize::try_from(u32::from_le_bytes([
            bytes[position + 4],
            bytes[position + 5],
            bytes[position + 6],
            bytes[position + 7],
        ]))
        .map_err(|_| "WAV chunk size is not representable on this platform".to_owned())?;
        let chunk_start = position + 8;
        let chunk_end = chunk_start.checked_add(chunk_len).ok_or_else(|| "WAV chunk size overflowed".to_owned())?;
        if chunk_end > riff_end {
            return Err("WAV chunk extends beyond the RIFF container".into());
        }
        if chunk_id == b"fmt " {
            if format.is_some() || chunk_len < 16 {
                return Err("WAV must contain one complete format chunk".into());
            }
            let chunk = &bytes[chunk_start..chunk_end];
            format = Some((
                u16::from_le_bytes([chunk[0], chunk[1]]),
                u16::from_le_bytes([chunk[2], chunk[3]]),
                u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]),
                u32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]),
                u16::from_le_bytes([chunk[12], chunk[13]]),
                u16::from_le_bytes([chunk[14], chunk[15]]),
            ));
        } else if chunk_id == b"data" && sample_data.replace(&bytes[chunk_start..chunk_end]).is_some() {
            return Err("WAV must contain one data chunk".into());
        }
        position = chunk_end.checked_add(chunk_len & 1).ok_or_else(|| "WAV chunk padding overflowed".to_owned())?;
        if position > riff_end {
            return Err("truncated WAV chunk padding".into());
        }
    }

    let (format, channels, sample_rate, byte_rate, block_align, bits_per_sample) =
        format.ok_or_else(|| "WAV is missing its format chunk".to_owned())?;
    if (format, channels, sample_rate, byte_rate, block_align, bits_per_sample)
        != (1, 1, SPEECH_RATE, SPEECH_RATE * 2, 2, 16)
    {
        return Err("input must be PCM16 mono at 8 kHz".into());
    }
    let sample_data = sample_data.ok_or_else(|| "WAV is missing its data chunk".to_owned())?;
    if sample_data.len() % 2 != 0 {
        return Err("PCM16 data must contain complete samples".into());
    }
    let sample_count = sample_data.len() / 2;
    if sample_count != SOURCE_SAMPLE_COUNT {
        return Err(format!("expected exactly {SOURCE_SAMPLE_COUNT} clean samples, found {sample_count}"));
    }
    Ok(sample_data.chunks_exact(2).map(|sample| i16::from_le_bytes([sample[0], sample[1]])).collect())
}

fn generate_audio(clean: &[i16]) -> Result<GeneratedAudio, String> {
    if clean.len() != SOURCE_SAMPLE_COUNT {
        return Err(format!("expected {SOURCE_SAMPLE_COUNT} clean samples"));
    }
    let clean_values: Vec<f64> = clean.iter().copied().map(f64::from).collect();
    let clean_rms = rms(&clean_values)?;
    if clean_rms == 0.0 {
        return Err("clean reference must contain nonzero audio".into());
    }

    let stream_len = CALIBRATION_SAMPLES + clean.len();
    let mut state = XORSHIFT32_SEED;
    let mut noise: Vec<f64> = (0..stream_len).map(|_| next_uniform(&mut state)).collect();
    let noise_rms = rms(&noise[CALIBRATION_SAMPLES..])?;
    if noise_rms == 0.0 {
        return Err("generated noise has zero RMS".into());
    }
    let noise_rms_scale = clean_rms / noise_rms;
    for sample in &mut noise {
        *sample *= noise_rms_scale;
    }

    let mut peak = 0.0_f64;
    for (index, noise_sample) in noise.iter().copied().enumerate() {
        let clean_sample = if index < CALIBRATION_SAMPLES { 0.0 } else { clean_values[index - CALIBRATION_SAMPLES] };
        peak = peak.max(clean_sample.abs()).max(noise_sample.abs()).max((clean_sample + noise_sample).abs());
    }
    let shared_gain = if peak > PCM16_MAX { PCM16_MAX / peak } else { 1.0 };
    let clean_reference = clean_values.iter().copied().map(|sample| quantize_pcm16(sample * shared_gain)).collect();
    let noisy_input = noise
        .iter()
        .copied()
        .enumerate()
        .map(|(index, noise_sample)| {
            let clean_sample =
                if index < CALIBRATION_SAMPLES { 0.0 } else { clean_values[index - CALIBRATION_SAMPLES] };
            quantize_pcm16((clean_sample + noise_sample) * shared_gain)
        })
        .collect();

    Ok(GeneratedAudio { clean_reference, noisy_input, noise_rms_scale, shared_gain })
}

fn next_uniform(state: &mut u32) -> f64 {
    let mut value = *state;
    value ^= value << 13;
    value ^= value >> 17;
    value ^= value << 5;
    *state = value;
    f64::from(value) / 4_294_967_296.0 - 0.5
}

fn rms(samples: &[f64]) -> Result<f64, String> {
    if samples.is_empty() {
        return Err("cannot compute RMS of an empty sample sequence".into());
    }
    let sample_count =
        u32::try_from(samples.len()).map_err(|_| "sample sequence is too long for RMS calculation".to_owned())?;
    let mean_square = samples.iter().map(|sample| sample * sample).sum::<f64>() / f64::from(sample_count);
    Ok(mean_square.sqrt())
}

#[allow(clippy::cast_possible_truncation)]
fn quantize_pcm16(sample: f64) -> i16 {
    (sample.round().clamp(f64::from(i16::MIN), f64::from(i16::MAX))) as i16
}

fn make_packets(input: &[u8]) -> Vec<[u8; PACKET_BYTES]> {
    let mut packets = Vec::with_capacity(input.len().div_ceil(PACKET_BYTES));
    for chunk in input.chunks(PACKET_BYTES) {
        let mut packet = [0xff; PACKET_BYTES];
        packet[..chunk.len()].copy_from_slice(chunk);
        packets.push(packet);
    }
    packets
}

fn process_packets(
    packets: &[[u8; PACKET_BYTES]],
    padded_samples: usize,
    #[cfg(feature = "performance-analysis")] profile_enabled: bool,
) -> Result<ProcessedAudio, String> {
    let mut processor = Processor::default();
    let mut output = Vec::with_capacity(padded_samples);
    let mut packet_output = [0_u8; MAX_PACKET_OUTPUT_BYTES];
    #[cfg(feature = "performance-analysis")]
    let mut profile =
        profile_enabled.then(|| Profile { push_nanos: Vec::with_capacity(packets.len()), drain_nanos: 0 });

    for packet in packets {
        #[cfg(feature = "performance-analysis")]
        let written = if let Some(profile) = &mut profile {
            let started = Instant::now();
            let result = processor.push_packet(packet, &mut packet_output);
            profile.push_nanos.push(started.elapsed().as_nanos());
            result
        } else {
            processor.push_packet(packet, &mut packet_output)
        };
        #[cfg(not(feature = "performance-analysis"))]
        let written = processor.push_packet(packet, &mut packet_output);

        let written = written.map_err(|error| format!("processor push failed: {error}"))?;
        output.extend_from_slice(&packet_output[..written]);
    }

    let mut drain_output = [0_u8; MAX_DRAIN_OUTPUT_BYTES];
    #[cfg(feature = "performance-analysis")]
    let drained = if let Some(profile) = &mut profile {
        let started = Instant::now();
        let result = processor.drain(&mut drain_output);
        profile.drain_nanos = started.elapsed().as_nanos();
        result
    } else {
        processor.drain(&mut drain_output)
    };
    #[cfg(not(feature = "performance-analysis"))]
    let drained = processor.drain(&mut drain_output);

    let drained = drained.map_err(|error| format!("processor drain failed: {error}"))?;
    output.extend_from_slice(&drain_output[..drained]);
    Ok(ProcessedAudio {
        samples: output,
        #[cfg(feature = "performance-analysis")]
        profile,
    })
}

#[cfg(feature = "performance-analysis")]
struct AllocationCounts {
    push_total: usize,
    push_max: usize,
    drain: usize,
}

#[cfg(feature = "performance-analysis")]
fn measure_allocations(packets: &[[u8; PACKET_BYTES]]) -> Result<AllocationCounts, String> {
    let mut processor = Processor::default();
    let mut packet_output = [0_u8; MAX_PACKET_OUTPUT_BYTES];
    let mut push_total = 0;
    let mut push_max = 0;
    for packet in packets {
        ALLOCATION_COUNT.store(0, Ordering::Relaxed);
        TRACK_ALLOCATIONS.store(true, Ordering::Relaxed);
        let result = processor.push_packet(packet, &mut packet_output);
        TRACK_ALLOCATIONS.store(false, Ordering::Relaxed);
        let allocations = ALLOCATION_COUNT.load(Ordering::Relaxed);
        push_total += allocations;
        push_max = push_max.max(allocations);
        black_box(result.map_err(|error| format!("processor push failed: {error}"))?);
    }

    let mut drain_output = [0_u8; MAX_DRAIN_OUTPUT_BYTES];
    ALLOCATION_COUNT.store(0, Ordering::Relaxed);
    TRACK_ALLOCATIONS.store(true, Ordering::Relaxed);
    let result = processor.drain(&mut drain_output);
    TRACK_ALLOCATIONS.store(false, Ordering::Relaxed);
    let drain = ALLOCATION_COUNT.load(Ordering::Relaxed);
    black_box(result.map_err(|error| format!("processor drain failed: {error}"))?);

    Ok(AllocationCounts { push_total, push_max, drain })
}

#[cfg(feature = "performance-analysis")]
fn report_profile(profile: &Profile, allocations: &AllocationCounts) -> Result<(), String> {
    let mut sorted = profile.push_nanos.clone();
    if sorted.is_empty() {
        return Err("profile did not record any packet pushes".into());
    }
    sorted.sort_unstable();
    let percentile = |percent: usize| {
        let rank = sorted.len().saturating_mul(percent).div_ceil(100).max(1);
        sorted[rank - 1]
    };
    let maximum = *sorted.last().ok_or_else(|| "profile is empty".to_owned())?;
    let cadence_percent_hundredths = maximum / PACKET_CADENCE_NANOS * 10_000
        + ((maximum % PACKET_CADENCE_NANOS) * 10_000 + PACKET_CADENCE_NANOS / 2) / PACKET_CADENCE_NANOS;
    let cadence_percent = format!("{}.{:02}", cadence_percent_hundredths / 100, cadence_percent_hundredths % 100);
    println!("Performance profile: release path, {} push calls", sorted.len());
    println!(
        "Push latency: p50={} ns, p95={} ns, p99={} ns, max={} ns ({cadence_percent}% of 20 ms cadence)",
        percentile(50),
        percentile(95),
        percentile(99),
        maximum
    );
    println!("Drain latency: {} ns (one call)", profile.drain_nanos);
    println!(
        "Initialized-call allocations, separate pass: push total={}, max per push={}, drain={}",
        allocations.push_total, allocations.push_max, allocations.drain
    );
    println!(
        "Host architecture: {} {}; compiler/profile/run method are recorded in docs/REPLAY.md",
        env::consts::OS,
        env::consts::ARCH
    );
    Ok(())
}

fn calculate_metrics(reference: &[i16], estimate: &[i16]) -> Result<Metrics, String> {
    if reference.is_empty() || reference.len() != estimate.len() {
        return Err("metric inputs must have the same nonzero sample count".into());
    }
    let mut reference_energy = 0.0;
    let mut error_energy = 0.0;
    let mut correlation = 0.0;
    for (&clean, &observed) in reference.iter().zip(estimate) {
        let clean = f64::from(clean);
        let observed = f64::from(observed);
        reference_energy += clean * clean;
        error_energy += (observed - clean) * (observed - clean);
        correlation += clean * observed;
    }
    if reference_energy == 0.0 {
        return Err("metric reference must contain nonzero speech".into());
    }
    let projection = correlation / reference_energy;
    let mut projection_error = 0.0;
    for (&clean, &observed) in reference.iter().zip(estimate) {
        let difference = f64::from(observed) - projection * f64::from(clean);
        projection_error += difference * difference;
    }
    Ok(Metrics {
        snr_db: ratio_db(reference_energy, error_energy),
        si_sdr_db: ratio_db(projection * projection * reference_energy, projection_error),
    })
}

fn ratio_db(signal_energy: f64, error_energy: f64) -> f64 {
    if error_energy == 0.0 {
        f64::INFINITY
    } else if signal_energy == 0.0 {
        f64::NEG_INFINITY
    } else {
        10.0 * (signal_energy / error_energy).log10()
    }
}

fn external_temp_root() -> Result<PathBuf, String> {
    let root =
        env::temp_dir().canonicalize().map_err(|error| format!("could not resolve OS temporary directory: {error}"))?;
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .map_err(|error| format!("could not resolve crate directory: {error}"))?;
    let checkout = crate_dir
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "could not locate the Git checkout from the crate directory".to_owned())?
        .canonicalize()
        .map_err(|error| format!("could not resolve Git checkout: {error}"))?;
    if root == checkout || root.starts_with(&checkout) {
        return Err(format!(
            "OS temporary directory {} resolves inside the Git checkout {}",
            root.display(),
            checkout.display()
        ));
    }
    Ok(root)
}

fn create_output_dir(root: &Path) -> Result<PathBuf, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?
        .as_nanos();
    for collision in 0..128_u8 {
        let path = root.join(format!("noise-oxydation-replay-{}-{stamp}-{collision}", std::process::id()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("could not create {}: {error}", path.display())),
        }
    }
    Err("could not reserve a unique replay output directory".into())
}

fn write_outputs(root: &Path, noisy: &[i16], enhanced: &[i16]) -> Result<PathBuf, String> {
    let directory = create_output_dir(root)?;
    let noisy_path = directory.join("noisy.wav");
    let enhanced_path = directory.join("enhanced.wav");
    if let Err(error) = write_wav_new(&noisy_path, noisy) {
        let _ = fs::remove_file(&noisy_path);
        let _ = fs::remove_dir(&directory);
        return Err(error);
    }
    if let Err(error) = write_wav_new(&enhanced_path, enhanced) {
        let _ = fs::remove_file(&noisy_path);
        let _ = fs::remove_file(&enhanced_path);
        let _ = fs::remove_dir(&directory);
        return Err(error);
    }
    Ok(directory)
}

fn write_wav_new(path: &Path, samples: &[i16]) -> Result<(), String> {
    let data_bytes = samples.len().checked_mul(2).ok_or_else(|| "WAV output size overflowed".to_owned())?;
    let data_bytes = u32::try_from(data_bytes).map_err(|_| "WAV output is too large".to_owned())?;
    let riff_bytes = 36_u32.checked_add(data_bytes).ok_or_else(|| "WAV RIFF size overflowed".to_owned())?;
    let mut pcm = Vec::with_capacity(usize::try_from(data_bytes).map_err(|_| "WAV output is too large".to_owned())?);
    for sample in samples {
        pcm.extend_from_slice(&sample.to_le_bytes());
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("could not create {} without overwriting: {error}", path.display()))?;
    output.write_all(b"RIFF").map_err(|error| format!("could not write {}: {error}", path.display()))?;
    output
        .write_all(&riff_bytes.to_le_bytes())
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    output.write_all(b"WAVEfmt ").map_err(|error| format!("could not write {}: {error}", path.display()))?;
    output.write_all(&16_u32.to_le_bytes()).map_err(|error| format!("could not write {}: {error}", path.display()))?;
    output.write_all(&1_u16.to_le_bytes()).map_err(|error| format!("could not write {}: {error}", path.display()))?;
    output.write_all(&1_u16.to_le_bytes()).map_err(|error| format!("could not write {}: {error}", path.display()))?;
    output
        .write_all(&SPEECH_RATE.to_le_bytes())
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    output
        .write_all(&(SPEECH_RATE * 2).to_le_bytes())
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    output.write_all(&2_u16.to_le_bytes()).map_err(|error| format!("could not write {}: {error}", path.display()))?;
    output.write_all(&16_u16.to_le_bytes()).map_err(|error| format!("could not write {}: {error}", path.display()))?;
    output.write_all(b"data").map_err(|error| format!("could not write {}: {error}", path.display()))?;
    output
        .write_all(&data_bytes.to_le_bytes())
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    output.write_all(&pcm).map_err(|error| format!("could not write {}: {error}", path.display()))?;
    output.flush().map_err(|error| format!("could not flush {}: {error}", path.display()))?;
    Ok(())
}

fn encode_mulaw(sample: i16) -> u8 {
    let sample = i32::from(sample);
    let sign = if sample < 0 { 0x80_u8 } else { 0 };
    let magnitude = sample.abs().min(32_635) + 0x84;
    let mut exponent = 0_u8;
    while exponent < 7 && magnitude > (0xff_i32 << exponent) {
        exponent += 1;
    }
    let mantissa =
        u8::try_from((magnitude >> (u32::from(exponent) + 3)) & 0x0f).expect("μ-law mantissa fits in four bits");
    !(sign | (exponent << 4) | mantissa)
}

fn decode_mulaw(byte: u8) -> i16 {
    let value = !byte;
    let sign = value & 0x80 != 0;
    let exponent = i32::from((value >> 4) & 0x07);
    let mantissa = i32::from(value & 0x0f);
    let magnitude = ((mantissa << 3) + 0x84) << exponent;
    let sample = magnitude - 0x84;
    let signed_sample = if sign { -sample } else { sample };
    i16::try_from(signed_sample).expect("μ-law codewords decode within the i16 range")
}
