//! Query history storage. Scenario names map to
//! `openspec/changes/query-history-saved/specs/query-history/spec.md`.

#![allow(clippy::unwrap_used)]

use std::time::Instant;

use storage::{HistoryEntry, HistoryQuery, Outcome, Storage};

fn entry(text: &str, conn: &str, outcome: Outcome, at: i64) -> HistoryEntry {
    HistoryEntry {
        id: 0,
        connection_id: Some(conn.into()),
        connection_name: conn.to_uppercase(),
        text: text.into(),
        started_at: at,
        duration_ms: 12,
        outcome,
        count: 1,
        error: None,
    }
}

fn search(s: &Storage, text: &str) -> Vec<HistoryEntry> {
    s.search_history_blocking(HistoryQuery {
        text: text.into(),
        ..Default::default()
    })
    .unwrap()
}

#[test]
fn scenario_query_history_entry_recorded() {
    let s = Storage::open_in_memory().unwrap();
    s.record_history_blocking(entry("g.V().count()", "local", Outcome::Success, 1), 100)
        .unwrap();
    let all = search(&s, "");
    assert_eq!(all.len(), 1);
    let e = &all[0];
    assert_eq!(e.text, "g.V().count()");
    assert_eq!(e.connection_name, "LOCAL");
    assert_eq!(e.outcome, Outcome::Success);
    assert_eq!(e.count, 1);
}

#[test]
fn search_filters_and_order() {
    let s = Storage::open_in_memory().unwrap();
    s.record_history_blocking(entry("g.V().out('route')", "a", Outcome::Success, 1), 100)
        .unwrap();
    s.record_history_blocking(entry("g.V().count()", "a", Outcome::Error, 2), 100)
        .unwrap();
    s.record_history_blocking(
        entry("g.E().hasLabel('route')", "b", Outcome::Success, 3),
        100,
    )
    .unwrap();
    let texts: Vec<String> = search(&s, "route").into_iter().map(|e| e.text).collect();
    assert_eq!(
        texts,
        ["g.E().hasLabel('route')", "g.V().out('route')"],
        "newest first"
    );
    let by_conn = s
        .search_history_blocking(HistoryQuery {
            text: "route".into(),
            connection_id: Some("a".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(by_conn.len(), 1);
    let errors = s
        .search_history_blocking(HistoryQuery {
            outcome: Some(Outcome::Error),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(errors[0].text, "g.V().count()");
    // Short needles fall back to LIKE (trigrams need three characters); wildcards are literal.
    assert_eq!(search(&s, "E(").len(), 1);
    assert!(search(&s, "%").is_empty());
}

#[test]
fn scenario_query_history_search_speed() {
    let s = Storage::open_in_memory().unwrap();
    s.call_blocking(|conn| {
        let tx = conn.transaction()?;
        for i in 0..10_000 {
            let text = if i % 50 == 0 {
                format!("g.V().has('code','X{i}').out('route').count()")
            } else {
                format!("g.V().has('airport','code','A{i}').values('city')")
            };
            tx.execute(
                "INSERT INTO history (connection_id, connection_name, text, started_at, duration_ms, outcome, count)
                 VALUES ('c', 'C', ?1, ?2, 5, 'success', 1)",
                storage::rusqlite::params![text, i],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
    .unwrap();
    let started = Instant::now();
    let found = s
        .search_history_blocking(HistoryQuery {
            text: "route".into(),
            limit: 500,
            ..Default::default()
        })
        .unwrap();
    let elapsed = started.elapsed();
    assert_eq!(found.len(), 200);
    assert!(elapsed.as_millis() < 100, "search took {elapsed:?}");
}

#[test]
fn scenario_query_history_retention_limit() {
    let s = Storage::open_in_memory().unwrap();
    for i in 0..5 {
        s.record_history_blocking(entry(&format!("q{i}"), "c", Outcome::Success, i), 5)
            .unwrap();
    }
    assert_eq!(s.history_len_blocking().unwrap(), 5);
    s.record_history_blocking(entry("q5", "c", Outcome::Success, 5), 5)
        .unwrap();
    let texts: Vec<String> = search(&s, "").into_iter().map(|e| e.text).collect();
    assert_eq!(
        texts,
        ["q5", "q4", "q3", "q2", "q1"],
        "the oldest was removed"
    );
    assert!(search(&s, "q0").is_empty(), "the index forgot it too");
    futures::executor::block_on(s.clear_history()).unwrap();
    assert_eq!(s.history_len_blocking().unwrap(), 0);
}
