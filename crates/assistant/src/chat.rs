//! The assistant panel's conversations (spec assistant-panel): a session per tab with modes (chat,
//! explain, fix, optimize, describe), read-only tools for capable models, and verified Gremlin code
//! blocks. Everything the model sees is chosen by the context the caller passes (the panel's chips).

use std::time::Duration;

use futures::future::BoxFuture;
use gremlin_model::{GValue, Metrics, normalize, plain};
use gremlin_syntax::Document;
use llm::{ChatRequest, ChatResponse, LlmError, Message, OutputSchema, Part, Role, ToolSpec};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::check::{Issue, check};
use crate::context::{Context, compact_schema, system_prompt};
use crate::dryrun::{DEFAULT_LIMIT, DEFAULT_TIMEOUT, DryRunner, inject_limit};
use crate::pipeline::{self, DryRun, Generator, Options, PipelineEvent};

/// Tool calls allowed per answer.
pub const MAX_TOOL_CALLS: usize = 6;
/// Rows compared by the optimize equivalence check.
pub const EQUIVALENCE_SAMPLE: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Chat,
    Explain,
    Fix,
    Optimize,
    Describe,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Chat => "Chat",
            Mode::Explain => "Explain query",
            Mode::Fix => "Fix error",
            Mode::Optimize => "Optimize",
            Mode::Describe => "Describe result",
        }
    }
}

/// The current result, summarised for "Describe result".
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResultSummary {
    pub count: usize,
    pub shape: String,
    /// Element labels and how many of each.
    pub labels: Vec<(String, usize)>,
    pub columns: Vec<String>,
    /// The first rows as JSON, only filled when sample sharing is allowed.
    pub rows: Vec<String>,
}

impl ResultSummary {
    /// Summarise `items`; rows are included only with `share_samples` (ADR-0021).
    pub fn of(items: &[GValue], share_samples: bool) -> Self {
        let shape = format!("{:?}", normalize::shape(items)).to_lowercase();
        let set = gremlin_model::elements::elements(items);
        let mut labels: Vec<(String, usize)> = Vec::new();
        for l in set
            .vertices
            .iter()
            .map(|v| &v.label)
            .chain(set.edges.iter().map(|e| &e.label))
        {
            match labels.iter_mut().find(|(n, _)| n == l) {
                Some((_, c)) => *c += 1,
                None => labels.push((l.clone(), 1)),
            }
        }
        let table = normalize::table(&items[..items.len().min(200)]);
        let columns = table
            .groups
            .first()
            .map(|g| g.columns.clone())
            .unwrap_or_default();
        let rows = if share_samples {
            items
                .iter()
                .take(5)
                .map(|v| plain::to_json(v).to_string())
                .collect()
        } else {
            Vec::new()
        };
        Self {
            count: items.len(),
            shape,
            labels,
            columns,
            rows,
        }
    }

    pub fn text(&self) -> String {
        let mut s = format!("{} items, shape: {}.", self.count, self.shape);
        if !self.labels.is_empty() {
            let l: Vec<String> = self
                .labels
                .iter()
                .map(|(n, c)| format!("{n} ({c})"))
                .collect();
            s.push_str(&format!("\nElement labels: {}.", l.join(", ")));
        }
        if !self.columns.is_empty() {
            s.push_str(&format!("\nColumns: {}.", self.columns.join(", ")));
        }
        if !self.rows.is_empty() {
            s.push_str(&format!("\nFirst rows:\n{}", self.rows.join("\n")));
        }
        s
    }
}

/// What the user chose to send (the chips) plus the turn's text.
#[derive(Debug, Clone, Default)]
pub struct TurnInput {
    pub mode: Option<Mode>,
    pub question: String,
    pub query: Option<String>,
    pub error: Option<String>,
    pub result: Option<ResultSummary>,
    /// Send tool definitions (the profile supports tool calling).
    pub tools: bool,
}

/// A verified Gremlin block from an answer.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeBlock {
    pub query: String,
    pub issues: Vec<Issue>,
    pub dry_run: DryRun,
    pub writes: bool,
    /// Optimize: the original and this query return the same count on the sample.
    pub equivalent: Option<bool>,
}

impl CodeBlock {
    pub fn verified(&self) -> bool {
        self.issues.is_empty() && !matches!(self.dry_run, DryRun::Failed { .. })
    }
}

/// One explanation item and the steps it covers (byte ranges in the explained statement).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplainItem {
    pub text: String,
    pub steps: Vec<usize>,
    pub spans: Vec<std::ops::Range<usize>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TurnEvent {
    Status(String),
    /// Streamed answer text.
    Text(String),
    Tool {
        name: String,
        input: String,
        summary: String,
    },
    Block(CodeBlock),
    Explain(Vec<ExplainItem>),
    Error(String),
    Done,
}

/// A conversation: what the model has seen so far.
#[derive(Debug, Clone, Default)]
pub struct Session {
    pub history: Vec<Message>,
}

impl Session {
    pub fn clear(&mut self) {
        self.history.clear();
    }
}

/// Streams text deltas when the generator can.
pub trait StreamingGenerator: Generator {
    fn generate_streaming<'a>(
        &'a self,
        req: &'a ChatRequest,
        on_text: &'a (dyn Fn(&str) + Send + Sync),
    ) -> BoxFuture<'a, Result<ChatResponse, LlmError>> {
        Box::pin(async move {
            let resp = self.generate(req).await?;
            if !resp.text.is_empty() {
                on_text(&resp.text);
            }
            Ok(resp)
        })
    }
}

impl StreamingGenerator for llm::LlmClient {
    fn generate_streaming<'a>(
        &'a self,
        req: &'a ChatRequest,
        on_text: &'a (dyn Fn(&str) + Send + Sync),
    ) -> BoxFuture<'a, Result<ChatResponse, LlmError>> {
        Box::pin(async move {
            use futures::StreamExt as _;
            let mut stream = self.stream(req).await?;
            let mut out = ChatResponse::default();
            while let Some(event) = stream.next().await {
                let event = event?;
                if let llm::ChatEvent::TextDelta(t) = &event {
                    on_text(t);
                }
                out.push(event);
            }
            Ok(out)
        })
    }
}

// ---- tools ---------------------------------------------------------------------------------------

pub fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "get_schema".into(),
            description: "The connected graph's labels, property keys with types, counts and edge connections.".into(),
            parameters: json!({"type": "object", "properties": {}}),
        },
        ToolSpec {
            name: "sample_values".into(),
            description: "A few distinct values of a property on a label (only when the user allows sample data).".into(),
            parameters: json!({"type": "object", "properties": {"label": {"type": "string"}, "key": {"type": "string"}}, "required": ["label", "key"]}),
        },
        ToolSpec {
            name: "validate_query".into(),
            description: "Check a Gremlin query for syntax errors and labels, keys or edge directions not in the schema.".into(),
            parameters: json!({"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}),
        },
        ToolSpec {
            name: "dry_run".into(),
            description: "Run a read-only Gremlin query with a 25-row limit and a timeout; returns the row count (and rows when sample data is allowed). Queries that change data are refused.".into(),
            parameters: json!({"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}),
        },
    ]
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

/// Run one tool call; returns (content for the model, one-line summary for the transcript, is_error).
pub async fn run_tool(
    name: &str,
    args: &Value,
    ctx: &Context,
    runner: Option<&dyn DryRunner>,
) -> (String, String, bool) {
    let arg = |k: &str| {
        args.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    match name {
        "get_schema" => match &ctx.schema {
            Some(s) => {
                let text = compact_schema(s, ctx.share_samples);
                let summary = format!(
                    "{} vertex labels, {} edge labels",
                    s.vertex_labels.len(),
                    s.edge_labels.len()
                );
                (text, summary, false)
            }
            None => (
                "The schema is not available.".into(),
                "schema not available".into(),
                true,
            ),
        },
        "sample_values" => {
            if !ctx.share_samples {
                let m =
                    "Refused: sample data is not shared with the assistant for this connection.";
                return (m.into(), "refused: sample sharing is off".into(), true);
            }
            let (label, key) = (arg("label"), arg("key"));
            let script = format!(
                "g.V().hasLabel('{}').values('{}').dedup().limit(10)",
                escape(&label),
                escape(&key)
            );
            match runner {
                Some(r) => match r.run(&script, DEFAULT_TIMEOUT).await {
                    Ok((items, _)) => {
                        let values: Vec<String> = items
                            .iter()
                            .map(|v| plain::to_json(v).to_string())
                            .collect();
                        let summary = format!("{} values of {label}.{key}", values.len());
                        (values.join(", "), summary, false)
                    }
                    Err(e) => (format!("Error: {e}"), format!("error: {e}"), true),
                },
                None => ("Not connected.".into(), "not connected".into(), true),
            }
        }
        "validate_query" => {
            let query = arg("query");
            let issues = check(&query, ctx.schema.as_deref(), &ctx.dialect);
            if issues.is_empty() {
                ("Valid.".into(), "valid".into(), false)
            } else {
                let text: Vec<String> = issues.iter().map(|i| i.message.clone()).collect();
                (
                    text.join("\n"),
                    format!("{} problem(s)", issues.len()),
                    false,
                )
            }
        }
        "dry_run" => {
            let query = arg("query");
            // ADR-0020: never sent to the database.
            if gremlin_syntax::is_mutating(&query) {
                let m = "Refused: this query changes data. Tools only run read-only queries.";
                return (m.into(), "refused: the query changes data".into(), true);
            }
            let Some(r) = runner else {
                return ("Not connected.".into(), "not connected".into(), true);
            };
            let script = gremlin_syntax::ensure_terminal(&inject_limit(&query, DEFAULT_LIMIT));
            match r.run(&script, DEFAULT_TIMEOUT).await {
                Ok((items, elapsed)) => {
                    let summary = format!("{} rows in {} ms", items.len(), elapsed.as_millis());
                    let content = if ctx.share_samples {
                        let rows: Vec<String> = items
                            .iter()
                            .take(5)
                            .map(|v| plain::to_json(v).to_string())
                            .collect();
                        format!("{summary}. First rows:\n{}", rows.join("\n"))
                    } else {
                        format!("{summary}. (Row values are not shared for this connection.)")
                    };
                    (content, summary, false)
                }
                Err(e) => (
                    format!("The database returned an error: {e}"),
                    format!("error: {e}"),
                    true,
                ),
            }
        }
        other => (
            format!("Unknown tool {other}"),
            format!("unknown tool {other}"),
            true,
        ),
    }
}

// ---- code blocks ---------------------------------------------------------------------------------

/// The ```gremlin (or unlabelled `g.`) fenced blocks in `text`.
pub fn code_blocks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("```") {
        let after = &rest[start + 3..];
        let Some(nl) = after.find('\n') else { break };
        let lang = after[..nl].trim().to_lowercase();
        let body = &after[nl + 1..];
        let Some(end) = body.find("```") else { break };
        let code = body[..end].trim();
        if lang == "gremlin" || lang == "groovy" || (lang.is_empty() && code.starts_with("g.")) {
            out.push(code.to_owned());
        }
        rest = &body[end + 3..];
    }
    out
}

/// Validate and (read-only, allowed) dry-run a block.
pub async fn verify_block(
    query: &str,
    ctx: &Context,
    runner: Option<&dyn DryRunner>,
    dry_run: bool,
) -> CodeBlock {
    let issues = check(query, ctx.schema.as_deref(), &ctx.dialect);
    let writes = gremlin_syntax::is_mutating(query);
    let dry_run = if !issues.is_empty() {
        DryRun::Skipped {
            reason: "did not validate".into(),
        }
    } else if writes {
        DryRun::Skipped {
            reason: "writes data".into(),
        }
    } else {
        match (dry_run, runner) {
            (true, Some(r)) => {
                let script = gremlin_syntax::ensure_terminal(&inject_limit(query, DEFAULT_LIMIT));
                match r.run(&script, DEFAULT_TIMEOUT).await {
                    Ok((items, elapsed)) => DryRun::Passed {
                        rows: items.len(),
                        elapsed,
                    },
                    Err(message) => DryRun::Failed { message },
                }
            }
            _ => DryRun::Skipped {
                reason: "dry runs are off".into(),
            },
        }
    };
    CodeBlock {
        query: query.to_owned(),
        issues,
        dry_run,
        writes,
        equivalent: None,
    }
}

// ---- explain -------------------------------------------------------------------------------------

/// The steps of `query`'s first statement, numbered in order (nested steps included), with spans.
pub fn numbered_steps(query: &str) -> Vec<(String, std::ops::Range<usize>)> {
    let doc = Document::parse(query);
    let Some(stmt) = doc.statements().into_iter().next() else {
        return Vec::new();
    };
    let mut steps: Vec<(String, std::ops::Range<usize>)> = doc
        .steps(&stmt)
        .iter()
        .flat_map(|s| s.walk())
        .map(|s| (query[s.span.clone()].to_owned(), s.span.clone()))
        .collect();
    steps.sort_by_key(|(_, r)| r.start);
    steps
}

#[derive(Deserialize)]
struct ExplainJson {
    items: Vec<ExplainJsonItem>,
}

#[derive(Deserialize)]
struct ExplainJsonItem {
    #[serde(default)]
    steps: Vec<usize>,
    text: String,
}

fn explain_schema() -> OutputSchema {
    OutputSchema {
        name: "explanation".into(),
        schema: json!({
            "type": "object",
            "properties": {"items": {"type": "array", "items": {"type": "object",
                "properties": {"steps": {"type": "array", "items": {"type": "integer"}}, "text": {"type": "string"}},
                "required": ["steps", "text"]}}},
            "required": ["items"],
        }),
    }
}

/// Map the model's explanation onto step spans; `None` when it is not usable JSON.
pub fn parse_explanation(
    resp: &ChatResponse,
    steps: &[(String, std::ops::Range<usize>)],
) -> Option<Vec<ExplainItem>> {
    let v = resp.output_json("explanation")?;
    let parsed: ExplainJson = serde_json::from_value(v).ok()?;
    let items: Vec<ExplainItem> = parsed
        .items
        .into_iter()
        .map(|i| {
            let valid: Vec<usize> = i
                .steps
                .into_iter()
                .filter(|n| *n >= 1 && *n <= steps.len())
                .collect();
            ExplainItem {
                spans: valid.iter().map(|n| steps[n - 1].1.clone()).collect(),
                steps: valid,
                text: i.text,
            }
        })
        .collect();
    (!items.is_empty()).then_some(items)
}

// ---- prompts -------------------------------------------------------------------------------------

const PANEL_RULES: &str = "\
You are a Gremlin expert inside a graph database IDE. Answer concisely in Markdown. Put every Gremlin query in a ```gremlin fenced block: one traversal per block, starting with g., no lambdas. Use only labels, keys and edge directions from the schema. Never suggest running queries that change data unless the user asks; never claim you ran anything you did not run with a tool.";

/// The system prompt for a mode, honouring the chips (`ctx.schema` is `None` when the schema chip is off).
pub fn mode_system(mode: Mode, ctx: &Context) -> String {
    let schema = match &ctx.schema {
        Some(s) => format!(
            "\nSchema of the connected graph:\n{}",
            compact_schema(s, ctx.share_samples)
        ),
        None => String::new(),
    };
    match mode {
        Mode::Explain => format!(
            "You are a Gremlin expert inside a graph database IDE. Explain Gremlin traversals step by step for a developer: what each step does to the traversers and what comes out. Answer with JSON only: {{\"items\": [{{\"steps\": [1, 2], \"text\": \"...\"}}]}} where steps are the numbers of the steps an item explains, in order, one item per step or small group of steps.{schema}"
        ),
        Mode::Chat | Mode::Describe => format!("{PANEL_RULES}{schema}"),
        // Fix and Optimize go through the verified pipeline, whose prompt is `system_prompt`.
        Mode::Fix | Mode::Optimize => system_prompt(ctx),
    }
}

fn user_text(input: &TurnInput) -> String {
    let mut s = String::new();
    if let Some(q) = &input.query {
        s.push_str(&format!("Current query:\n```gremlin\n{q}\n```\n"));
    }
    if let Some(e) = &input.error {
        s.push_str(&format!("Last error:\n{e}\n"));
    }
    if let Some(r) = &input.result {
        s.push_str(&format!("Current result: {}\n", r.text()));
    }
    s.push_str(&input.question);
    s
}

/// Profile `query` (read-only) and summarise its slowest steps.
pub async fn profile_summary(query: &str, runner: &dyn DryRunner) -> Option<String> {
    if gremlin_syntax::is_mutating(query) {
        return None;
    }
    let base = query
        .trim()
        .trim_end_matches(';')
        .trim_end_matches(".toList()")
        .trim_end_matches(".iterate()");
    let (items, _) = runner
        .run(&format!("{base}.profile()"), Duration::from_secs(10))
        .await
        .ok()?;
    let metrics = items.iter().find_map(|v| match v {
        GValue::TraversalMetrics(m) | GValue::Metrics(m) => Some(m.clone()),
        _ => None,
    })?;
    Some(metrics_text(&metrics))
}

/// `Step (12.3 ms, 64%, 98 traversers)` lines, slowest first.
pub fn metrics_text(m: &Metrics) -> String {
    let total: f64 = m
        .metrics
        .iter()
        .map(|s| s.dur_ms)
        .sum::<f64>()
        .max(m.dur_ms)
        .max(f64::EPSILON);
    let mut steps: Vec<&Metrics> = m.metrics.iter().collect();
    steps.sort_by(|a, b| b.dur_ms.total_cmp(&a.dur_ms));
    let lines: Vec<String> = steps
        .iter()
        .map(|s| {
            let traversers = s.counts.get("traverserCount").copied().unwrap_or_default();
            format!(
                "- {} ({:.2} ms, {:.0}%, {traversers} traversers)",
                s.name.as_deref().unwrap_or("?"),
                s.dur_ms,
                s.dur_ms / total * 100.0
            )
        })
        .collect();
    format!(
        "Total {:.2} ms. Steps, slowest first:\n{}",
        m.dur_ms,
        lines.join("\n")
    )
}

/// Whether `a` and `b` return the same number of results on the first `k`.
pub async fn equivalent_on_sample(
    a: &str,
    b: &str,
    runner: &dyn DryRunner,
    k: usize,
) -> Option<bool> {
    let count = |q: &str| {
        let base = q
            .trim()
            .trim_end_matches(';')
            .trim_end_matches(".toList()")
            .to_owned();
        format!("{base}.limit({k}).count()")
    };
    let (ra, _) = runner.run(&count(a), DEFAULT_TIMEOUT).await.ok()?;
    let (rb, _) = runner.run(&count(b), DEFAULT_TIMEOUT).await.ok()?;
    Some(ra.first()?.as_i64()? == rb.first()?.as_i64()?)
}

// ---- turns ---------------------------------------------------------------------------------------

/// Answer one turn in `session`, emitting events as they happen.
pub async fn turn(
    session: &mut Session,
    input: &TurnInput,
    ctx: &Context,
    generator: &dyn StreamingGenerator,
    runner: Option<&dyn DryRunner>,
    dry_runs: bool,
    emit: &(dyn Fn(TurnEvent) + Send + Sync),
) {
    let mode = input.mode.unwrap_or(Mode::Chat);
    match mode {
        Mode::Explain => explain(session, input, ctx, generator, emit).await,
        Mode::Fix | Mode::Optimize => {
            verified(session, mode, input, ctx, generator, runner, dry_runs, emit).await
        }
        Mode::Chat | Mode::Describe => {
            chat(session, mode, input, ctx, generator, runner, dry_runs, emit).await
        }
    }
    emit(TurnEvent::Done);
}

async fn explain(
    session: &mut Session,
    input: &TurnInput,
    ctx: &Context,
    generator: &dyn StreamingGenerator,
    emit: &(dyn Fn(TurnEvent) + Send + Sync),
) {
    let Some(query) = input.query.clone() else {
        emit(TurnEvent::Error("There is no query to explain.".into()));
        return;
    };
    let steps = numbered_steps(&query);
    let numbered: Vec<String> = steps
        .iter()
        .enumerate()
        .map(|(i, (t, _))| format!("{}. {t}", i + 1))
        .collect();
    let prompt = format!(
        "Explain this query:\n```gremlin\n{query}\n```\nIts steps, numbered:\n{}\n{}",
        numbered.join("\n"),
        input.question
    );
    let req = ChatRequest {
        system: Some(mode_system(Mode::Explain, ctx)),
        messages: session
            .history
            .iter()
            .cloned()
            .chain([Message::user(prompt.clone())])
            .collect(),
        output_schema: Some(explain_schema()),
        ..ChatRequest::default()
    };
    emit(TurnEvent::Status("Explaining\u{2026}".into()));
    match generator.generate(&req).await {
        Ok(resp) => {
            session.history.push(Message::user(prompt));
            match parse_explanation(&resp, &steps) {
                Some(items) => {
                    let text: Vec<String> = items.iter().map(|i| i.text.clone()).collect();
                    session.history.push(Message::assistant(text.join("\n")));
                    emit(TurnEvent::Explain(items));
                }
                None => {
                    // Weak models: plain explanation without step mapping.
                    session.history.push(Message::assistant(resp.text.clone()));
                    emit(TurnEvent::Text(resp.text));
                }
            }
        }
        Err(e) => emit(TurnEvent::Error(e.to_string())),
    }
}

#[allow(clippy::too_many_arguments)]
async fn verified(
    session: &mut Session,
    mode: Mode,
    input: &TurnInput,
    ctx: &Context,
    generator: &dyn StreamingGenerator,
    runner: Option<&dyn DryRunner>,
    dry_runs: bool,
    emit: &(dyn Fn(TurnEvent) + Send + Sync),
) {
    let Some(query) = input.query.clone() else {
        emit(TurnEvent::Error("There is no query to work on.".into()));
        return;
    };
    let prompt = match mode {
        Mode::Fix => {
            let error = input
                .error
                .clone()
                .unwrap_or_else(|| "(no error message)".into());
            format!(
                "This query failed:\n{query}\nThe error was:\n{error}\nWrite a corrected query that does what the original intended. In the explanation, say what caused the error and what you changed. {}",
                input.question
            )
        }
        _ => {
            emit(TurnEvent::Status("Profiling\u{2026}".into()));
            let profile = match runner {
                Some(r) if dry_runs => profile_summary(&query, r).await,
                _ => None,
            };
            if let Some(p) = &profile {
                emit(TurnEvent::Tool {
                    name: "profile".into(),
                    input: query.clone(),
                    summary: p.lines().next().unwrap_or_default().to_owned(),
                });
            }
            format!(
                "Optimize this query for speed while returning the same results:\n{query}\n{}In the explanation, name the slowest steps and why the new query is faster. {}",
                profile
                    .map(|p| format!("Profile:\n{p}\n"))
                    .unwrap_or_default(),
                input.question
            )
        }
    };
    let mut pctx = ctx.clone();
    pctx.current_query = None;
    let opts = Options {
        dry_run: dry_runs,
        ..Options::default()
    };
    let status = |e: PipelineEvent| {
        if let PipelineEvent::Stage(s) = e {
            emit(TurnEvent::Status(s.label()));
        }
    };
    let Some(outcome) = pipeline::run(
        &prompt,
        &session.history,
        &pctx,
        generator,
        runner,
        &opts,
        &status,
    )
    .await
    else {
        emit(TurnEvent::Error(
            "The model did not return a usable query.".into(),
        ));
        return;
    };
    session.history = outcome.history.clone();
    if !outcome.answer.explanation.is_empty() {
        emit(TurnEvent::Text(outcome.answer.explanation.clone()));
    }
    let mut block = CodeBlock {
        query: outcome.answer.query.clone(),
        issues: outcome.issues.clone(),
        dry_run: outcome.badges.dry_run.clone(),
        writes: outcome.writes,
        equivalent: None,
    };
    if mode == Mode::Optimize
        && block.verified()
        && let Some(r) = runner.filter(|_| dry_runs)
    {
        block.equivalent = equivalent_on_sample(&query, &block.query, r, EQUIVALENCE_SAMPLE).await;
    }
    emit(TurnEvent::Block(block));
}

#[allow(clippy::too_many_arguments)]
async fn chat(
    session: &mut Session,
    mode: Mode,
    input: &TurnInput,
    ctx: &Context,
    generator: &dyn StreamingGenerator,
    runner: Option<&dyn DryRunner>,
    dry_runs: bool,
    emit: &(dyn Fn(TurnEvent) + Send + Sync),
) {
    let mut question = input.clone();
    if mode == Mode::Describe && question.question.is_empty() {
        question.question =
            "Describe this result: what it contains, its size and anything notable.".into();
    }
    let prompt = user_text(&question);
    session.history.push(Message::user(prompt));
    let tools = input.tools && mode == Mode::Chat;
    let mut calls = 0;
    let mut answer = String::new();
    loop {
        let req = ChatRequest {
            system: Some(mode_system(mode, ctx)),
            messages: session.history.clone(),
            tools: if tools && calls < MAX_TOOL_CALLS {
                tool_specs()
            } else {
                Vec::new()
            },
            ..ChatRequest::default()
        };
        let on_text = |t: &str| emit(TurnEvent::Text(t.to_owned()));
        let resp = match generator.generate_streaming(&req, &on_text).await {
            Ok(r) => r,
            Err(e) => {
                emit(TurnEvent::Error(e.to_string()));
                return;
            }
        };
        answer.push_str(&resp.text);
        if resp.tool_calls.is_empty() || !tools {
            session.history.push(Message::assistant(resp.text.clone()));
            break;
        }
        // Run the tool calls and loop with their results.
        let mut call_parts = Vec::new();
        if !resp.text.is_empty() {
            call_parts.push(Part::Text {
                text: resp.text.clone(),
            });
        }
        let mut result_parts = Vec::new();
        for (id, name, args_json) in &resp.tool_calls {
            calls += 1;
            let args: Value = serde_json::from_str(args_json).unwrap_or(json!({}));
            let (content, summary, is_error) = run_tool(name, &args, ctx, runner).await;
            let shown = args
                .get("query")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| args.to_string());
            emit(TurnEvent::Tool {
                name: name.clone(),
                input: shown,
                summary,
            });
            call_parts.push(Part::ToolCall {
                id: id.clone(),
                name: name.clone(),
                args,
            });
            result_parts.push(Part::ToolResult {
                id: id.clone(),
                name: name.clone(),
                content,
                is_error,
            });
        }
        session.history.push(Message {
            role: Role::Assistant,
            parts: call_parts,
        });
        session.history.push(Message {
            role: Role::User,
            parts: result_parts,
        });
    }
    for q in code_blocks(&answer) {
        emit(TurnEvent::Block(
            verify_block(&q, ctx, runner, dry_runs).await,
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::context::Dialect;

    fn ctx(schema: bool) -> Context {
        Context {
            schema: schema.then(|| {
                Arc::new(
                    schema::Schema::from_json(include_str!(
                        "../../gremlin-syntax/tests/fixtures/modern-schema.json"
                    ))
                    .unwrap_or_default(),
                )
            }),
            dialect: Dialect {
                tinkerpop_version: Some("3.7.3".into()),
                neptune: false,
            },
            current_query: None,
            share_samples: false,
        }
    }

    #[test]
    fn chat_prompt() {
        insta::assert_snapshot!(mode_system(Mode::Chat, &ctx(true)));
    }

    #[test]
    fn explain_prompt_without_schema() {
        insta::assert_snapshot!(mode_system(Mode::Explain, &ctx(false)));
    }

    #[test]
    fn steps_are_numbered_in_text_order() {
        let steps = numbered_steps("g.V().has('code','AUS').out('route').values('code')");
        let names: Vec<&str> = steps.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(
            names,
            ["V()", "has('code','AUS')", "out('route')", "values('code')"]
        );
    }

    #[test]
    fn code_blocks_are_found() {
        let md = "Try this:\n```gremlin\ng.V().count()\n```\nor\n```\ng.E().count()\n```\n```json\n{}\n```";
        assert_eq!(code_blocks(md), ["g.V().count()", "g.E().count()"]);
    }
}
