//! Enhances a headerless 8 kHz G.711 μ-law file packet by packet, as a call loop would.
//!
//! ```text
//! cargo run --locked --release -p noise-oxydation --example enhance_mulaw -- <input.ul> <output.ul>
//! ```
//!
//! A final partial packet is padded with μ-law silence (0xFF) and reported; the output then contains the padded
//! packet in full.

use std::{env, fs, path::Path, process::ExitCode};

use noise_oxydation::{CallConfig, CallEnhancer, DELAY_PACKETS, PACKET_SAMPLES, Packet, PacketOutcome};

/// μ-law code of a zero sample.
const SILENCE: u8 = 0xFF;

fn main() -> ExitCode {
    let mut arguments = env::args_os().skip(1);
    let (Some(input), Some(output), None) = (arguments.next(), arguments.next(), arguments.next()) else {
        eprintln!("usage: enhance_mulaw <input.ul> <output.ul>");
        return ExitCode::from(2);
    };
    match run(Path::new(&input), Path::new(&output)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("enhance_mulaw: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(input_path: &Path, output_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let input = fs::read(input_path)?;
    let chunks = input.chunks(PACKET_SAMPLES);
    let padding = chunks.clone().last().map_or(0, |last| PACKET_SAMPLES - last.len());
    if padding > 0 {
        println!("padded the final partial packet with {padding} silence bytes");
    }

    let mut enhancer = CallEnhancer::new(&CallConfig::default())?;
    let mut output = Vec::with_capacity(input.len() + padding);
    let mut packet_out: Packet = [0; PACKET_SAMPLES];
    let mut packets_in = 0_usize;
    for chunk in chunks {
        let mut packet: Packet = [SILENCE; PACKET_SAMPLES];
        packet[..chunk.len()].copy_from_slice(chunk);
        if enhancer.process_packet(&packet, &mut packet_out)? == PacketOutcome::Emitted {
            output.extend_from_slice(&packet_out);
        }
        packets_in += 1;
    }
    let mut tail = [[0; PACKET_SAMPLES]; DELAY_PACKETS];
    let withheld = enhancer.drain(&mut tail)?;
    output.extend(tail[..withheld].iter().flatten());

    fs::write(output_path, &output)?;
    println!("input packets: {packets_in}");
    println!("output packets: {}", output.len() / PACKET_SAMPLES);
    Ok(())
}
