//! Assistant panel conversations with a scripted model: modes, tools and verified blocks. Scenario names
//! map to `openspec/changes/assistant-panel/specs/assistant-panel/spec.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use assistant::chat::{Mode, ResultSummary, Session, TurnEvent, TurnInput, turn};
use assistant::testing::{ScriptedDb, ScriptedLlm};
use assistant::{Context, Dialect, DryRun};
use gremlin_client::{Client, ClientConfig, require_testenv};
use gremlin_model::GValue;
use schema::Schema;

fn air() -> Arc<Schema> {
    Arc::new(
        Schema::from_json(include_str!(
            "../../gremlin-syntax/tests/fixtures/air-routes-schema.json"
        ))
        .unwrap(),
    )
}

fn ctx() -> Context {
    Context {
        schema: Some(air()),
        dialect: Dialect {
            tinkerpop_version: Some("3.7.3".into()),
            neptune: false,
        },
        current_query: None,
        share_samples: false,
    }
}

async fn go(
    input: TurnInput,
    llm: &ScriptedLlm,
    db: Option<&dyn assistant::DryRunner>,
    ctx: &Context,
) -> (Vec<TurnEvent>, Session) {
    let events = Mutex::new(Vec::new());
    let mut session = Session::default();
    turn(&mut session, &input, ctx, llm, db, true, &|e| {
        events.lock().unwrap().push(e)
    })
    .await;
    (events.into_inner().unwrap(), session)
}

fn blocks(events: &[TurnEvent]) -> Vec<assistant::chat::CodeBlock> {
    events
        .iter()
        .filter_map(|e| match e {
            TurnEvent::Block(b) => Some(b.clone()),
            _ => None,
        })
        .collect()
}

fn text(events: &[TurnEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            TurnEvent::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn explain_maps_items_to_steps() {
    let llm = ScriptedLlm::new(vec![
        r#"{"items": [
        {"steps": [1], "text": "Start from all vertices."},
        {"steps": [2], "text": "Keep the one with code AUS."},
        {"steps": [3], "text": "Follow outgoing route edges."},
        {"steps": [4], "text": "Return their codes."}]}"#,
    ]);
    let query = "g.V().has('code','AUS').out('route').values('code')";
    let input = TurnInput {
        mode: Some(Mode::Explain),
        query: Some(query.into()),
        ..TurnInput::default()
    };
    let (events, _) = go(input, &llm, None, &ctx()).await;
    let items = events.iter().find_map(|e| match e {
        TurnEvent::Explain(i) => Some(i.clone()),
        _ => None,
    });
    let items = items.expect("explanation items");
    assert_eq!(items.len(), 4);
    assert_eq!(&query[items[2].spans[0].clone()], "out('route')");
    // The model saw the numbered steps.
    let prompt = llm.requests()[0].messages[0].text();
    assert!(prompt.contains("3. out('route')"), "{prompt}");
    // Weak models answering prose still get an explanation.
    let llm = ScriptedLlm::new(vec!["It finds Austin and lists where you can fly."]);
    let input = TurnInput {
        mode: Some(Mode::Explain),
        query: Some(query.into()),
        ..TurnInput::default()
    };
    let (events, _) = go(input, &llm, None, &ctx()).await;
    assert_eq!(
        text(&events),
        "It finds Austin and lists where you can fly."
    );
}

#[tokio::test]
async fn scenario_assistant_panel_fix_a_failing_query() {
    let db = ScriptedDb::new(vec![("'airport'", Ok(vec![GValue::String("AUS".into())]))]);
    let reply = serde_json::json!({
        "query": "g.V().has('airport','code','AUS').values('city')",
        "explanation": "The label is `airport`, not `airports`, so nothing matched and values() failed on the missing key.",
        "intent": "read", "expected_shape": "table"
    })
    .to_string();
    let llm = ScriptedLlm::new(vec![&reply]);
    let input = TurnInput {
        mode: Some(Mode::Fix),
        query: Some("g.V().has('airports','code','AUS').values('cty')".into()),
        error: Some("The property does not exist as the key has no associated value: cty".into()),
        ..TurnInput::default()
    };
    let (events, _) = go(input, &llm, Some(&db), &ctx()).await;
    let b = &blocks(&events)[0];
    assert!(b.verified(), "{b:?}");
    assert!(matches!(b.dry_run, DryRun::Passed { rows: 1, .. }));
    assert!(
        text(&events).contains("not `airports`"),
        "explains the cause"
    );
    let prompt = llm.requests()[0].messages.last().unwrap().text();
    assert!(
        prompt.contains("no associated value: cty"),
        "the error was sent: {prompt}"
    );
}

#[tokio::test]
async fn scenario_assistant_panel_optimize_with_profile() {
    let url = require_testenv!();
    let db = Client::connect(ClientConfig::new(&url)).await.unwrap();
    let original = "g.V().hasLabel('airport').has('code','AUS').out('route').values('code')";
    let reply = serde_json::json!({
        "query": "g.V().has('airport','code','AUS').out('route').values('code')",
        "explanation": "The slowest step is the TinkerGraphStep that scans airports; has(label, key, value) lets the index lookup do it in one step.",
        "intent": "read", "expected_shape": "table"
    })
    .to_string();
    let llm = ScriptedLlm::new(vec![&reply]);
    let input = TurnInput {
        mode: Some(Mode::Optimize),
        query: Some(original.into()),
        ..TurnInput::default()
    };
    let (events, _) = go(input, &llm, Some(&db), &ctx()).await;
    // The profile ran and was sent, slowest steps first.
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TurnEvent::Tool { name, .. } if name == "profile"))
    );
    let prompt = llm.requests()[0].messages.last().unwrap().text();
    assert!(prompt.contains("Steps, slowest first:"), "{prompt}");
    assert!(prompt.contains("TinkerGraphStep"), "{prompt}");
    let b = &blocks(&events)[0];
    assert_eq!(b.equivalent, Some(true), "same count on the sample: {b:?}");
    assert!(text(&events).contains("slowest step"));
}

#[tokio::test]
async fn scenario_assistant_panel_describe_without_samples() {
    let url = require_testenv!();
    let db = Client::connect(ClientConfig::new(&url)).await.unwrap();
    let items = assistant::DryRunner::run(
        &db,
        "g.V().hasLabel('airport').limit(20).toList()",
        std::time::Duration::from_secs(5),
    )
    .await
    .unwrap()
    .0;
    let summary = ResultSummary::of(&items, false);
    assert_eq!(summary.count, 20);
    assert!(summary.rows.is_empty());
    let llm = ScriptedLlm::new(vec!["20 airports."]);
    let input = TurnInput {
        mode: Some(Mode::Describe),
        result: Some(summary),
        ..TurnInput::default()
    };
    let (events, _) = go(input, &llm, Some(&db), &ctx()).await;
    assert_eq!(text(&events), "20 airports.");
    let prompt = llm.requests()[0].messages[0].text();
    assert!(prompt.contains("20 items"), "{prompt}");
    assert!(prompt.contains("airport (20)"), "labels: {prompt}");
    assert!(
        prompt.contains("Columns:") && prompt.contains("code"),
        "columns: {prompt}"
    );
    // No property values: none of the airport codes or cities appear.
    for v in items.iter().filter_map(|v| match v {
        GValue::Vertex(v) => v
            .properties
            .get("code")
            .and_then(|p| p[0].value.as_str().map(str::to_owned)),
        _ => None,
    }) {
        assert!(!prompt.contains(&format!("\"{v}\"")), "value {v} was sent");
    }
    // With sharing on, rows are included.
    assert!(!ResultSummary::of(&items, true).rows.is_empty());
}

#[tokio::test]
async fn scenario_assistant_panel_visible_tool_call() {
    let db = ScriptedDb::new(vec![("g.V()", Ok(vec![GValue::Int64(3504)]))]);
    let llm = ScriptedLlm::new(vec![
        r#"@tool {"name": "dry_run", "args": {"query": "g.V().hasLabel('airport').count()"}}"#,
        "There are 3,504 airports.\n```gremlin\ng.V().hasLabel('airport').count()\n```",
    ]);
    let input = TurnInput {
        question: "how many airports?".into(),
        tools: true,
        ..TurnInput::default()
    };
    let (events, session) = go(input, &llm, Some(&db), &ctx()).await;
    let tool = events.iter().find_map(|e| match e {
        TurnEvent::Tool {
            name,
            input,
            summary,
        } => Some((name.clone(), input.clone(), summary.clone())),
        _ => None,
    });
    let (name, input, summary) = tool.unwrap();
    assert_eq!(name, "dry_run");
    assert_eq!(input, "g.V().hasLabel('airport').count()");
    assert!(summary.starts_with("1 rows in"), "{summary}");
    assert!(db.scripts()[0].contains("count()"));
    // The tool result went back to the model, then the answer streamed in pieces.
    assert_eq!(llm.requests().len(), 2);
    assert!(llm.requests()[0].tools.iter().any(|t| t.name == "dry_run"));
    let deltas = events
        .iter()
        .filter(|e| matches!(e, TurnEvent::Text(_)))
        .count();
    assert!(deltas >= 2, "streamed");
    assert_eq!(blocks(&events).len(), 1, "the answer's query is verified");
    assert!(session.history.len() >= 4);
}

#[tokio::test]
async fn scenario_assistant_panel_mutation_refused_by_tool() {
    let db = ScriptedDb::new(vec![]);
    let llm = ScriptedLlm::new(vec![
        r#"@tool {"name": "dry_run", "args": {"query": "g.V().hasLabel('airport').drop()"}}"#,
        "I can't run that.",
    ]);
    let input = TurnInput {
        question: "delete the airports".into(),
        tools: true,
        ..TurnInput::default()
    };
    let (events, _) = go(input, &llm, Some(&db), &ctx()).await;
    assert!(db.scripts().is_empty(), "nothing reached the database");
    let summary = events.iter().find_map(|e| match e {
        TurnEvent::Tool { summary, .. } => Some(summary.clone()),
        _ => None,
    });
    assert_eq!(summary.as_deref(), Some("refused: the query changes data"));
    let requests = llm.requests();
    let result = &requests[1].messages.last().unwrap().parts[0];
    let llm::Part::ToolResult {
        content, is_error, ..
    } = result
    else {
        panic!("{result:?}")
    };
    assert!(content.starts_with("Refused") && *is_error);
}

#[tokio::test]
async fn tools_only_when_enabled_and_samples_respect_privacy() {
    let db = ScriptedDb::new(vec![]);
    let llm = ScriptedLlm::new(vec!["ok"]);
    let (_, _) = go(
        TurnInput {
            question: "hi".into(),
            tools: false,
            ..TurnInput::default()
        },
        &llm,
        Some(&db),
        &ctx(),
    )
    .await;
    assert!(llm.requests()[0].tools.is_empty());
    let (content, summary, _) = assistant::chat::run_tool(
        "sample_values",
        &serde_json::json!({"label": "airport", "key": "code"}),
        &ctx(),
        Some(&db),
    )
    .await;
    assert!(content.starts_with("Refused") && summary.contains("sample sharing is off"));
    assert!(db.scripts().is_empty());
}
