use std::{fs, path::PathBuf};

#[derive(Clone, Copy)]
struct Palette {
    canvas: &'static str,
    grid: &'static str,
    frame: &'static str,
    card: &'static str,
    border: &'static str,
    accent: &'static str,
    text: &'static str,
    quiet: &'static str,
    connector: &'static str,
}

const DAY: Palette = Palette {
    canvas: "#f5f1e8",
    grid: "#dcd5c8",
    frame: "#8b8172",
    card: "#fffdf8",
    border: "#b5aa98",
    accent: "#346b6d",
    text: "#202b33",
    quiet: "#53616a",
    connector: "#415560",
};

const NIGHT: Palette = Palette {
    canvas: "#0d1117",
    grid: "#202a36",
    frame: "#536174",
    card: "#161f2b",
    border: "#526174",
    accent: "#7fd1cf",
    text: "#e6edf3",
    quiet: "#b2bfce",
    connector: "#bdcbd9",
};

const DIAGRAM: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="2650" viewBox="0 0 1280 2650" role="img" aria-labelledby="title description">
<title id="title">Noise Oxydation: Go boundary and ordered audio path</title>
<desc id="description">The Go package calls a Rust static C ABI over the Rust core, which depends on realfft. A 160-byte mu-law packet passes in order through framing, estimation, suppression, reconstruction, and output.</desc>
<defs>
  <pattern id="grid" width="32" height="32" patternUnits="userSpaceOnUse"><path d="M32 0H0V32" fill="none" stroke="__GRID__" stroke-width="1"/></pattern>
  <marker id="arrow" viewBox="0 0 12 12" refX="10" refY="6" markerWidth="10" markerHeight="10" orient="auto"><path d="M0 0L12 6L0 12Z" fill="__CONNECTOR__"/></marker>
  <style>.canvas-label { paint-order: stroke; stroke: __CANVAS__; stroke-width: 6px; stroke-linejoin: round; }</style>
</defs>
<rect width="1280" height="2650" fill="__CANVAS__"/>
<rect width="1280" height="2650" fill="url(#grid)" opacity="0.72"/>
<rect x="32" y="32" width="1216" height="2586" fill="none" stroke="__FRAME__" stroke-width="2"/>
<g font-family="system-ui, -apple-system, Segoe UI, sans-serif" fill="__TEXT__">
  <text class="canvas-label" x="72" y="96" font-size="30" font-weight="700">How one Go packet reaches the Rust core</text>
  <text class="canvas-label" x="72" y="128" font-size="16" fill="__QUIET__">Go owns the handle · Rust owns packet state and DSP · independent calls may run concurrently</text>

  <rect x="60" y="170" width="1160" height="390" fill="none" stroke="__FRAME__" stroke-width="2"/>
  <text class="canvas-label" x="88" y="210" font-size="14" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1.2" fill="__ACCENT__">01 / GO AND RUST OWNERSHIP</text>
  <rect x="95" y="245" width="615" height="280" fill="none" stroke="__FRAME__" stroke-width="2"/>
  <text class="canvas-label" x="120" y="276" font-size="14" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1" fill="__ACCENT__">CARGO WORKSPACE · 2 PRODUCTION CRATES</text>

  <g>
    <rect x="120" y="300" width="565" height="85" fill="__CARD__" stroke="__BORDER__" stroke-width="2"/>
    <rect x="120" y="300" width="7" height="85" fill="__ACCENT__"/>
    <text x="148" y="329" font-size="13" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1" fill="__ACCENT__">CORE DSP OWNER</text>
    <text x="148" y="357" font-size="21" font-weight="650">noise-oxydation-core</text>
    <text x="438" y="357" font-size="14" fill="__QUIET__">packet state · realfft</text>

    <rect x="120" y="415" width="565" height="80" fill="__CARD__" stroke="__BORDER__" stroke-width="2"/>
    <rect x="120" y="415" width="7" height="80" fill="__ACCENT__"/>
    <text x="148" y="444" font-size="13" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1" fill="__ACCENT__">STATIC C ABI</text>
    <text x="148" y="473" font-size="21" font-weight="650">noise-oxydation-ffi</text>
    <text x="405" y="473" font-size="14" fill="__QUIET__">opaque handle · statuses</text>
    <text x="120" y="516" font-size="13" fill="__QUIET__">examples/replay.rs stays with core</text>
  </g>

  <path d="M402 415V390" fill="none" stroke="__CONNECTOR__" stroke-width="2.5" marker-end="url(#arrow)"/>
  <text class="canvas-label" x="431" y="402" font-size="12" fill="__QUIET__">depends on</text>

  <g>
    <rect x="785" y="245" width="400" height="120" fill="__CARD__" stroke="__BORDER__" stroke-width="2"/>
    <rect x="785" y="245" width="7" height="120" fill="__ACCENT__"/>
    <text x="812" y="276" font-size="13" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1" fill="__ACCENT__">GO PACKAGE</text>
    <text x="812" y="309" font-size="20" font-weight="650">bindings/go</text>
    <text x="812" y="335" font-size="14" fill="__QUIET__">cgo · private C types · reusable buffers</text>
    <text x="812" y="354" font-size="13" fill="__QUIET__">links the checkout-built static archive</text>
  </g>
  <path d="M785 304H755V455H710" fill="none" stroke="__CONNECTOR__" stroke-width="2.5" marker-end="url(#arrow)"/>
  <text class="canvas-label" x="761" y="394" font-size="12" fill="__QUIET__" transform="rotate(-90 761 394)">calls ABI</text>

  <path d="M685 340H870V455H930" fill="none" stroke="__CONNECTOR__" stroke-width="2.5" marker-end="url(#arrow)"/>
  <g>
    <rect x="930" y="415" width="255" height="90" fill="__CARD__" stroke="__BORDER__" stroke-width="2"/>
    <rect x="930" y="415" width="7" height="90" fill="__ACCENT__"/>
    <text x="956" y="446" font-size="13" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1" fill="__ACCENT__">TRANSFORM DEPENDENCY</text>
    <text x="956" y="477" font-size="21" font-weight="650">realfft</text>
    <text x="1040" y="477" font-size="14" fill="__QUIET__">real-valued FFT</text>
  </g>

  <rect x="60" y="600" width="1160" height="2020" fill="none" stroke="__FRAME__" stroke-width="2"/>
  <text class="canvas-label" x="88" y="644" font-size="14" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1.2" fill="__ACCENT__">02 / ORDERED PACKET PATH</text>
  <text class="canvas-label" x="88" y="676" font-size="16" fill="__QUIET__">Solid arrows show the order within one Processor; branches preserve the original-power inputs.</text>

  <g>
    <rect x="440" y="730" width="400" height="100" fill="__CARD__" stroke="__BORDER__" stroke-width="2"/>
    <rect x="440" y="730" width="7" height="100" fill="__ACCENT__"/>
    <text x="466" y="758" font-size="13" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1" fill="__ACCENT__">INPUT</text>
    <text x="466" y="786" font-size="19" font-weight="650">160-byte μ-law packet</text>
    <text x="466" y="811" font-size="15" fill="__QUIET__">8 kHz mono stream</text>
  </g>
  <path d="M640 830V865" fill="none" stroke="__CONNECTOR__" stroke-width="2.5" marker-end="url(#arrow)"/>

  <g>
    <rect x="440" y="865" width="400" height="105" fill="__CARD__" stroke="__BORDER__" stroke-width="2"/>
    <rect x="440" y="865" width="7" height="105" fill="__ACCENT__"/>
    <text x="466" y="893" font-size="13" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1" fill="__ACCENT__">DECODE + FILTER</text>
    <text x="466" y="922" font-size="19" font-weight="650">μ-law decode · 80 Hz high-pass</text>
    <text x="466" y="949" font-size="15" fill="__QUIET__">bytes become filtered PCM samples</text>
  </g>
  <path d="M640 970V1005" fill="none" stroke="__CONNECTOR__" stroke-width="2.5" marker-end="url(#arrow)"/>

  <g>
    <rect x="440" y="1005" width="400" height="105" fill="__CARD__" stroke="__BORDER__" stroke-width="2"/>
    <rect x="440" y="1005" width="7" height="105" fill="__ACCENT__"/>
    <text x="466" y="1033" font-size="13" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1" fill="__ACCENT__">FRAMING</text>
    <text x="466" y="1062" font-size="19" font-weight="650">Hann-windowed analysis frame</text>
    <text x="466" y="1089" font-size="15" fill="__QUIET__">256 samples · 128-sample hop</text>
  </g>
  <path d="M640 1110V1145" fill="none" stroke="__CONNECTOR__" stroke-width="2.5" marker-end="url(#arrow)"/>

  <g>
    <rect x="440" y="1145" width="400" height="105" fill="__CARD__" stroke="__BORDER__" stroke-width="2"/>
    <rect x="440" y="1145" width="7" height="105" fill="__ACCENT__"/>
    <text x="466" y="1173" font-size="13" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1" fill="__ACCENT__">TRANSFORM</text>
    <text x="466" y="1202" font-size="19" font-weight="650">Real FFT</text>
    <text x="466" y="1229" font-size="15" fill="__QUIET__">one spectrum per analysis frame</text>
  </g>
  <path d="M640 1250V1285" fill="none" stroke="__CONNECTOR__" stroke-width="2.5" marker-end="url(#arrow)"/>

  <g>
    <rect x="440" y="1285" width="400" height="130" fill="__CARD__" stroke="__BORDER__" stroke-width="2"/>
    <rect x="440" y="1285" width="7" height="130" fill="__ACCENT__"/>
    <text x="466" y="1313" font-size="13" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1" fill="__ACCENT__">ORIGINAL FRAME POWER</text>
    <text x="466" y="1342" font-size="19" font-weight="650">Shared analysis input</text>
    <text x="466" y="1369" font-size="15" fill="__QUIET__">feeds estimator and tonal detector</text>
    <text x="466" y="1392" font-size="15" fill="__QUIET__">also remains the observed power</text>
  </g>

  <path d="M510 1415V1460Q510 1470 500 1470H320V1500" fill="none" stroke="__CONNECTOR__" stroke-width="2.5" marker-end="url(#arrow)"/>
  <path d="M770 1415V1460Q770 1470 780 1470H960V1500" fill="none" stroke="__CONNECTOR__" stroke-width="2.5" marker-end="url(#arrow)"/>

  <g>
    <rect x="130" y="1500" width="380" height="140" fill="__CARD__" stroke="__BORDER__" stroke-width="2"/>
    <rect x="130" y="1500" width="7" height="140" fill="__ACCENT__"/>
    <text x="156" y="1530" font-size="13" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1" fill="__ACCENT__">ESTIMATOR</text>
    <text x="156" y="1561" font-size="18" font-weight="650">Selected noise estimator</text>
    <text x="156" y="1588" font-size="15" fill="__QUIET__">MinimumNoise · MCRA · SPP-MMSE</text>
    <text x="156" y="1613" font-size="15" fill="__QUIET__">emits the selected noise PSD</text>
  </g>
  <g>
    <rect x="770" y="1500" width="380" height="140" fill="__CARD__" stroke="__BORDER__" stroke-width="2"/>
    <rect x="770" y="1500" width="7" height="140" fill="__ACCENT__"/>
    <text x="796" y="1530" font-size="13" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1" fill="__ACCENT__">TONAL DETECTOR</text>
    <text x="796" y="1561" font-size="18" font-weight="650">Reads original frame power</text>
    <text x="796" y="1588" font-size="15" fill="__QUIET__">tracks tonal transients</text>
    <text x="796" y="1613" font-size="15" fill="__QUIET__">emits a per-bin gain</text>
  </g>

  <path d="M320 1640V1685" fill="none" stroke="__CONNECTOR__" stroke-width="2.5" marker-end="url(#arrow)"/>
  <path d="M440 1348H95V1805Q95 1820 110 1820H130" fill="none" stroke="__CONNECTOR__" stroke-width="2.5" marker-end="url(#arrow)"/>

  <g>
    <rect x="130" y="1685" width="380" height="215" fill="__CARD__" stroke="__BORDER__" stroke-width="2"/>
    <rect x="130" y="1685" width="7" height="215" fill="__ACCENT__"/>
    <text x="156" y="1715" font-size="13" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1" fill="__ACCENT__">SHARED SUPPRESSION PATH</text>
    <text x="156" y="1747" font-size="18" font-weight="650">Decision-directed SNR</text>
    <text x="156" y="1775" font-size="18" font-weight="650">+ Log-MMSE</text>
    <text x="156" y="1810" font-size="15" fill="__QUIET__">uses observed frame power and</text>
    <text x="156" y="1834" font-size="15" fill="__QUIET__">the selected noise PSD</text>
    <text x="156" y="1870" font-size="14" fill="__QUIET__">active after the quiet-intro boundary</text>
  </g>

  <path d="M320 1900V1960Q320 1970 330 1970H500V2050" fill="none" stroke="__CONNECTOR__" stroke-width="2.5" marker-end="url(#arrow)"/>
  <path d="M960 1640V1960Q960 1970 950 1970H780V2050" fill="none" stroke="__CONNECTOR__" stroke-width="2.5" marker-end="url(#arrow)"/>

  <g>
    <rect x="440" y="2050" width="400" height="135" fill="__CARD__" stroke="__BORDER__" stroke-width="2"/>
    <rect x="440" y="2050" width="7" height="135" fill="__ACCENT__"/>
    <text x="466" y="2080" font-size="13" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1" fill="__ACCENT__">POST-SUPPRESSION GAIN</text>
    <text x="466" y="2112" font-size="18" font-weight="650">Apply tonal gain after Log-MMSE</text>
    <text x="466" y="2140" font-size="15" fill="__QUIET__">detector gain multiplies the suppressed</text>
    <text x="466" y="2163" font-size="15" fill="__QUIET__">complex spectrum before reconstruction</text>
  </g>
  <path d="M640 2185V2230" fill="none" stroke="__CONNECTOR__" stroke-width="2.5" marker-end="url(#arrow)"/>

  <g>
    <rect x="440" y="2230" width="400" height="105" fill="__CARD__" stroke="__BORDER__" stroke-width="2"/>
    <rect x="440" y="2230" width="7" height="105" fill="__ACCENT__"/>
    <text x="466" y="2258" font-size="13" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1" fill="__ACCENT__">RECONSTRUCTION</text>
    <text x="466" y="2287" font-size="19" font-weight="650">Inverse FFT + overlap-add</text>
    <text x="466" y="2314" font-size="15" fill="__QUIET__">restores the ordered sample stream</text>
  </g>
  <path d="M640 2335V2380" fill="none" stroke="__CONNECTOR__" stroke-width="2.5" marker-end="url(#arrow)"/>

  <g>
    <rect x="440" y="2380" width="400" height="130" fill="__CARD__" stroke="__BORDER__" stroke-width="2"/>
    <rect x="440" y="2380" width="7" height="130" fill="__ACCENT__"/>
    <text x="466" y="2410" font-size="13" font-family="ui-monospace, SFMono-Regular, Menlo, monospace" letter-spacing="1" fill="__ACCENT__">OUTPUT</text>
    <text x="466" y="2442" font-size="19" font-weight="650">Encode ordered μ-law bytes</text>
    <text x="466" y="2470" font-size="15" fill="__QUIET__">0–256 bytes per push; drain writes</text>
    <text x="466" y="2493" font-size="15" fill="__QUIET__">up to 255 remaining valid samples</text>
  </g>
</g>
</svg>
"#;

fn render(palette: Palette) -> String {
    let mut svg = DIAGRAM.to_owned();
    for (token, color) in [
        ("__CANVAS__", palette.canvas),
        ("__GRID__", palette.grid),
        ("__FRAME__", palette.frame),
        ("__CARD__", palette.card),
        ("__BORDER__", palette.border),
        ("__ACCENT__", palette.accent),
        ("__TEXT__", palette.text),
        ("__QUIET__", palette.quiet),
        ("__CONNECTOR__", palette.connector),
    ] {
        svg = svg.replace(token, color);
    }
    svg
}

fn main() -> std::io::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = root.join("docs/images");
    fs::create_dir_all(&output)?;
    for (name, palette) in [("how-it-works-day.svg", DAY), ("how-it-works-night.svg", NIGHT)] {
        fs::write(output.join(name), render(palette))?;
    }
    Ok(())
}
