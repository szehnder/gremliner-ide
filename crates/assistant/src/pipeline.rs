//! The NL→Gremlin pipeline (ADR-0019): generate → validate → dry run → repair (≤ 3) → outcome, emitting
//! [`PipelineEvent`]s for the live status. Mutations are never executed (ADR-0020).

use std::time::Duration;

use futures::future::BoxFuture;
use gremlin_model::GValue;
use llm::{ChatRequest, ChatResponse, LlmClient, LlmError, Message};
use serde::Serialize;

use crate::answer::{self, Answer, Intent, Shape};
use crate::check::{Issue, check};
use crate::context::{Context, build_request};
use crate::dryrun::{DEFAULT_LIMIT, DEFAULT_TIMEOUT, DryRunner, inject_limit};

/// Produces the model's reply (an `LlmClient` in the app, a script in tests).
pub trait Generator: Send + Sync {
    fn generate<'a>(
        &'a self,
        req: &'a ChatRequest,
    ) -> BoxFuture<'a, Result<ChatResponse, LlmError>>;
}

impl Generator for LlmClient {
    fn generate<'a>(
        &'a self,
        req: &'a ChatRequest,
    ) -> BoxFuture<'a, Result<ChatResponse, LlmError>> {
        Box::pin(self.complete(req))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub max_repairs: usize,
    pub limit: usize,
    pub timeout: Duration,
    /// Dry runs are allowed on this connection.
    pub dry_run: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_repairs: 3,
            limit: DEFAULT_LIMIT,
            timeout: DEFAULT_TIMEOUT,
            dry_run: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "stage", content = "attempt")]
pub enum Stage {
    Generating,
    Validating,
    DryRunning,
    /// Repair attempt n of max.
    Repairing(usize, usize),
}

impl Stage {
    pub fn label(&self) -> String {
        match self {
            Stage::Generating => "Generating\u{2026}".into(),
            Stage::Validating => "Validating\u{2026}".into(),
            Stage::DryRunning => "Test-running\u{2026}".into(),
            Stage::Repairing(n, max) => format!("Repairing {n}/{max}\u{2026}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum DryRun {
    Passed {
        rows: usize,
        #[serde(serialize_with = "ms")]
        elapsed: Duration,
    },
    Failed {
        message: String,
    },
    /// Not run: a write, dry runs disabled, or the query did not validate.
    Skipped {
        reason: String,
    },
}

fn ms<S: serde::Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_u64(u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// The verification badges.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Badges {
    pub syntax: bool,
    /// `None`: no schema to check against.
    pub schema: Option<bool>,
    pub dry_run: DryRun,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Outcome {
    pub answer: Answer,
    pub badges: Badges,
    pub issues: Vec<Issue>,
    /// Dry-run rows (at most the limit).
    #[serde(skip)]
    pub preview: Vec<GValue>,
    /// Passed validation and the dry run (or is a valid write, which is never run).
    pub verified: bool,
    pub writes: bool,
    pub repairs: usize,
    /// The conversation including this request and answer, for refinements.
    #[serde(skip)]
    pub history: Vec<Message>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PipelineEvent {
    Stage(Stage),
    /// A candidate query (each generation or repair).
    Candidate(String),
    Done(Box<Outcome>),
    /// The model could not be reached or answered nothing usable at all.
    Failed(String),
}

/// One attempt's result.
struct Attempt {
    answer: Answer,
    issues: Vec<Issue>,
    dry_run: DryRun,
    preview: Vec<GValue>,
}

impl Attempt {
    /// Higher is better: a passing dry run, then a valid query, then fewer issues.
    fn score(&self) -> (u8, isize) {
        let run = match &self.dry_run {
            DryRun::Passed { rows, .. } if *rows > 0 => 3,
            DryRun::Passed { .. } => 2,
            _ if self.issues.is_empty() => 1,
            _ => 0,
        };
        (
            run,
            -isize::try_from(self.issues.len()).unwrap_or(isize::MAX),
        )
    }
}

fn repair_message(answer: &Answer, problem: &str) -> String {
    format!(
        "The query\n{}\n{problem}\nReturn a corrected query in the same JSON format, keeping what the request asked for.",
        answer.query
    )
}

fn issues_text(issues: &[Issue]) -> String {
    let lines: Vec<String> = issues.iter().map(|i| format!("- {}", i.message)).collect();
    format!("has these problems:\n{}", lines.join("\n"))
}

/// Run the pipeline for `prompt`. `history` holds earlier prompts and answers (refinement).
pub async fn run(
    prompt: &str,
    history: &[Message],
    ctx: &Context,
    generator: &dyn Generator,
    runner: Option<&dyn DryRunner>,
    opts: &Options,
    emit: &(dyn Fn(PipelineEvent) + Send + Sync),
) -> Option<Outcome> {
    let mut req = build_request(ctx, history, prompt);
    let done = |repairs: usize| Request {
        history,
        prompt,
        has_schema: ctx.schema.is_some(),
        repairs,
    };
    let mut best: Option<Attempt> = None;
    let mut repairs = 0;
    let mut empty_retried = false;
    loop {
        emit(PipelineEvent::Stage(if repairs == 0 {
            Stage::Generating
        } else {
            Stage::Repairing(repairs, opts.max_repairs)
        }));
        let resp = match generator.generate(&req).await {
            Ok(r) => r,
            Err(e) => {
                emit(PipelineEvent::Failed(e.to_string()));
                return best.map(|b| finish(b, &done(repairs), emit));
            }
        };
        let Some(answer) = answer::parse(&resp) else {
            if repairs < opts.max_repairs {
                repairs += 1;
                req.messages.push(Message::assistant(resp.text.clone()));
                req.messages.push(Message::user(
                    "That reply had no query. Answer with the JSON object only, e.g. {\"query\": \"g.V()...\", \"explanation\": \"...\", \"intent\": \"read\", \"expected_shape\": \"table\"}.",
                ));
                continue;
            }
            emit(PipelineEvent::Failed(
                "The model did not return a Gremlin query.".into(),
            ));
            return best.map(|b| finish(b, &done(repairs), emit));
        };
        emit(PipelineEvent::Candidate(answer.query.clone()));
        emit(PipelineEvent::Stage(Stage::Validating));
        let issues = check(&answer.query, ctx.schema.as_deref(), &ctx.dialect);
        let writes = answer.intent == Intent::Write || gremlin_syntax::is_mutating(&answer.query);
        req.messages.push(Message::assistant(answer.to_json()));

        // A problem to repair, if any; else the attempt is final.
        let (attempt, problem) = if !issues.is_empty() {
            let problem = issues_text(&issues);
            (
                Attempt {
                    answer,
                    issues,
                    dry_run: DryRun::Skipped {
                        reason: "did not validate".into(),
                    },
                    preview: Vec::new(),
                },
                Some(problem),
            )
        } else if writes {
            // ADR-0020: never executed automatically.
            let a = Attempt {
                answer,
                issues,
                dry_run: DryRun::Skipped {
                    reason: "writes data".into(),
                },
                preview: Vec::new(),
            };
            return Some(finish(a, &done(repairs), emit));
        } else if let (true, Some(runner)) = (opts.dry_run, runner) {
            emit(PipelineEvent::Stage(Stage::DryRunning));
            let script = gremlin_syntax::ensure_terminal(&inject_limit(&answer.query, opts.limit));
            match runner.run(&script, opts.timeout).await {
                Ok((items, elapsed)) => {
                    let rows = items.len();
                    let attempt = Attempt {
                        answer,
                        issues,
                        dry_run: DryRun::Passed { rows, elapsed },
                        preview: items,
                    };
                    let expect_rows = attempt.answer.expected_shape != Shape::Scalar;
                    if rows == 0 && expect_rows && !empty_retried {
                        empty_retried = true;
                        (attempt, Some("ran without errors but returned 0 rows. Check the labels, property values (case, exact spelling) and edge directions.".to_owned()))
                    } else {
                        return Some(finish(better(best, attempt), &done(repairs), emit));
                    }
                }
                Err(message) => {
                    let problem = format!("failed when run on the database: {message}");
                    (
                        Attempt {
                            answer,
                            issues,
                            dry_run: DryRun::Failed { message },
                            preview: Vec::new(),
                        },
                        Some(problem),
                    )
                }
            }
        } else {
            let a = Attempt {
                answer,
                issues,
                dry_run: DryRun::Skipped {
                    reason: "dry runs are off for this connection".into(),
                },
                preview: Vec::new(),
            };
            return Some(finish(a, &done(repairs), emit));
        };
        let problem = problem.unwrap_or_default();
        let message = repair_message(&attempt.answer, &problem);
        best = Some(better(best, attempt));
        if repairs >= opts.max_repairs {
            return best.map(|b| finish(b, &done(repairs), emit));
        }
        repairs += 1;
        req.messages.push(Message::user(message));
    }
}

fn better(best: Option<Attempt>, next: Attempt) -> Attempt {
    match best {
        Some(b) if b.score() > next.score() => b,
        _ => next,
    }
}

/// What `finish` needs from the request.
struct Request<'a> {
    history: &'a [Message],
    prompt: &'a str,
    has_schema: bool,
    repairs: usize,
}

fn finish(a: Attempt, r: &Request<'_>, emit: &(dyn Fn(PipelineEvent) + Send + Sync)) -> Outcome {
    use crate::check::IssueKind;
    let syntax = !a.issues.iter().any(|i| i.kind == IssueKind::Syntax);
    let schema_ok = !a.issues.iter().any(|i| i.kind == IssueKind::Schema);
    let writes = a.answer.intent == Intent::Write || gremlin_syntax::is_mutating(&a.answer.query);
    let verified = a.issues.is_empty()
        && match &a.dry_run {
            DryRun::Passed { .. } => true,
            DryRun::Skipped { .. } => writes,
            DryRun::Failed { .. } => false,
        };
    // For refinement: earlier turns, this prompt and the final answer (not the repair exchange).
    let mut history = r.history.to_vec();
    history.push(Message::user(r.prompt));
    history.push(Message::assistant(a.answer.to_json()));
    let outcome = Outcome {
        badges: Badges {
            syntax,
            schema: r.has_schema.then_some(schema_ok),
            dry_run: a.dry_run,
        },
        answer: a.answer,
        issues: a.issues,
        preview: a.preview,
        verified,
        writes,
        repairs: r.repairs,
        history,
    };
    emit(PipelineEvent::Done(Box::new(outcome.clone())));
    outcome
}
