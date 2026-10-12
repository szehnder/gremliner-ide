//! Deterministic stand-ins for tests: a scripted model and a scripted database.

use std::sync::Mutex;
use std::time::Duration;

use futures::future::BoxFuture;
use gremlin_model::GValue;
use llm::{ChatRequest, ChatResponse, LlmError};

use crate::dryrun::DryRunner;
use crate::pipeline::Generator;

/// Replies with the scripted texts in order (the last one repeats); records every request.
pub struct ScriptedLlm {
    replies: Vec<Result<String, LlmError>>,
    pub requests: Mutex<Vec<ChatRequest>>,
}

impl ScriptedLlm {
    pub fn new(replies: Vec<&str>) -> Self {
        Self {
            replies: replies.into_iter().map(|r| Ok(r.to_owned())).collect(),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub fn failing(err: LlmError) -> Self {
        Self {
            replies: vec![Err(err)],
            requests: Mutex::new(Vec::new()),
        }
    }

    /// A JSON answer as a model would write it.
    pub fn answer(query: &str, shape: &str) -> String {
        serde_json::json!({"query": query, "explanation": "Scripted.", "intent": "read", "expected_shape": shape}).to_string()
    }

    pub fn requests(&self) -> Vec<ChatRequest> {
        self.requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl Generator for ScriptedLlm {
    fn generate<'a>(
        &'a self,
        req: &'a ChatRequest,
    ) -> BoxFuture<'a, Result<ChatResponse, LlmError>> {
        let mut requests = self
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let n = requests.len();
        requests.push(req.clone());
        let reply = self.replies[n.min(self.replies.len() - 1)].clone();
        Box::pin(async move {
            reply.map(|text| ChatResponse {
                text,
                ..ChatResponse::default()
            })
        })
    }
}

/// Answers dry runs by script substring: the first `(needle, result)` whose needle occurs in the script.
pub struct ScriptedDb {
    pub answers: Vec<(String, Result<Vec<GValue>, String>)>,
    pub scripts: Mutex<Vec<String>>,
}

impl ScriptedDb {
    pub fn new(answers: Vec<(&str, Result<Vec<GValue>, String>)>) -> Self {
        Self {
            answers: answers
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect(),
            scripts: Mutex::new(Vec::new()),
        }
    }

    pub fn scripts(&self) -> Vec<String> {
        self.scripts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl DryRunner for ScriptedDb {
    fn run<'a>(
        &'a self,
        script: &'a str,
        _: Duration,
    ) -> BoxFuture<'a, Result<(Vec<GValue>, Duration), String>> {
        self.scripts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(script.to_owned());
        let answer = self
            .answers
            .iter()
            .find(|(needle, _)| script.contains(needle.as_str()))
            .map(|(_, a)| a.clone())
            .unwrap_or_else(|| Ok(Vec::new()));
        Box::pin(async move { answer.map(|items| (items, Duration::from_millis(7))) })
    }
}
