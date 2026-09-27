//! Command-line parsing (std only) and dispatch.

use std::path::PathBuf;

use crate::{
    bench::{BenchOptions, bench, default_concurrent_calls},
    compare::{DEFAULT_SPLIT, compare},
    error::EvalError,
    heap::HeapCounter,
    replay::{ReplayOptions, replay},
};

const USAGE: &str = "\
usage: noise-oxydation-eval <command> [options]

commands:
  replay  [--sources DIR] [--out DIR]
          Build the real-speech scenarios from the fetched sources (default audio/sources), enhance them with every
          evaluated configuration, and write WAV files, headerless mu-law files and metrics.json (default audio/out).
  compare REFERENCE CANDIDATE [--split SAMPLE]
          Compare two equal-length headerless mu-law files, overall and split into calibration and enhanced regions
          (default split 39808, the first output sample influenced by an enhanced frame with 5 s calibration).
  bench   [--input FILE] [--passes N] [--calls N[,N...]] [--call-seconds S]
          Measure heap activity, per-call memory, process_packet latency and concurrent-call throughput on a headerless
          mu-law input (default audio/out/office-5db/noisy.ul, 20 passes, 1, all cores and 100 calls of 120 s).
  help    Print this message.";

/// Runs one command. `heap` reads the binary's per-thread allocation counters for `bench`.
///
/// # Errors
///
/// Returns an [`EvalError`] when the command line is invalid or the workflow fails.
pub fn run(arguments: Vec<String>, heap: &dyn HeapCounter) -> Result<(), EvalError> {
    let mut arguments = arguments.into_iter();
    let command = arguments.next();
    let mut parsed = Arguments::parse(arguments)?;
    match command.as_deref() {
        Some("replay") => {
            let options = ReplayOptions {
                sources: parsed.path("--sources", "audio/sources"),
                out: parsed.path("--out", "audio/out"),
            };
            parsed.finish(0)?;
            replay(&options)
        }
        Some("compare") => {
            let split = parsed.number("--split", DEFAULT_SPLIT)?;
            let [reference, candidate] = parsed.finish(2)?.try_into().map_err(|_| usage("compare needs two files"))?;
            compare(&PathBuf::from(reference), &PathBuf::from(candidate), split)
        }
        Some("bench") => {
            let options = BenchOptions {
                input: parsed.path("--input", "audio/out/office-5db/noisy.ul"),
                passes: parsed.number("--passes", 20)?.max(1),
                concurrent_calls: parsed.list("--calls")?.unwrap_or_else(default_concurrent_calls),
                call_seconds: parsed.number("--call-seconds", 120)?.max(1),
            };
            parsed.finish(0)?;
            bench(&options, heap)
        }
        Some("help" | "--help" | "-h") => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(usage(&format!("unknown command `{other}`"))),
        None => Err(usage("missing command")),
    }
}

fn usage(problem: &str) -> EvalError {
    EvalError::Usage(format!("{problem}\n\n{USAGE}"))
}

/// Options (`--name value`) and positional arguments.
struct Arguments {
    options: Vec<(String, String)>,
    positional: Vec<String>,
}

impl Arguments {
    fn parse(arguments: impl Iterator<Item = String>) -> Result<Self, EvalError> {
        let mut parsed = Self { options: Vec::new(), positional: Vec::new() };
        let mut arguments = arguments.peekable();
        while let Some(argument) = arguments.next() {
            if argument.starts_with("--") {
                let value = arguments.next().ok_or_else(|| usage(&format!("{argument} needs a value")))?;
                parsed.options.push((argument, value));
            } else {
                parsed.positional.push(argument);
            }
        }
        Ok(parsed)
    }

    fn take(&mut self, name: &str) -> Option<String> {
        let index = self.options.iter().position(|(option, _)| option == name)?;
        Some(self.options.remove(index).1)
    }

    fn path(&mut self, name: &str, default: &str) -> PathBuf {
        PathBuf::from(self.take(name).unwrap_or_else(|| default.to_owned()))
    }

    fn number(&mut self, name: &str, default: usize) -> Result<usize, EvalError> {
        self.take(name).map_or(Ok(default), |value| {
            value.parse().map_err(|_| usage(&format!("{name} expects a non-negative integer, got `{value}`")))
        })
    }

    fn list(&mut self, name: &str) -> Result<Option<Vec<usize>>, EvalError> {
        self.take(name)
            .map(|value| {
                value
                    .split(',')
                    .map(|item| {
                        item.parse::<usize>()
                            .ok()
                            .filter(|&count| count > 0)
                            .ok_or_else(|| usage(&format!("{name} expects positive integers, got `{value}`")))
                    })
                    .collect()
            })
            .transpose()
    }

    /// Rejects unknown options and returns exactly `positional` positional arguments.
    fn finish(self, positional: usize) -> Result<Vec<String>, EvalError> {
        if let Some((option, _)) = self.options.first() {
            return Err(usage(&format!("unknown option {option}")));
        }
        if self.positional.len() != positional {
            return Err(usage(&format!("expected {positional} positional argument(s), got {}", self.positional.len())));
        }
        Ok(self.positional)
    }
}

#[cfg(test)]
#[path = "../tests/unit/cli.rs"]
mod tests;
