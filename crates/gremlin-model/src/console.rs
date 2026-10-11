//! Gremlin Console-style text for result values (`v[1]`, `e[7][1-knows->2]`, `[name:[marko]]`), as printed
//! by `gremlin.sh` after `==>`.

use std::fmt::Write as _;

use crate::{GKey, GValue};

/// One result item as a console line, without the `==>` prefix.
pub fn format(value: &GValue) -> String {
    let mut out = String::new();
    write_value(&mut out, value);
    out
}

/// The console line for a result item: `==>` + [`format`].
pub fn line(value: &GValue) -> String {
    format!("==>{}", format(value))
}

fn write_value(out: &mut String, value: &GValue) {
    match value {
        GValue::Null => out.push_str("null"),
        GValue::Bool(b) => {
            let _ = write!(out, "{b}");
        }
        GValue::Int32(n) => {
            let _ = write!(out, "{n}");
        }
        GValue::Int64(n) => {
            let _ = write!(out, "{n}");
        }
        GValue::Short(n) => {
            let _ = write!(out, "{n}");
        }
        GValue::Byte(n) => {
            let _ = write!(out, "{n}");
        }
        GValue::Float(f) => write_float(out, f64::from(*f)),
        GValue::Double(f) => write_float(out, *f),
        GValue::BigDecimal(s) | GValue::BigInteger(s) | GValue::Duration(s) | GValue::String(s) => {
            out.push_str(s);
        }
        GValue::Char(c) => out.push(*c),
        GValue::Uuid(u) => {
            let _ = write!(out, "{u}");
        }
        GValue::Date(ms) | GValue::Timestamp(ms) => out.push_str(&iso_millis(*ms)),
        GValue::Bytes(b) => {
            let _ = write!(out, "[{} bytes]", b.len());
        }
        GValue::List(items) | GValue::Set(items) => write_list(out, items),
        GValue::BulkSet(items) => {
            let expanded: Vec<GValue> = items
                .iter()
                .flat_map(|(v, n)| std::iter::repeat_n(v.clone(), usize::try_from(*n).unwrap_or(0)))
                .collect();
            write_list(out, &expanded);
        }
        GValue::Map(map) => {
            out.push('[');
            if map.is_empty() {
                out.push(':');
            }
            for (i, (k, v)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_key(out, k);
                out.push(':');
                write_value(out, v);
            }
            out.push(']');
        }
        GValue::Vertex(v) => {
            out.push_str("v[");
            write_value(out, &v.id);
            out.push(']');
        }
        GValue::Edge(e) => {
            out.push_str("e[");
            write_value(out, &e.id);
            out.push_str("][");
            write_value(out, &e.out_v);
            let _ = write!(out, "-{}->", e.label);
            write_value(out, &e.in_v);
            out.push(']');
        }
        GValue::VertexProperty(p) => {
            let _ = write!(out, "vp[{}->", p.label);
            write_value(out, &p.value);
            out.push(']');
        }
        GValue::Property(p) => {
            let _ = write!(out, "p[{}->", p.key);
            write_value(out, &p.value);
            out.push(']');
        }
        GValue::Path(p) => {
            out.push_str("path[");
            for (i, o) in p.objects.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_value(out, o);
            }
            out.push(']');
        }
        GValue::Tree(entries) => {
            out.push('[');
            for (i, (k, v)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(out, k);
                out.push(':');
                write_value(out, v);
            }
            out.push(']');
        }
        GValue::Traverser { value, .. } => write_value(out, value),
        GValue::T(t) => {
            let _ = write!(out, "{t}");
        }
        GValue::Direction(d) => {
            let _ = write!(out, "{d}");
        }
        GValue::Token { value, .. } | GValue::Class(value) => out.push_str(value),
        GValue::TraversalMetrics(m) | GValue::Metrics(m) => {
            let _ = write!(
                out,
                "Traversal Metrics ({} steps, {:.3} ms)",
                m.metrics.len(),
                m.dur_ms
            );
        }
        GValue::Unknown { value, .. } => out.push_str(&value.to_string()),
    }
}

fn write_list(out: &mut String, items: &[GValue]) {
    out.push('[');
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_value(out, item);
    }
    out.push(']');
}

fn write_key(out: &mut String, key: &GKey) {
    match key {
        GKey::Null => out.push_str("null"),
        GKey::Bool(b) => {
            let _ = write!(out, "{b}");
        }
        GKey::Int(n) => {
            let _ = write!(out, "{n}");
        }
        GKey::String(s) | GKey::Other(s) => out.push_str(s),
        GKey::Uuid(u) => {
            let _ = write!(out, "{u}");
        }
        GKey::T(t) => {
            let _ = write!(out, "{t}");
        }
        GKey::Direction(d) => {
            let _ = write!(out, "{d}");
        }
        GKey::Token { value, .. } => out.push_str(value),
        GKey::Vertex { id, .. } => {
            out.push_str("v[");
            write_key(out, id);
            out.push(']');
        }
        GKey::Edge { id, .. } => {
            out.push_str("e[");
            write_key(out, id);
            out.push(']');
        }
    }
}

/// Java's `Double.toString` style: integral values keep a trailing `.0`.
fn write_float(out: &mut String, f: f64) {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e16 {
        let _ = write!(out, "{f:.1}");
    } else {
        let _ = write!(out, "{f}");
    }
}

/// Milliseconds since the epoch → `2024-01-31T12:00:00.000Z`.
pub fn iso_millis(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}.{millis:03}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Edge, Path, Vertex, new_map};

    fn vertex(id: i64) -> GValue {
        GValue::Vertex(Vertex {
            id: Box::new(GValue::Int64(id)),
            label: "person".into(),
            properties: new_map(),
        })
    }

    #[test]
    fn scenario_results_console_console_formatting() {
        assert_eq!(line(&vertex(1)), "==>v[1]");
        let edge = GValue::Edge(Edge {
            id: Box::new(GValue::Int64(7)),
            label: "knows".into(),
            out_v: Box::new(GValue::Int64(1)),
            out_v_label: "person".into(),
            in_v: Box::new(GValue::Int64(2)),
            in_v_label: "person".into(),
            properties: new_map(),
        });
        assert_eq!(format(&edge), "e[7][1-knows->2]");
        assert_eq!(line(&GValue::Int32(29)), "==>29");
        assert_eq!(format(&GValue::Double(0.5)), "0.5");
        assert_eq!(format(&GValue::Double(1.0)), "1.0");
        let mut map = new_map();
        map.insert(
            GKey::String("name".into()),
            GValue::List(vec![GValue::String("marko".into())]),
        );
        map.insert(
            GKey::String("age".into()),
            GValue::List(vec![GValue::Int32(29)]),
        );
        assert_eq!(format(&GValue::Map(map)), "[name:[marko],age:[29]]");
        assert_eq!(format(&GValue::Map(new_map())), "[:]");
        let path = GValue::Path(Path {
            labels: vec![vec![], vec![]],
            objects: vec![vertex(1), vertex(2)],
        });
        assert_eq!(format(&path), "path[v[1], v[2]]");
    }

    #[test]
    fn dates_are_iso() {
        assert_eq!(iso_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_millis(1_706_702_400_123), "2024-01-31T12:00:00.123Z");
        assert_eq!(iso_millis(-1), "1969-12-31T23:59:59.999Z");
    }
}
