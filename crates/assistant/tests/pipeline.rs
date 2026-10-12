//! The pipeline with a scripted model, dry-running against the Docker testenv (air-routes) or a scripted
//! database. Scenario names map to `openspec/changes/nl-to-gremlin/specs/nl-to-gremlin/spec.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use assistant::testing::{ScriptedDb, ScriptedLlm};
use assistant::{Context, Dialect, DryRun, Options, Outcome, PipelineEvent, Stage, run};
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

fn ctx(share: bool) -> Context {
    Context {
        schema: Some(air()),
        dialect: Dialect {
            tinkerpop_version: Some("3.7.3".into()),
            neptune: false,
        },
        current_query: None,
        share_samples: share,
    }
}

async fn client(url: &str) -> Client {
    Client::connect(ClientConfig::new(url)).await.unwrap()
}

async fn go(
    prompt: &str,
    llm: &ScriptedLlm,
    db: &dyn assistant::DryRunner,
    ctx: &Context,
) -> (Option<Outcome>, Vec<PipelineEvent>) {
    let events = Mutex::new(Vec::new());
    let outcome = run(prompt, &[], ctx, llm, Some(db), &Options::default(), &|e| {
        events.lock().unwrap().push(e)
    })
    .await;
    (outcome, events.into_inner().unwrap())
}

fn stages(events: &[PipelineEvent]) -> Vec<Stage> {
    events
        .iter()
        .filter_map(|e| match e {
            PipelineEvent::Stage(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn scenario_nl_to_gremlin_dry_run_preview() {
    let url = require_testenv!();
    let db = client(&url).await;
    let llm = ScriptedLlm::new(vec![&ScriptedLlm::answer(
        "g.V().has('airport','region','US-TX').values('code')",
        "table",
    )]);
    let (outcome, events) = go("airports in Texas", &llm, &db, &ctx(false)).await;
    let outcome = outcome.unwrap();
    assert!(outcome.verified, "{outcome:?}");
    let DryRun::Passed { rows, elapsed } = outcome.badges.dry_run else {
        panic!("{:?}", outcome.badges)
    };
    assert_eq!(rows, 25, "limited to 25");
    assert!(elapsed.as_millis() < 5_000);
    assert_eq!(outcome.preview.len(), 25);
    assert!(outcome.badges.syntax && outcome.badges.schema == Some(true));
    assert_eq!(
        stages(&events),
        [Stage::Generating, Stage::Validating, Stage::DryRunning]
    );
    assert!(matches!(events.last(), Some(PipelineEvent::Done(_))));
}

#[tokio::test]
async fn scenario_nl_to_gremlin_mutation_never_auto_executed() {
    let db = ScriptedDb::new(vec![]);
    let reply = serde_json::json!({
        "query": "g.V().has('airport','region','US-TX').drop()",
        "explanation": "Deletes every airport in Texas.",
        "intent": "write",
        "expected_shape": "scalar"
    })
    .to_string();
    let llm = ScriptedLlm::new(vec![&reply]);
    let (outcome, events) = go("delete all airports in Texas", &llm, &db, &ctx(false)).await;
    let outcome = outcome.unwrap();
    assert!(outcome.writes);
    assert!(db.scripts().is_empty(), "nothing ran: {:?}", db.scripts());
    assert!(
        matches!(&outcome.badges.dry_run, DryRun::Skipped { reason } if reason == "writes data")
    );
    assert!(!stages(&events).contains(&Stage::DryRunning));
    // Even when the model claims "read", mutation detection wins.
    let llm = ScriptedLlm::new(vec![&ScriptedLlm::answer(
        "g.V().has('code','AUS').property('x', 1)",
        "graph",
    )]);
    let (outcome, _) = go("tag austin", &llm, &db, &ctx(false)).await;
    assert!(outcome.unwrap().writes);
    assert!(db.scripts().is_empty());
}

#[tokio::test]
async fn scenario_nl_to_gremlin_repair_after_server_error() {
    let url = require_testenv!();
    let db = client(&url).await;
    // math() on a string fails on the server.
    let llm = ScriptedLlm::new(vec![
        &ScriptedLlm::answer(
            "g.V().has('airport','code','AUS').values('city').math('_ + 1')",
            "scalar",
        ),
        &ScriptedLlm::answer(
            "g.V().has('airport','code','AUS').values('runways')",
            "scalar",
        ),
    ]);
    let (outcome, events) = go("runways at Austin plus one", &llm, &db, &ctx(false)).await;
    let outcome = outcome.unwrap();
    assert!(outcome.verified, "{outcome:?}");
    assert_eq!(outcome.repairs, 1);
    assert!(stages(&events).contains(&Stage::Repairing(1, 3)));
    assert_eq!(Stage::Repairing(1, 3).label(), "Repairing 1/3\u{2026}");
    // The repair request carried the server's error.
    let requests = llm.requests();
    let repair = requests[1].messages.last().unwrap().text();
    assert!(
        repair.contains("failed when run on the database"),
        "{repair}"
    );
    assert!(repair.contains("math('_ + 1')"), "{repair}");
}

#[tokio::test]
async fn scenario_nl_to_gremlin_give_up_gracefully() {
    let db = ScriptedDb::new(vec![]);
    let llm = ScriptedLlm::new(vec![&ScriptedLlm::answer(
        "g.V().hasLabel('airports').values('code')",
        "table",
    )]);
    let (outcome, events) = go("airport codes", &llm, &db, &ctx(false)).await;
    let outcome = outcome.unwrap();
    assert!(!outcome.verified);
    assert_eq!(outcome.repairs, 3);
    assert_eq!(llm.requests().len(), 4, "one generation, three repairs");
    assert!(outcome.issues[0].message.contains("airports"));
    assert_eq!(
        outcome.answer.query, "g.V().hasLabel('airports').values('code')",
        "best attempt kept"
    );
    assert!(stages(&events).contains(&Stage::Repairing(3, 3)));
    assert!(db.scripts().is_empty(), "invalid queries are not run");
}

#[tokio::test]
async fn scenario_nl_to_gremlin_uses_real_labels() {
    let db = ScriptedDb::new(vec![("g.V()", Ok(vec![GValue::String("DFW".into())]))]);
    let llm = ScriptedLlm::new(vec![&ScriptedLlm::answer(
        "g.V().has('airport','code','AUS').out('route').values('code')",
        "table",
    )]);
    let (outcome, _) = go(
        "which airports can I fly to from Austin",
        &llm,
        &db,
        &ctx(false),
    )
    .await;
    assert!(outcome.unwrap().verified);
    let system = llm.requests()[0].system.clone().unwrap();
    for needle in [
        "airport ~3,504",
        "code:String",
        "airport -route-> airport",
        "country -contains-> airport",
    ] {
        assert!(system.contains(needle), "context has {needle}");
    }
    assert!(system.contains("Apache TinkerPop 3.7.3"), "dialect");
    // A query going the wrong way along `route` would have been caught by validation.
    let issues = assistant::check(
        "g.V().has('country','code','US').in('contains')",
        Some(&air()),
        &Dialect::default(),
    );
    assert!(!issues.is_empty());
}

#[tokio::test]
async fn scenario_nl_to_gremlin_cloud_model_without_sample_sharing() {
    let schema = air();
    let samples: Vec<String> = schema
        .vertex_labels
        .iter()
        .flat_map(|l| l.properties.iter().flat_map(|p| p.samples.clone()))
        // Values that are also label names (`type: 'airport'`) appear in the schema structure anyway.
        .filter(|s| s.len() > 3 && !schema.knows_label(s))
        .collect();
    let row = GValue::String("PRIVATE-ROW-VALUE".into());
    // First attempt returns rows for a different query; a 0-row result triggers a repair.
    let db = ScriptedDb::new(vec![
        ("'region','TX'", Ok(vec![])),
        ("'US-TX'", Ok(vec![row])),
    ]);
    let llm = ScriptedLlm::new(vec![
        &ScriptedLlm::answer("g.V().has('airport','region','TX').values('code')", "table"),
        &ScriptedLlm::answer(
            "g.V().has('airport','region','US-TX').values('code')",
            "table",
        ),
    ]);
    let (outcome, _) = go("airports in Texas", &llm, &db, &ctx(false)).await;
    assert!(outcome.unwrap().verified);
    for req in llm.requests() {
        let mut all = req.system.clone().unwrap_or_default();
        for m in &req.messages {
            all.push_str(&m.text());
        }
        for s in &samples {
            assert!(!all.contains(s.as_str()), "sample {s:?} was sent");
        }
        assert!(!all.contains("PRIVATE-ROW-VALUE"), "dry-run rows were sent");
    }
    let repair = llm.requests()[1].messages.last().unwrap().text();
    assert!(repair.contains("returned 0 rows"), "{repair}");
}

#[tokio::test]
async fn refinement_keeps_the_conversation() {
    let db = ScriptedDb::new(vec![("g.V()", Ok(vec![GValue::String("AUS".into())]))]);
    let first = ScriptedLlm::new(vec![&ScriptedLlm::answer(
        "g.V().has('airport','region','US-TX').values('code')",
        "table",
    )]);
    let (o1, _) = go("airports in Texas", &first, &db, &ctx(false)).await;
    let o1 = o1.unwrap();
    assert_eq!(o1.history.len(), 2);
    let second = ScriptedLlm::new(vec![&ScriptedLlm::answer(
        "g.V().has('airport','region','US-TX').has('runways', gt(3)).values('code')",
        "table",
    )]);
    let outcome = run(
        "only those with more than 3 runways",
        &o1.history,
        &ctx(false),
        &second,
        Some(&db),
        &Options::default(),
        &|_| {},
    )
    .await
    .unwrap();
    assert!(outcome.verified);
    let req = &second.requests()[0];
    assert_eq!(
        req.messages.len(),
        3,
        "earlier prompt, earlier answer, refinement"
    );
    assert!(req.messages[1].text().contains("US-TX"));
    assert_eq!(outcome.history.len(), 4);
}
