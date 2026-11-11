//! Command-line arguments.

/// Most nodes `--nodes` accepts.
const MAX_NODES: usize = 1_000_000;
/// Most frames `--frames` accepts.
const MAX_FRAMES: usize = 10_000;

/// The parsed command line.
#[derive(Debug, PartialEq, Eq)]
pub struct Args {
    pub nodes: usize,
    pub measure: bool,
    pub frames: usize,
}

/// The value after `flag` as a whole number in `1..=max`, or `default` if the flag is absent.
fn number(args: &[String], flag: &str, default: usize, max: usize) -> Result<usize, String> {
    let Some(pos) = args.iter().position(|a| a == flag) else {
        return Ok(default);
    };
    let Some(value) = args.get(pos + 1) else {
        return Err(format!("{flag} needs a number"));
    };
    let n: usize = value
        .parse()
        .map_err(|_| format!("{flag}: `{value}` is not a whole number"))?;
    if n == 0 || n > max {
        return Err(format!("{flag} must be between 1 and {max}, got {n}"));
    }
    Ok(n)
}

/// The arguments in `args` (program name first).
pub fn parse_args(args: &[String]) -> Result<Args, String> {
    Ok(Args {
        nodes: number(args, "--nodes", 13, MAX_NODES)?,
        measure: args.iter().any(|a| a == "--measure"),
        frames: number(args, "--frames", 300, MAX_FRAMES)?,
    })
}
