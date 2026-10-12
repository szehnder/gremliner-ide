//! Each wire format against recorded fixtures, served by a scripted HTTP server: streaming, tools,
//! structured output, model lists and errors. Scenario names map to
//! `openspec/changes/llm-providers/specs/llm-providers/spec.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use futures::StreamExt as _;
use llm::testing::{MockServer, Route};
use llm::{
    ChatEvent, ChatRequest, LlmClient, LlmError, OutputSchema, Profile, ProviderKind, StopReason,
    preset,
};
use serde_json::json;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn profile(kind: ProviderKind, base: &str) -> Profile {
    let mut p = Profile::new("test", kind, base);
    p.model = match kind {
        ProviderKind::Anthropic => "claude-sonnet-5-5",
        ProviderKind::Gemini => "gemini-2.5-flash",
        ProviderKind::OpenAiCompatible => "gpt-test",
    }
    .into();
    p
}

fn path_for(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Anthropic => "/v1/messages",
        ProviderKind::Gemini => "/v1beta/models/gemini-2.5-flash:streamGenerateContent",
        ProviderKind::OpenAiCompatible => "/chat/completions",
    }
}

async fn collect_events(kind: ProviderKind, fixture_name: &str) -> (Vec<ChatEvent>, MockServer) {
    let server = MockServer::start(vec![Route::sse(path_for(kind), &fixture(fixture_name))]).await;
    let client =
        LlmClient::new(profile(kind, &server.base), Some("test-key-123456".into())).unwrap();
    let mut stream = client.stream(&ChatRequest::user("hello")).await.unwrap();
    let mut out = Vec::new();
    while let Some(e) = stream.next().await {
        out.push(e.unwrap());
    }
    (out, server)
}

#[tokio::test]
async fn scenario_llm_providers_incremental_tokens() {
    for (kind, file) in [
        (ProviderKind::Anthropic, "anthropic-text.sse"),
        (ProviderKind::Gemini, "gemini-text.sse"),
        (ProviderKind::OpenAiCompatible, "openai-text.sse"),
    ] {
        let (events, server) = collect_events(kind, file).await;
        let deltas: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                ChatEvent::TextDelta(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            deltas.len() >= 3,
            "{kind:?}: several incremental deltas, got {deltas:?}"
        );
        assert!(deltas.concat().starts_with("Hello! Graph"), "{kind:?}");
        assert_eq!(
            events.last(),
            Some(&ChatEvent::Done(StopReason::EndTurn)),
            "{kind:?}"
        );
        let usage = events.iter().find_map(|e| match e {
            ChatEvent::Usage(u) => Some(*u),
            _ => None,
        });
        assert!(
            usage.is_some_and(|u| u.output_tokens.is_some()),
            "{kind:?} usage"
        );
        // Requests carry the format's auth header and the streaming flag.
        let req = &server.requests()[0];
        match kind {
            ProviderKind::Anthropic => {
                assert_eq!(req.header("x-api-key"), Some("test-key-123456"));
                assert_eq!(req.header("anthropic-version"), Some("2023-06-01"));
                assert!(req.body.contains("\"stream\":true"));
            }
            ProviderKind::Gemini => {
                assert_eq!(req.header("x-goog-api-key"), Some("test-key-123456"));
                assert!(req.path.ends_with("?alt=sse"), "{}", req.path);
            }
            ProviderKind::OpenAiCompatible => {
                assert_eq!(req.header("authorization"), Some("Bearer test-key-123456"));
                assert!(req.body.contains("\"include_usage\":true"));
            }
        }
    }
}

#[tokio::test]
async fn tool_calls_are_assembled_per_format() {
    let (events, _) = collect_events(ProviderKind::Anthropic, "anthropic-tool.sse").await;
    let call = events.iter().find_map(|e| match e {
        ChatEvent::ToolCall {
            name, args_json, ..
        } => Some((name.clone(), args_json.clone())),
        _ => None,
    });
    let (name, args) = call.unwrap();
    assert_eq!(name, "answer");
    let v: serde_json::Value = serde_json::from_str(&args).unwrap();
    assert_eq!(v["query"], "g.V().has('airport','code','AUS')");
    assert_eq!(events.last(), Some(&ChatEvent::Done(StopReason::ToolUse)));

    let (events, _) = collect_events(ProviderKind::OpenAiCompatible, "openai-tool.sse").await;
    assert!(events.contains(&ChatEvent::ToolCall {
        id: "call_abc".into(),
        name: "dry_run".into(),
        args_json: "{\"query\": \"g.V().limit(1)\"}".into(),
    }));
    assert_eq!(events.last(), Some(&ChatEvent::Done(StopReason::ToolUse)));

    let (events, _) = collect_events(ProviderKind::Gemini, "gemini-tool.sse").await;
    let call = events.iter().find_map(|e| match e {
        ChatEvent::ToolCall {
            name, args_json, ..
        } => Some((name.clone(), args_json.clone())),
        _ => None,
    });
    assert_eq!(
        call.unwrap(),
        ("get_schema".into(), "{\"label\":\"airport\"}".into())
    );
    assert_eq!(events.last(), Some(&ChatEvent::Done(StopReason::ToolUse)));
}

#[tokio::test]
async fn structured_output_per_format() {
    let schema = OutputSchema {
        name: "answer".into(),
        schema: json!({"type": "object", "additionalProperties": false,
            "properties": {"query": {"type": "string"}, "intent": {"type": "string"}},
            "required": ["query"]}),
    };
    let req = ChatRequest {
        output_schema: Some(schema.clone()),
        ..ChatRequest::user("count vertices")
    };
    // Anthropic: a forced tool.
    let server = MockServer::start(vec![Route::sse(
        "/v1/messages",
        &fixture("anthropic-tool.sse"),
    )])
    .await;
    let client = LlmClient::new(
        profile(ProviderKind::Anthropic, &server.base),
        Some("k-123456".into()),
    )
    .unwrap();
    let resp = client.complete(&req).await.unwrap();
    assert_eq!(resp.output_json("answer").unwrap()["intent"], "read");
    let body: serde_json::Value = serde_json::from_str(&server.requests()[0].body).unwrap();
    assert_eq!(
        body["tool_choice"],
        json!({"type": "tool", "name": "answer"})
    );
    // OpenAI: response_format json_schema, the JSON arrives as text.
    let server2 = MockServer::start(vec![Route::sse(
        "/v1/chat/completions",
        &fixture("openai-json.sse"),
    )])
    .await;
    let mut p = profile(
        ProviderKind::OpenAiCompatible,
        &format!("{}/v1", server2.base),
    );
    p.overrides.json_schema = Some(true);
    let client = LlmClient::new(p, Some("k-123456".into())).unwrap();
    let resp = client.complete(&req).await.unwrap();
    assert_eq!(
        resp.output_json("answer").unwrap()["query"],
        "g.V().count()"
    );
    let body: serde_json::Value = serde_json::from_str(&server2.requests()[0].body).unwrap();
    assert_eq!(body["response_format"]["type"], "json_schema");
    // Gemini: responseSchema without the keywords it rejects.
    let built = llm::wire::build(
        &profile(ProviderKind::Gemini, "http://x"),
        Some("k"),
        &req,
        true,
    )
    .unwrap();
    let rs = &built.body["generationConfig"]["responseSchema"];
    assert_eq!(
        built.body["generationConfig"]["responseMimeType"],
        "application/json"
    );
    assert!(rs.get("additionalProperties").is_none());
    assert_eq!(rs["required"], json!(["query"]));
}

#[tokio::test]
async fn stream_errors_and_retries() {
    // An error event mid-stream ends the stream with that error.
    let server = MockServer::start(vec![Route::sse(
        "/v1/messages",
        &fixture("anthropic-overloaded.sse"),
    )])
    .await;
    let client = LlmClient::new(
        profile(ProviderKind::Anthropic, &server.base),
        Some("k-123456".into()),
    )
    .unwrap();
    let err = client.complete(&ChatRequest::user("hi")).await.unwrap_err();
    assert!(
        matches!(&err, LlmError::Http { message, .. } if message == "Overloaded"),
        "{err:?}"
    );
    // 500s are retried twice, then reported.
    let server = MockServer::start(vec![Route::json(
        "POST",
        "/chat/completions",
        500,
        r#"{"error":"boom"}"#,
    )])
    .await;
    let client =
        LlmClient::new(profile(ProviderKind::OpenAiCompatible, &server.base), None).unwrap();
    let err = client.complete(&ChatRequest::user("hi")).await.unwrap_err();
    assert_eq!(
        err,
        LlmError::Http {
            status: 500,
            message: "boom".into()
        }
    );
    assert_eq!(server.requests().len(), 3);
}

#[tokio::test]
async fn scenario_llm_providers_list_local_models() {
    let server = MockServer::start(vec![Route::json(
        "GET",
        "/v1/models",
        200,
        &fixture("ollama-models.json"),
    )])
    .await;
    let mut p = preset("ollama").unwrap().profile("Local");
    p.base_url = format!("{}/v1", server.base);
    let client = LlmClient::new(p, None).unwrap();
    assert_eq!(
        client.list_models().await.unwrap(),
        ["llama3.1:8b", "qwen2.5-coder:7b"]
    );
    assert!(
        server.requests()[0].header("authorization").is_none(),
        "no key for local models"
    );
    // Gemini lists only generateContent models, without the `models/` prefix.
    let server = MockServer::start(vec![Route::json(
        "GET",
        "/v1beta/models",
        200,
        &fixture("gemini-models.json"),
    )])
    .await;
    let client = LlmClient::new(
        profile(ProviderKind::Gemini, &server.base),
        Some("k-123456".into()),
    )
    .unwrap();
    assert_eq!(client.list_models().await.unwrap(), ["gemini-2.5-flash"]);
}

#[tokio::test]
async fn scenario_llm_providers_manual_model_entry() {
    // Listing fails (nothing listens): an error comes back, and a typed model still works.
    let mut p = preset("ollama").unwrap().profile("Local");
    p.base_url = "http://127.0.0.1:1/v1".into();
    p.model = String::new();
    let client = LlmClient::new(p.clone(), None).unwrap();
    let err = client.list_models().await.unwrap_err();
    assert!(matches!(err, LlmError::Network(_)), "{err:?}");
    assert!(!err.to_string().is_empty());
    // A profile with a typed model is valid to build requests (and save, see the settings tests).
    p.model = "my-custom-model".into();
    let built = llm::wire::build(&p, None, &ChatRequest::user("hi"), true).unwrap();
    assert_eq!(built.body["model"], "my-custom-model");
    // Without any model, requests explain why.
    p.model = String::new();
    assert_eq!(
        llm::wire::build(&p, None, &ChatRequest::user("hi"), true).unwrap_err(),
        LlmError::NoModel
    );
}

#[tokio::test]
async fn scenario_llm_providers_invalid_key() {
    let key = "sk-test-abcdefghijklmnopWXYZ";
    let server = MockServer::start(vec![Route::json(
        "POST",
        "/chat/completions",
        401,
        &fixture("openai-401.json"),
    )])
    .await;
    let client = LlmClient::new(
        profile(ProviderKind::OpenAiCompatible, &server.base),
        Some(key.into()),
    )
    .unwrap();
    let err = client.test(|_| {}).await.unwrap_err();
    let LlmError::Auth { status, message } = &err else {
        panic!("{err:?}")
    };
    assert_eq!(*status, 401);
    assert!(message.contains("Incorrect API key provided"), "{message}");
    assert!(!message.contains("sk-test"), "no key material: {message}");
    assert!(!err.to_string().contains(&key[3..10]));
    // Anthropic's own message comes through.
    let server = MockServer::start(vec![Route::json(
        "POST",
        "/v1/messages",
        401,
        &fixture("anthropic-401.json"),
    )])
    .await;
    let client = LlmClient::new(
        profile(ProviderKind::Anthropic, &server.base),
        Some(key.into()),
    )
    .unwrap();
    let err = client.test(|_| {}).await.unwrap_err();
    assert_eq!(
        err,
        LlmError::Auth {
            status: 401,
            message: "invalid x-api-key".into()
        }
    );
    // A cloud profile without a key fails before any request.
    let client = LlmClient::new(profile(ProviderKind::Anthropic, &server.base), None).unwrap();
    assert_eq!(client.test(|_| {}).await.unwrap_err(), LlmError::MissingKey);
}

#[tokio::test]
async fn scenario_llm_providers_successful_test() {
    let server = MockServer::start(vec![Route::sse(
        "/v1/messages",
        &fixture("anthropic-text.sse"),
    )])
    .await;
    let client = LlmClient::new(
        profile(ProviderKind::Anthropic, &server.base),
        Some("k-123456".into()),
    )
    .unwrap();
    let mut streamed = Vec::new();
    let report = client.test(|t| streamed.push(t.to_owned())).await.unwrap();
    assert!(streamed.len() >= 3, "text streamed in pieces");
    assert_eq!(report.text, streamed.concat());
    assert!(report.first_token.is_some());
    assert!(report.total >= report.first_token.unwrap());
    let summary = report.summary();
    assert!(summary.starts_with("First token "), "{summary}");
    assert!(summary.contains("25 in / 15 out tokens"), "{summary}");
}

#[tokio::test]
async fn non_streaming_profiles_decode_whole_responses() {
    let body = r#"{"choices":[{"index":0,"message":{"role":"assistant","content":"Hi there."},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":2}}"#;
    let server = MockServer::start(vec![Route::json("POST", "/chat/completions", 200, body)]).await;
    let mut p = profile(ProviderKind::OpenAiCompatible, &server.base);
    p.overrides.streaming = Some(false);
    let client = LlmClient::new(p, None).unwrap();
    let resp = client.complete(&ChatRequest::user("hi")).await.unwrap();
    assert_eq!(resp.text, "Hi there.");
    assert_eq!(resp.usage.output_tokens, Some(2));
    assert!(!server.requests()[0].body.contains("\"stream\""));
}
