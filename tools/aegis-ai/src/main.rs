//! CLI entry point for the `aegis-ai` subproject (Tasks AI-05, AI-11, AI-12).
//!
//! Subcommands:
//! - `generate [--out-dir <DIR>]`: Compiles and verifies all Gear 1, Gear 2 SFT,
//!   and DPO training/validation/test JSONL splits.
//! - `stats`: Compiles the dataset in memory and prints task, language, and
//!   node-kind counts.
//! - `verify-dsl <DSL>`: Parses and validates a candidate model completion
//!   through `aegis-model` (`AttackTree`, `evaluate(MaxMin)`, and `Project::lint`).
//! - `predict [OPTIONS] <CONTEXT_TEXT>`: Runs the pure-Rust Gear 1 inference
//!   engine and Best-of-N verifier filter to suggest ghost-node completions.
//! - `eval [--model <PATH>]`: Benchmarks the pure-Rust Gear 1 engine on the
//!   held-out evaluation split (`valid + test`).

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use aegis_ai::dataset::CompiledDataset;
use aegis_ai::predictor::{Gear1Predictor, Gear1Query};
use aegis_ai::verifier::Verifier;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("generate") => cmd_generate(&args[1..]),
        Some("stats") => cmd_stats(),
        Some("verify-dsl") => cmd_verify_dsl(&args[1..]),
        Some("predict") => cmd_predict(&args[1..]),
        Some("eval") => cmd_eval(&args[1..]),
        _ => {
            print_usage();
            ExitCode::from(2)
        }
    }
}

fn cmd_generate(args: &[String]) -> ExitCode {
    let mut out_dir = PathBuf::from("tools/aegis-ai/data");
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--out-dir" && i + 1 < args.len() {
            out_dir = PathBuf::from(&args[i + 1]);
            i += 2;
        } else {
            eprintln!("unknown argument: {}", args[i]);
            print_usage();
            return ExitCode::from(2);
        }
    }

    let dataset = match CompiledDataset::build_verified() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: dataset verification failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = dataset.write_to_dir(&out_dir) {
        eprintln!(
            "error: failed to write dataset to {}: {e}",
            out_dir.display()
        );
        return ExitCode::FAILURE;
    }

    println!(
        "ok    generated and verified {} SFT examples (train={}, valid={}, test={}), {} Gear-1 structural rows, and {} DPO pairs in {}",
        dataset.stats.total_sft,
        dataset.stats.train_count,
        dataset.stats.valid_count,
        dataset.stats.test_count,
        dataset.stats.total_gear1,
        dataset.stats.total_dpo,
        out_dir.display()
    );
    ExitCode::SUCCESS
}

fn cmd_stats() -> ExitCode {
    let dataset = match CompiledDataset::build_verified() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let json = serde_json::to_string_pretty(&dataset.stats).unwrap_or_default();
    println!("{json}");
    ExitCode::SUCCESS
}

fn cmd_verify_dsl(args: &[String]) -> ExitCode {
    let Some(raw) = args.first() else {
        eprintln!("error: missing DSL string argument");
        print_usage();
        return ExitCode::from(2);
    };
    let normalized = raw.replace("\\n", "\n");
    let verifier = Verifier::iso21434_r155();

    if normalized.contains("<|threat|>") {
        match verifier.verify_threats_dsl(&normalized) {
            Ok(proposals) => {
                println!(
                    "ok    verified {} threat proposal(s) against aegis-model (iso21434-r155)",
                    proposals.len()
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("FAIL  {e}");
                ExitCode::FAILURE
            }
        }
    } else {
        match verifier.verify_tree_dsl(&normalized) {
            Ok(res) => {
                println!(
                    "ok    verified tree: nodes={}, steps={}, root_feasibility={:?}, why_path={:?}",
                    res.node_count, res.step_count, res.root_feasibility_key, res.why_path_labels
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("FAIL  {e}");
                ExitCode::FAILURE
            }
        }
    }
}

fn cmd_predict(args: &[String]) -> ExitCode {
    let mut model_path = PathBuf::from("tools/aegis-ai/artifacts/gear1_model.json");
    let mut interfaces = vec!["can".to_owned(), "ethernet".to_owned()];
    let mut stride = "tampering".to_owned();
    let mut parent_gate = "or".to_owned();
    let mut depth = 2usize;
    let mut sibling_index = 1usize;
    let mut context_text = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--model" if i + 1 < args.len() => {
                model_path = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            "--interfaces" if i + 1 < args.len() => {
                interfaces = args[i + 1]
                    .split(',')
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect();
                i += 2;
            }
            "--stride" if i + 1 < args.len() => {
                stride.clone_from(&args[i + 1]);
                i += 2;
            }
            "--parent-gate" if i + 1 < args.len() => {
                parent_gate.clone_from(&args[i + 1]);
                i += 2;
            }
            "--depth" if i + 1 < args.len() => {
                depth = args[i + 1].parse().unwrap_or(2);
                i += 2;
            }
            "--sibling-index" if i + 1 < args.len() => {
                sibling_index = args[i + 1].parse().unwrap_or(1);
                i += 2;
            }
            other if !other.starts_with("--") => {
                context_text = Some(other.to_owned());
                i += 1;
            }
            other => {
                eprintln!("unknown argument: {other}");
                print_usage();
                return ExitCode::from(2);
            }
        }
    }

    let Some(context_text) = context_text else {
        eprintln!("error: missing <CONTEXT_TEXT> argument for `predict`");
        print_usage();
        return ExitCode::from(2);
    };

    let predictor = match Gear1Predictor::load_from_path(&model_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "error: failed to load Gear 1 model from {}: {e}",
                model_path.display()
            );
            return ExitCode::FAILURE;
        }
    };

    let query = Gear1Query {
        lang: "en".into(),
        interfaces,
        stride,
        parent_gate,
        depth,
        sibling_index,
        context_text,
    };
    let verifier = Verifier::iso21434_r155();
    match predictor.predict(&query, &verifier) {
        Ok(pred) => {
            let json = serde_json::to_string_pretty(&pred).unwrap_or_default();
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("FAIL  {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_eval(args: &[String]) -> ExitCode {
    let mut model_path = PathBuf::from("tools/aegis-ai/artifacts/gear1_model.json");
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--model" && i + 1 < args.len() {
            model_path = PathBuf::from(&args[i + 1]);
            i += 2;
        } else {
            eprintln!("unknown argument: {}", args[i]);
            print_usage();
            return ExitCode::from(2);
        }
    }

    let predictor = match Gear1Predictor::load_from_path(&model_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "error: failed to load Gear 1 model from {}: {e}",
                model_path.display()
            );
            return ExitCode::FAILURE;
        }
    };

    let dataset = match CompiledDataset::build_verified() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: dataset verification failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Filter to the held-out evaluation split (valid + test: first byte >= 204)
    let eval_examples: Vec<_> = dataset
        .gear1_examples
        .into_iter()
        .filter(|ex| {
            let b = u8::from_str_radix(ex.id.get(0..2).unwrap_or("00"), 16).unwrap_or(0);
            b >= 204
        })
        .collect();

    let verifier = Verifier::iso21434_r155();
    let report = predictor.evaluate_split(&eval_examples, &verifier);

    println!(
        "ok    evaluated Gear-1 Rust engine on {} held-out examples (avg CPU latency: {:.2} us/query, verifier pass rate: {:.1}%)",
        report.num_examples,
        report.avg_latency_us,
        report.verifier_pass_rate * 100.0
    );
    for (head, m) in &report.heads {
        println!(
            "      head {:<20} classes={:<4} eval_top1={:5.1}%  eval_top3={:5.1}%",
            head,
            m.num_classes,
            m.top1 * 100.0,
            m.top3 * 100.0
        );
    }
    ExitCode::SUCCESS
}

fn print_usage() {
    eprintln!(
        "Usage:
  aegis-ai generate [--out-dir <DIR>]
  aegis-ai stats
  aegis-ai verify-dsl <DSL_STRING>
  aegis-ai predict [--model <PATH>] [--interfaces <CSV>] [--stride <STRIDE>] [--parent-gate <GATE>] [--depth <N>] [--sibling-index <N>] <CONTEXT_TEXT>
  aegis-ai eval [--model <PATH>]"
    );
}
