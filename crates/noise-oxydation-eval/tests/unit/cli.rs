use super::{Arguments, run};
use crate::{error::EvalError, heap::NoHeapCounter};

fn arguments(values: &[&str]) -> Arguments {
    Arguments::parse(values.iter().map(|value| (*value).to_owned())).expect("parses")
}

#[test]
fn options_and_positional_arguments_are_separated() {
    let mut parsed = arguments(&["a.ul", "--split", "123", "b.ul", "--calls", "1,4,100"]);
    assert_eq!(parsed.number("--split", 0).expect("number"), 123);
    assert_eq!(parsed.list("--calls").expect("list"), Some(vec![1, 4, 100]));
    assert_eq!(parsed.path("--out", "audio/out"), std::path::PathBuf::from("audio/out"));
    assert_eq!(parsed.finish(2).expect("two files"), ["a.ul", "b.ul"]);
}

#[test]
fn invalid_command_lines_are_usage_errors() {
    let usage = |result: Result<(), EvalError>| matches!(result, Err(EvalError::Usage(_)));
    let run = |values: &[&str]| run(values.iter().map(|value| (*value).to_owned()).collect(), &NoHeapCounter);
    assert!(usage(run(&[])));
    assert!(usage(run(&["unknown"])));
    assert!(usage(run(&["compare", "only-one.ul"])));
    assert!(usage(run(&["replay", "--frobnicate", "1"])));
    assert!(usage(run(&["bench", "--passes"])));
    assert!(usage(run(&["bench", "--passes", "many"])));
    assert!(usage(run(&["bench", "--calls", "1,0"])));
    assert!(run(&["help"]).is_ok());
}
