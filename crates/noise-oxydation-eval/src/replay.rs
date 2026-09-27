//! `replay`: enhances the real-speech scenarios with every evaluated configuration and writes listenable WAV files,
//! the headerless μ-law input and outputs, and metrics.
//!
//! Output layout, all under the Git-ignored `audio/out/` by default:
//!
//! ```text
//! <scenario>/clean.wav                     clean speech reference (zeros during the intro)
//! <scenario>/noisy.wav, noisy.ul           the μ-law input fed to the enhancer, as WAV and headerless μ-law
//! <scenario>/enhanced-<configuration>.wav  enhanced output, as WAV
//! <scenario>/enhanced-<configuration>.ul   enhanced output, headerless μ-law
//! <scenario>/metrics.json                  metrics of the noisy input and of every configuration
//! ```

use std::{
    ops::Range,
    path::{Path, PathBuf},
    time::Duration,
};

use noise_oxydation::{
    CallConfig, CallEnhancer, DELAY_PACKETS, InterferenceConfig, McraConfig, MinimumConfig, NoiseEstimatorConfig,
    PACKET_SAMPLES, Packet, PacketOutcome,
};

use crate::{
    audio_io::{create_dir, read_wav, write_bytes, write_wav},
    convert::quantize_pcm16,
    error::EvalError,
    json::Value,
    metrics::{FRAMES_PER_SECOND, FrameClass, Reference, SignalMetrics, WindowMetrics, recovery_window},
    mulaw,
    resample::decimate_16k_to_8k,
    scenario::{SAMPLE_RATE, Scenario, Sources, scenarios},
};

/// Where `replay` reads sources and writes results.
#[derive(Debug, Clone)]
pub(crate) struct ReplayOptions {
    pub(crate) sources: PathBuf,
    pub(crate) out: PathBuf,
}

/// The evaluated configurations: every noise estimator with tonal transient suppression, and SPP-MMSE without it.
pub(crate) fn configurations() -> [(&'static str, CallConfig); 4] {
    let with_estimator = |noise_estimator| CallConfig { noise_estimator, ..CallConfig::default() };
    [
        ("spp-mmse", CallConfig::default()),
        ("mcra", with_estimator(NoiseEstimatorConfig::Mcra(McraConfig::default()))),
        ("minimum", with_estimator(NoiseEstimatorConfig::Minimum(MinimumConfig::default()))),
        (
            "spp-mmse-no-interference",
            CallConfig { interference: InterferenceConfig::Disabled, ..CallConfig::default() },
        ),
    ]
}

/// Enhances a whole number of μ-law packets as one call, packet by packet, and drains it. The output is aligned
/// sample for sample with the input.
pub(crate) fn enhance(input: &[u8], config: &CallConfig) -> Result<Vec<u8>, EvalError> {
    let mut enhancer = CallEnhancer::new(config)?;
    let mut output = Vec::with_capacity(input.len());
    let mut packet_out: Packet = [0; PACKET_SAMPLES];
    for packet in input.as_chunks::<PACKET_SAMPLES>().0 {
        if enhancer.process_packet(packet, &mut packet_out)? == PacketOutcome::Emitted {
            output.extend_from_slice(&packet_out);
        }
    }
    let mut tail = [[0; PACKET_SAMPLES]; DELAY_PACKETS];
    let withheld = enhancer.drain(&mut tail)?;
    tail[..withheld].iter().for_each(|packet| output.extend_from_slice(packet));
    Ok(output)
}

/// Metrics start this long after the end of calibration.
const SETTLING: Duration = Duration::from_secs(1);
/// Windows of the speech-during-calibration comparison.
const CALIBRATION_WINDOWS: usize = 10;

/// One scenario's enhanced outputs and metrics.
struct ScenarioResult {
    scenario: Scenario,
    noisy: Vec<f64>,
    evaluated: Range<usize>,
    active_frames: usize,
    pause_frames: usize,
    noisy_metrics: SignalMetrics,
    enhanced: Vec<(&'static str, Vec<f64>, SignalMetrics)>,
}

/// Recovery comparison of one configuration.
struct Recovery {
    configuration: &'static str,
    windows: Vec<(WindowMetrics, WindowMetrics)>,
    recovered_after: Option<usize>,
}

pub(crate) fn replay(options: &ReplayOptions) -> Result<(), EvalError> {
    let sources = load_sources(&options.sources)?;
    let calibration = CallConfig::default().calibration_duration;
    let evaluated_from = seconds_to_frames(calibration + SETTLING);
    let mut results = Vec::new();
    for scenario in scenarios(&sources)? {
        results.push(run_scenario(scenario, evaluated_from, &options.out)?);
    }

    let recovery = calibration_recovery(&results);
    for result in &results {
        let recovery = (result.scenario.name == "speech-during-calibration").then_some(recovery.as_slice());
        let json = scenario_json(result, recovery);
        write_bytes(&options.out.join(result.scenario.name).join("metrics.json"), json.render().as_bytes())?;
        print_scenario(result);
    }
    print_recovery(&recovery);
    println!("\nwrote clean, noisy and enhanced audio and metrics.json under {}", options.out.display());
    Ok(())
}

fn load_sources(directory: &Path) -> Result<Sources, EvalError> {
    let speech = |name: &str| read_wav(&directory.join(name), 8000);
    let noise = |name: &str| read_wav(&directory.join(name), 16_000).map(|noise| decimate_16k_to_8k(&noise));
    Ok(Sources {
        speech_0030: speech("OSR_us_000_0030_8k.wav")?,
        speech_0031: speech("OSR_us_000_0031_8k.wav")?,
        office_noise: noise("DEMAND_OOFFICE_ch01.wav")?,
        cafeteria_noise: noise("DEMAND_PCAFETER_ch01.wav")?,
    })
}

fn seconds_to_frames(duration: Duration) -> usize {
    usize::try_from(duration.as_millis()).expect("durations of a call fit usize") * FRAMES_PER_SECOND / 1000
}

fn run_scenario(scenario: Scenario, evaluated_from: usize, out: &Path) -> Result<ScenarioResult, EvalError> {
    let directory = create_dir(&out.join(scenario.name))?;
    let noisy_mulaw = mulaw::encode_signal(&scenario.noisy);
    let noisy = mulaw::decode_signal(&noisy_mulaw);
    let rate = u32::try_from(SAMPLE_RATE).expect("8000 fits u32");
    write_wav(
        &directory.join("clean.wav"),
        rate,
        &scenario.clean.iter().map(|&x| quantize_pcm16(x)).collect::<Vec<_>>(),
    )?;
    write_wav(
        &directory.join("noisy.wav"),
        rate,
        &noisy_mulaw.iter().map(|&byte| mulaw::decode(byte)).collect::<Vec<_>>(),
    )?;
    write_bytes(&directory.join("noisy.ul"), &noisy_mulaw)?;

    let reference = Reference::new(&scenario.clean);
    let evaluated = evaluated_from..reference.classes().len();
    let noisy_metrics = SignalMetrics::measure(&reference, &noisy, &noisy, evaluated.clone());
    let mut enhanced = Vec::new();
    for (name, config) in configurations() {
        let output = enhance(&noisy_mulaw, &config)?;
        write_bytes(&directory.join(format!("enhanced-{name}.ul")), &output)?;
        write_wav(
            &directory.join(format!("enhanced-{name}.wav")),
            rate,
            &output.iter().map(|&byte| mulaw::decode(byte)).collect::<Vec<_>>(),
        )?;
        let decoded = mulaw::decode_signal(&output);
        let metrics = SignalMetrics::measure(&reference, &noisy, &decoded, evaluated.clone());
        enhanced.push((name, decoded, metrics));
    }
    Ok(ScenarioResult {
        active_frames: reference.count(FrameClass::Active, evaluated.clone()),
        pause_frames: reference.count(FrameClass::Pause, evaluated.clone()),
        evaluated,
        noisy_metrics,
        enhanced,
        noisy,
        scenario,
    })
}

/// Compares the speech-during-calibration run with the speech-after-calibration run on the same speech and noise:
/// window `k` covers `[5 + k, 6 + k)` s of the during run and `[11 + k, 12 + k)` s of the after run.
fn calibration_recovery(results: &[ScenarioResult]) -> Vec<Recovery> {
    let find = |name| results.iter().find(|result| result.scenario.name == name).expect("scenario exists");
    let during = find("speech-during-calibration");
    let after = find("speech-after-calibration");
    let during_reference = Reference::new(&during.scenario.clean);
    let after_reference = Reference::new(&after.scenario.clean);
    let window = |start_second: usize| start_second * FRAMES_PER_SECOND..(start_second + 1) * FRAMES_PER_SECOND;
    during
        .enhanced
        .iter()
        .zip(&after.enhanced)
        .map(|((configuration, during_output, _), (_, after_output, _))| {
            let windows: Vec<_> = (0..CALIBRATION_WINDOWS)
                .map(|k| {
                    (
                        WindowMetrics::measure(&during_reference, &during.noisy, during_output, window(5 + k)),
                        WindowMetrics::measure(&after_reference, &after.noisy, after_output, window(11 + k)),
                    )
                })
                .collect();
            Recovery { configuration, recovered_after: recovery_window(&windows), windows }
        })
        .collect()
}

fn metrics_json(metrics: &SignalMetrics, noisy: Option<&SignalMetrics>) -> Value {
    let mut fields = vec![
        ("noise_attenuation_db", Value::decibels(metrics.noise_attenuation)),
        ("segmental_snr_db", Value::decibels(metrics.segmental_snr)),
    ];
    if let Some(noisy) = noisy {
        let improvement = metrics.segmental_snr.zip(noisy.segmental_snr).map(|(enhanced, noisy)| enhanced - noisy);
        fields.push(("segmental_snr_improvement_db", Value::decibels(improvement)));
    }
    fields.push(("speech_level_change_db", Value::decibels(metrics.speech_level_change)));
    fields.push(("log_spectral_distance_db", Value::decibels(metrics.log_spectral_distance)));
    Value::Object(fields)
}

fn window_json(metrics: &WindowMetrics) -> Value {
    Value::Object(vec![
        ("speech_level_change_db", Value::decibels(metrics.speech_level_change)),
        ("noise_attenuation_db", Value::decibels(metrics.noise_attenuation)),
    ])
}

fn scenario_json(result: &ScenarioResult, recovery: Option<&[Recovery]>) -> Value {
    let scenario = &result.scenario;
    let samples = u64::try_from(scenario.clean.len()).expect("fits u64");
    let mut fields = vec![
        ("scenario", Value::text(scenario.name)),
        ("description", Value::text(scenario.description.clone())),
        ("sample_rate_hz", Value::Integer(8000)),
        ("samples", Value::Integer(samples)),
        ("noise_gain_db", Value::Number(scenario.noise_gain_db)),
        ("headroom_gain_db", Value::Number(scenario.headroom_gain_db)),
        ("evaluated_from_frame", Value::Integer(u64::try_from(result.evaluated.start).expect("fits u64"))),
        ("active_frames", Value::Integer(u64::try_from(result.active_frames).expect("fits u64"))),
        ("pause_frames", Value::Integer(u64::try_from(result.pause_frames).expect("fits u64"))),
        ("noisy", metrics_json(&result.noisy_metrics, None)),
        (
            "configurations",
            Value::Object(
                result
                    .enhanced
                    .iter()
                    .map(|(name, _, metrics)| (*name, metrics_json(metrics, Some(&result.noisy_metrics))))
                    .collect(),
            ),
        ),
    ];
    if let Some(recovery) = recovery {
        let entries = recovery
            .iter()
            .map(|recovery| {
                Value::Object(vec![
                    ("configuration", Value::text(recovery.configuration)),
                    (
                        "recovery_seconds",
                        recovery
                            .recovered_after
                            .map_or(Value::Null, |k| Value::Integer(u64::try_from(k).expect("small"))),
                    ),
                    (
                        "windows",
                        Value::Array(
                            recovery
                                .windows
                                .iter()
                                .enumerate()
                                .map(|(k, (during, after))| {
                                    Value::Object(vec![
                                        ("k", Value::Integer(u64::try_from(k).expect("small"))),
                                        ("during", window_json(during)),
                                        ("after", window_json(after)),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                ])
            })
            .collect();
        fields.push(("calibration_recovery", Value::Array(entries)));
    }
    Value::Object(fields)
}

fn format_db(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |value| format!("{value:.2}"))
}

fn print_scenario(result: &ScenarioResult) {
    let scenario = &result.scenario;
    println!(
        "\n{} ({}); noise gain {:.2} dB, headroom gain {:.2} dB; {} active and {} pause frames from {} s",
        scenario.name,
        scenario.description,
        scenario.noise_gain_db,
        scenario.headroom_gain_db,
        result.active_frames,
        result.pause_frames,
        result.evaluated.start / FRAMES_PER_SECOND
    );
    println!(
        "  {:<26} {:>12} {:>9} {:>11} {:>13} {:>8}",
        "signal", "noise atten.", "seg. SNR", "SNR impr.", "speech level", "LSD"
    );
    let noisy = &result.noisy_metrics;
    println!(
        "  {:<26} {:>12} {:>9} {:>11} {:>13} {:>8}",
        "noisy input",
        format_db(noisy.noise_attenuation),
        format_db(noisy.segmental_snr),
        "",
        format_db(noisy.speech_level_change),
        format_db(noisy.log_spectral_distance)
    );
    for (name, _, metrics) in &result.enhanced {
        let improvement = metrics.segmental_snr.zip(noisy.segmental_snr).map(|(enhanced, noisy)| enhanced - noisy);
        println!(
            "  {:<26} {:>12} {:>9} {:>11} {:>13} {:>8}",
            name,
            format_db(metrics.noise_attenuation),
            format_db(metrics.segmental_snr),
            format_db(improvement),
            format_db(metrics.speech_level_change),
            format_db(metrics.log_spectral_distance)
        );
    }
}

fn print_recovery(recovery: &[Recovery]) {
    println!("\nspeech during calibration: speech level change (dB) per 1 s window, during run / after run");
    print!("  {:<26}", "window [5+k, 6+k) s, k =");
    for k in 0..CALIBRATION_WINDOWS {
        print!(" {k:>13}");
    }
    println!(" {:>10}", "recovery");
    for entry in recovery {
        print!("  {:<26}", entry.configuration);
        for (during, after) in &entry.windows {
            print!(
                " {:>13}",
                format!("{}/{}", format_db(during.speech_level_change), format_db(after.speech_level_change))
            );
        }
        let recovered =
            entry.recovered_after.map_or_else(|| format!("> {CALIBRATION_WINDOWS} s"), |k| format!("{k} s"));
        println!(" {recovered:>10}");
    }
}
