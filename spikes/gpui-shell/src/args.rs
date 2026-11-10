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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn default_is_13() {
        assert_eq!(parse_nodes(&args(&["spike"])), Ok(13));
    }

    #[test]
    fn reads_the_count() {
        assert_eq!(
            parse_nodes(&args(&["spike", "--nodes", "100000"])),
            Ok(100_000)
        );
    }

    #[test]
    fn refuses_zero_words_and_too_many() {
        let cases = [
            ("0", "between 1 and 1000000, got 0"),
            ("many", "`many` is not a whole number"),
            ("1000001", "between 1 and 1000000, got 1000001"),
        ];
        for (bad, why) in cases {
            let err = parse_nodes(&args(&["spike", "--nodes", bad])).unwrap_err();
            assert!(err.contains(why), "{bad}: {err}");
        }
        assert_eq!(
            parse_nodes(&args(&["spike", "--nodes"])),
            Err("--nodes needs a number".to_string())
        );
    }
}
