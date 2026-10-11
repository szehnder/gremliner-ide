//! `gremliner-cli lint|fmt|parse`: the language service on files or stdin.

use std::io::Read as _;
use std::path::PathBuf;

use anyhow::{Context, Result};
use gremlin_syntax::{Document, Severity, StepNode, format, line_col};
use serde_json::json;

/// Read `path`, or stdin for `-`/none.
fn read(path: Option<&PathBuf>) -> Result<(String, String)> {
    match path {
        Some(p) if p.as_os_str() != "-" => Ok((
            p.display().to_string(),
            std::fs::read_to_string(p).with_context(|| format!("cannot read {}", p.display()))?,
        )),
        _ => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text)?;
            Ok(("<stdin>".into(), text))
        }
    }
}

/// Prints diagnostics; returns the process exit code (1 when there are errors, or warnings with `deny`).
pub fn lint(path: Option<&PathBuf>, deny_warnings: bool) -> Result<i32> {
    let (name, text) = read(path)?;
    let diagnostics = Document::parse(text.as_str()).diagnostics();
    let mut failed = false;
    for d in &diagnostics {
        let (line, col) = line_col(&text, d.range.start);
        let level = match d.severity {
            Severity::Error => {
                failed = true;
                "error"
            }
            Severity::Warning => {
                failed |= deny_warnings;
                "warning"
            }
        };
        println!("{name}:{line}:{col}: {level}: {}", d.message);
    }
    Ok(i32::from(failed))
}

pub fn fmt(path: Option<&PathBuf>, width: usize, write: bool, check: bool) -> Result<i32> {
    let (name, text) = read(path)?;
    let formatted = format(&text, width);
    if check {
        if formatted != text {
            eprintln!("{name} is not formatted");
            return Ok(1);
        }
        return Ok(0);
    }
    match path {
        Some(p) if write && p.as_os_str() != "-" => std::fs::write(p, formatted)?,
        _ => print!("{formatted}"),
    }
    Ok(0)
}

fn step_json(step: &StepNode) -> serde_json::Value {
    json!({
        "id": step.id,
        "name": step.name,
        "span": [step.span.start, step.span.end],
        "depth": step.depth,
        "children": step.children.iter().map(step_json).collect::<Vec<_>>(),
    })
}

pub fn parse(path: Option<&PathBuf>, json: bool) -> Result<i32> {
    let (_, text) = read(path)?;
    let doc = Document::parse(text.as_str());
    if json {
        let statements: Vec<_> = doc
            .analyze_all()
            .iter()
            .map(|a| {
                json!({
                    "text": a.statement.text,
                    "range": [a.statement.range.start, a.statement.range.end],
                    "has_error": a.statement.has_error,
                    "mutating": a.is_mutating,
                    "parameters": a.parameters,
                    "steps": a.steps.iter().map(step_json).collect::<Vec<_>>(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "statements": statements }))?
        );
    } else {
        println!("{}", doc.tree().root_node().to_sexp());
    }
    Ok(0)
}
