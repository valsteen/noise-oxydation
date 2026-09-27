//! Regenerate the visitor guide and its day/night diagrams with the Rust standard library.
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;

const GUIDE: &str = r#"# How Noise Oxydation works

Noise Oxydation enhances one 8 kHz mono G.711 μ-law call through a `Pipeline`. The caller supplies 160-byte packets, receives ordered complete output packets, and calls `finish` once to drain the final valid samples. A quiet call intro lets the pipeline learn a noise baseline before suppression begins.

## Ownership

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/how-it-works/crates-night.svg">
  <img src="assets/how-it-works/crates-day.svg" alt="Pipeline owns packet and call state and depends on separate codec and DSP crates.">
</picture>

The Rust 2024 workspace has three crates. `pipeline` owns public configuration, packet buffering, and call lifecycle. It calls `codec` for μ-law conversion and `dsp` for fixed-frame enhancement. Neither lower crate depends on `pipeline`. Each `Pipeline` contains its own estimator, FFT, tonal, and output state.

Independent calls can run at the same time when the caller schedules separate `Pipeline` instances. Within one call, packets, FFT hops, estimator updates, and `finish` advance that instance in order. There is no internal worker or shared DSP state.

## One call's audio flow

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/how-it-works/audio-night.svg">
  <img src="assets/how-it-works/audio-day.svg" alt="Decoded packets enter high-pass filtering and 256-point STFT. After the quiet intro, selected noise estimation, Log-MMSE, tonal gain, and synthesis lead to encoded output. During the intro, original decoded audio bypasses suppression while complete frames learn noise.">
</picture>

`process_packet(&[u8; 160])` decodes each 20 ms packet. A first-order high-pass filter feeds a periodic Hann short-time Fourier transform (STFT). The first 256 samples complete a frame; subsequent frames start every 128 samples. One input packet can complete zero, one, or two hops, so its result can contain zero, one, or two **whole** output packets.

The default `learning_duration` is five seconds and assumes a quiet intro. Complete FFT windows wholly inside the interval update the noise baseline. Frames starting before the cutoff emit the original decoded PCM instead of filtered, reconstructed audio. With the default 40,000-sample interval, the first suppressed frame starts at sample 40,064 (5.008 seconds). A different call setup can configure another duration; at least one complete 256-sample frame is required.

After the intro, the selected estimator updates one call-owned noise spectrum:

- **SPP-MMSE** is the default. It smooths speech-presence probability for a conditional noise update.
- **MCRA** compares smoothed power with a fixed 50-frame minimum history, then uses speech probability to slow noise updates. A Rust safeguard protects sustained narrowband voice after the window turns over.
- **Minimum estimation** uses the same history as a lower-envelope estimate. It can learn continuous foreground energy.

All three alternatives feed the same decision-directed prior SNR and Log-MMSE gain. Tonal gain then attenuates isolated transient peaks while accounting for neighboring and harmonic evidence. The modified spectrum passes through inverse FFT, Hann synthesis, and window-square overlap normalization. `codec` encodes the valid output samples back to μ-law.

The formulas and safeguards chosen where the Go documentation is incomplete are [recorded separately](docs/reference-observations.md). The [real-speech replay](docs/real-audio-evidence.md) measures this Rust path on one credited clip; it does not establish universal speech quality or exact Go numerical parity.

## End of call and another call

`finish` pads the DSP just enough to emit the remaining valid audio and reports the last packet's valid sample count. With fixed 160-sample inputs, a returned final packet currently has 160 valid samples. A second `finish` or a `process_packet` after finishing returns `AlreadyFinished`. `reset` clears the filter, estimator, tonal, FFT, overlap, pending-packet, and finish state while retaining configuration.

Processing and finishing allocate no memory after construction and use no blocking synchronization. The opt-in `logging` feature only writes a status snapshot when the caller explicitly invokes `write_status` outside the audio thread. The opt-in `performance-analysis` examples run offline.

See [Architecture](ARCHITECTURE.md) for the detailed timing and ownership contract and [README](README.md) for a caller example. The project is [MIT licensed](LICENSE).
"#;

#[derive(Clone, Copy)]
struct Palette {
    canvas: &'static str,
    grid: &'static str,
    frame: &'static str,
    card: &'static str,
    border: &'static str,
    text: &'static str,
    label: &'static str,
    muted: &'static str,
    connector: &'static str,
    primary: &'static str,
    secondary: &'static str,
}

const DAY: Palette = Palette {
    canvas: "#fffdf7",
    grid: "#e8e1d5",
    frame: "#cec7b9",
    card: "#ffffff",
    border: "#aaa397",
    text: "#292721",
    label: "#6d665b",
    muted: "#625d53",
    connector: "#697487",
    primary: "#394a62",
    secondary: "#81796f",
};

const NIGHT: Palette = Palette {
    canvas: "#1c2127",
    grid: "#45535f",
    frame: "#45515c",
    card: "#242b32",
    border: "#5e6a74",
    text: "#edf1f2",
    label: "#b7c0c5",
    muted: "#c5cdd0",
    connector: "#9ba8b1",
    primary: "#91a7bb",
    secondary: "#9b948b",
};

#[derive(Clone, Copy)]
struct Card {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    label: &'static str,
    title: &'static str,
    details: &'static [&'static str],
    meta: &'static str,
    secondary: bool,
}

const CRATES: &[Card] = &[
    Card {
        x: 55,
        y: 25,
        width: 330,
        height: 115,
        label: "CALL OWNER",
        title: "One Pipeline per call",
        details: &["Separate calls may run concurrently"],
        meta: "caller schedules each instance",
        secondary: false,
    },
    Card {
        x: 55,
        y: 165,
        width: 330,
        height: 190,
        label: "PIPELINE CRATE",
        title: "Packet and call lifecycle",
        details: &[
            "Config · process_packet",
            "finish once · reset",
            "owns pending output",
        ],
        meta: "noise-oxydation-pipeline",
        secondary: false,
    },
    Card {
        x: 650,
        y: 90,
        width: 255,
        height: 125,
        label: "CODEC CRATE",
        title: "G.711 μ-law conversion",
        details: &["decode and encode"],
        meta: "noise-oxydation-codec",
        secondary: true,
    },
    Card {
        x: 650,
        y: 280,
        width: 255,
        height: 155,
        label: "DSP CRATE",
        title: "Fixed-frame enhancement",
        details: &["high-pass · STFT / ISTFT", "estimators · gains · overlap"],
        meta: "noise-oxydation-dsp",
        secondary: false,
    },
];

const AUDIO: &[Card] = &[
    Card {
        x: 40,
        y: 215,
        width: 175,
        height: 110,
        label: "INPUT",
        title: "Decode μ-law",
        details: &["160 bytes / 20 ms"],
        meta: "",
        secondary: false,
    },
    Card {
        x: 275,
        y: 80,
        width: 195,
        height: 145,
        label: "FRAME ANALYSIS",
        title: "High-pass + STFT",
        details: &["Hann, 256 samples", "128-sample hop"],
        meta: "",
        secondary: false,
    },
    Card {
        x: 515,
        y: 80,
        width: 205,
        height: 145,
        label: "NOISE AND GAIN",
        title: "Noise + Log-MMSE",
        details: &["SPP-MMSE default", "MCRA / minimum"],
        meta: "",
        secondary: false,
    },
    Card {
        x: 760,
        y: 80,
        width: 205,
        height: 145,
        label: "SYNTHESIS",
        title: "Tonal + inverse FFT",
        details: &["all estimators", "overlap normalization"],
        meta: "",
        secondary: false,
    },
    Card {
        x: 275,
        y: 380,
        width: 245,
        height: 125,
        label: "QUIET INTRO",
        title: "Original PCM bypass",
        details: &["frames before cutoff", "complete windows learn noise"],
        meta: "",
        secondary: true,
    },
    Card {
        x: 760,
        y: 380,
        width: 205,
        height: 125,
        label: "OUTPUT",
        title: "Encode + batch",
        details: &["0–2 whole packets", "finish: final valid count"],
        meta: "",
        secondary: false,
    },
];

fn start_svg(width: u16, height: u16, title: &str, description: &str, p: Palette) -> String {
    let mut svg = String::new();
    writeln!(svg, r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-labelledby="title description">"#).unwrap();
    writeln!(svg, "<title id=\"title\">{title}</title>").unwrap();
    writeln!(svg, "<desc id=\"description\">{description}</desc>").unwrap();
    writeln!(svg, r#"<defs><pattern id="grid" width="24" height="24" patternUnits="userSpaceOnUse"><path d="M 24 0 H 0 V 24" fill="none" stroke="{}" stroke-width="1"/></pattern><marker id="arrow" markerWidth="10" markerHeight="10" refX="9" refY="5" orient="auto" markerUnits="userSpaceOnUse"><path d="M 0 0 L 10 5 L 0 10 Z" fill="{}"/></marker><marker id="bypass-arrow" markerWidth="10" markerHeight="10" refX="9" refY="5" orient="auto" markerUnits="userSpaceOnUse"><path d="M 0 0 L 10 5 L 0 10 Z" fill="{}"/></marker></defs>"#, p.grid, p.connector, p.secondary).unwrap();
    writeln!(svg, r#"<rect width="{width}" height="{height}" fill="{}"/><rect width="{width}" height="{height}" fill="url(#grid)"/><rect x="1" y="1" width="{}" height="{}" fill="none" stroke="{}" stroke-width="2"/>"#, p.canvas, width - 2, height - 2, p.frame).unwrap();
    svg
}

fn path(svg: &mut String, data: &str, color: &str, marker: &str) {
    writeln!(svg, r#"<path d="{data}" fill="none" stroke="{color}" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" marker-end="url(#{marker})"/>"#).unwrap();
}

fn card_boxes(svg: &mut String, cards: &[Card], p: Palette) {
    for card in cards {
        let accent = if card.secondary {
            p.secondary
        } else {
            p.primary
        };
        writeln!(svg, r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" stroke="{}" stroke-width="1.5"/><rect x="{}" y="{}" width="6" height="{}" fill="{}"/>"#, card.x, card.y, card.width, card.height, p.card, p.border, card.x, card.y, card.height, accent).unwrap();
    }
}

fn card_text(svg: &mut String, cards: &[Card], p: Palette) {
    for card in cards {
        let x = card.x + 22;
        let label_y = card.y + 26;
        let title_y = card.y + 55;
        writeln!(svg, r#"<text x="{x}" y="{label_y}" fill="{}" font-family="ui-monospace, SFMono-Regular, monospace" font-size="11.5" font-weight="700" letter-spacing="1">{}</text>"#, p.label, card.label).unwrap();
        writeln!(svg, r#"<text x="{x}" y="{title_y}" fill="{}" font-family="-apple-system, BlinkMacSystemFont, Segoe UI, sans-serif" font-size="17" font-weight="700">{}</text>"#, p.text, card.title).unwrap();
        for (i, detail) in card.details.iter().enumerate() {
            let y = card.y + 81 + u16::try_from(i).unwrap() * 20;
            writeln!(svg, r#"<text x="{x}" y="{y}" fill="{}" font-family="-apple-system, BlinkMacSystemFont, Segoe UI, sans-serif" font-size="13">{detail}</text>"#, p.muted).unwrap();
        }
        if !card.meta.is_empty() {
            let y = card.y + card.height - 16;
            writeln!(svg, r#"<text x="{x}" y="{y}" fill="{}" font-family="ui-monospace, SFMono-Regular, monospace" font-size="11">{}</text>"#, p.label, card.meta).unwrap();
        }
    }
}

fn canvas_label(svg: &mut String, x: u16, y: u16, width: u16, text: &str, p: Palette) {
    writeln!(svg, r#"<rect x="{x}" y="{}" width="{width}" height="21" fill="{}"/><text x="{}" y="{y}" fill="{}" font-family="ui-monospace, SFMono-Regular, monospace" font-size="12" font-weight="700" letter-spacing="0.8">{text}</text>"#, y - 16, p.canvas, x + 5, p.label).unwrap();
}

fn crate_svg(p: Palette) -> String {
    let mut svg = start_svg(
        960,
        470,
        "Crate ownership and dependency direction",
        "Each caller owns a Pipeline for one call. Pipeline depends on codec for G.711 conversion and on DSP for fixed-frame enhancement. Independent calls can run concurrently; no lower crate depends on pipeline.",
        p,
    );
    card_boxes(&mut svg, CRATES, p);
    path(&mut svg, "M 220 140 V 165", p.connector, "arrow");
    path(
        &mut svg,
        "M 385 215 H 520 V 153 H 650",
        p.connector,
        "arrow",
    );
    path(
        &mut svg,
        "M 385 300 H 560 V 360 H 650",
        p.connector,
        "arrow",
    );
    card_text(&mut svg, CRATES, p);
    canvas_label(
        &mut svg,
        55,
        452,
        445,
        "One call = one state owner; crate code is reusable",
        p,
    );
    svg.push_str("</svg>\n");
    svg
}

fn audio_svg(p: Palette) -> String {
    let mut svg = start_svg(
        1000,
        565,
        "Packet enhancement and quiet-intro bypass",
        "One 160-byte mu-law packet is decoded. High-pass and 256-point STFT analyze 128-sample hops. After learning, selected noise estimation, Log-MMSE, tonal gain and synthesis feed encoded output. During quiet-intro learning, original decoded audio bypasses suppression.",
        p,
    );
    card_boxes(&mut svg, AUDIO, p);
    path(
        &mut svg,
        "M 215 250 H 245 V 150 H 275",
        p.connector,
        "arrow",
    );
    path(&mut svg, "M 470 150 H 515", p.connector, "arrow");
    path(&mut svg, "M 720 150 H 760", p.connector, "arrow");
    path(&mut svg, "M 862 225 V 380", p.connector, "arrow");
    path(
        &mut svg,
        "M 215 290 H 245 V 440 H 275",
        p.secondary,
        "bypass-arrow",
    );
    path(&mut svg, "M 520 440 H 760", p.secondary, "bypass-arrow");
    card_text(&mut svg, AUDIO, p);
    canvas_label(
        &mut svg,
        275,
        52,
        360,
        "ANALYZE ALWAYS · GAIN AFTER INTRO",
        p,
    );
    canvas_label(&mut svg, 275, 352, 228, "QUIET-INTRO OUTPUT", p);
    svg.push_str("</svg>\n");
    svg
}

fn output(path: &str, content: &str, check: bool) -> io::Result<()> {
    if check {
        if fs::read_to_string(path)? != content {
            return Err(io::Error::other(format!("stale generated file: {path}")));
        }
    } else {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let check = match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [] => false,
        [flag] if flag == "--check" => true,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: render_how_it_works [--check]",
            ));
        }
    };
    output("HOW_IT_WORKS.md", GUIDE, check)?;
    output("assets/how-it-works/crates-day.svg", &crate_svg(DAY), check)?;
    output(
        "assets/how-it-works/crates-night.svg",
        &crate_svg(NIGHT),
        check,
    )?;
    output("assets/how-it-works/audio-day.svg", &audio_svg(DAY), check)?;
    output(
        "assets/how-it-works/audio-night.svg",
        &audio_svg(NIGHT),
        check,
    )?;
    Ok(())
}
