//! Command-line arguments.

/// Most nodes `--nodes` accepts.
const MAX_NODES: usize = 1_000_000;

/// The `--nodes N` value from `args` (program name first); 13 if absent.
pub fn parse_nodes(args: &[String]) -> Result<usize, String> {
    let Some(pos) = args.iter().position(|a| a == "--nodes") else {
        return Ok(13);
    };
    let Some(value) = args.get(pos + 1) else {
        return Err("--nodes needs a number".to_string());
    };
    let n: usize = value
        .parse()
        .map_err(|_| format!("--nodes: `{value}` is not a whole number"))?;
    if n == 0 || n > MAX_NODES {
        return Err(format!(
            "--nodes must be between 1 and {MAX_NODES}, got {n}"
        ));
    }
    Ok(n)
}
