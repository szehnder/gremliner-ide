//! A tolerant scanner for completion: tokenises Gremlin up to the cursor (unfinished strings and calls
//! included) and tracks, per nesting level, the call being written, its arguments so far and the chain of
//! calls before it. Tree-sitter is the right tool for complete statements; half-typed text is the normal
//! case here, so this works on tokens instead.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tok {
    Ident,
    /// A string literal; `closed` is false when it runs to the end of the scanned text or line.
    Str {
        closed: bool,
    },
    Num,
    Dot,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Semicolon,
    /// A balanced `{ … }` closure, or any other character.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: Tok,
    pub span: Range<usize>,
}

/// Tokenise `text`, skipping whitespace and comments.
pub fn tokenize(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        let kind = match c {
            b' ' | b'\t' | b'\r' | b'\n' => {
                i += 1;
                continue;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i < bytes.len() && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/')) {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
                continue;
            }
            b'\'' | b'"' => {
                i += 1;
                let mut closed = false;
                while i < bytes.len() {
                    match bytes[i] {
                        b'\\' => i += 2,
                        b'\n' => break,
                        q if q == c => {
                            i += 1;
                            closed = true;
                            break;
                        }
                        _ => i += 1,
                    }
                }
                i = i.min(bytes.len());
                Tok::Str { closed }
            }
            b'{' => {
                let mut depth = 0usize;
                while i < bytes.len() {
                    match bytes[i] {
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                i += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                Tok::Other
            }
            b'.' => {
                i += 1;
                Tok::Dot
            }
            b'(' => {
                i += 1;
                Tok::LParen
            }
            b')' => {
                i += 1;
                Tok::RParen
            }
            b'[' => {
                i += 1;
                Tok::LBracket
            }
            b']' => {
                i += 1;
                Tok::RBracket
            }
            b',' => {
                i += 1;
                Tok::Comma
            }
            b';' => {
                i += 1;
                Tok::Semicolon
            }
            c if c.is_ascii_digit()
                || (c == b'-' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)) =>
            {
                i += 1;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.' || bytes[i] == b'_')
                {
                    // `1.5` is a number, but `1.toString` style calls do not occur in Gremlin.
                    if bytes[i] == b'.' && !bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                        break;
                    }
                    i += 1;
                }
                Tok::Num
            }
            c if c.is_ascii_alphabetic() || c == b'_' || c == b'$' => {
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
                {
                    i += 1;
                }
                Tok::Ident
            }
            _ => {
                // One character, whole (multi-byte safe).
                i += text[i..].chars().next().map_or(1, char::len_utf8);
                Tok::Other
            }
        };
        out.push(Token {
            kind,
            span: start..i,
        });
    }
    out
}

/// The value of a string literal token (quotes removed, simple escapes resolved).
pub fn string_value(raw: &str) -> String {
    let mut chars = raw.chars();
    let quote = chars.next();
    let mut body: String = chars.collect();
    if quote.is_some_and(|q| body.ends_with(q)) {
        body.pop();
    }
    let mut out = String::with_capacity(body.len());
    let mut escaped = false;
    for c in body.chars() {
        if escaped {
            out.push(match c {
                'n' => '\n',
                't' => '\t',
                other => other,
            });
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else {
            out.push(c);
        }
    }
    out
}

/// One argument of a call, as far as completion cares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg {
    /// The string value, when the argument is a single string literal.
    pub string: Option<String>,
    /// Inside the quotes, for strings; the whole argument otherwise.
    pub span: Range<usize>,
    /// The argument's source text.
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub name: String,
    pub name_span: Range<usize>,
    pub args: Vec<Arg>,
}

/// A chain of calls: `g.V().out()` has root `g`; an anonymous `out().in()` has none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chain {
    pub root: Option<String>,
    pub calls: Vec<Call>,
}

impl Chain {
    /// Rooted at `__` or not rooted at all: it continues from the enclosing step.
    pub fn is_anonymous(&self) -> bool {
        matches!(self.root.as_deref(), None | Some("__"))
    }
}

/// One nesting level on the way to the cursor: the chain written so far at that level, and the call
/// whose parentheses the next level is inside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Level {
    pub chain: Chain,
    pub call: Call,
}

/// Every closed call in the scanned text, with the levels that lead to it (for schema warnings).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSite {
    /// Enclosing levels, outermost first, ending with the chain this call belongs to (whose `call` is
    /// this call).
    pub levels: Vec<Level>,
}

#[derive(Debug, Default)]
struct Acc {
    tokens: usize,
    string: Option<(String, Range<usize>)>,
    start: Option<usize>,
    end: usize,
}

#[derive(Debug, Default)]
struct Frame {
    call: Option<(String, Range<usize>)>,
    args: Vec<Arg>,
    acc: Acc,
    chain: Chain,
    after_dot: bool,
    pending: Option<(String, Range<usize>)>,
}

impl Frame {
    fn finish_arg(&mut self, text: &str) {
        if self.acc.tokens == 0 {
            return;
        }
        let start = self.acc.start.unwrap_or(self.acc.end);
        let (string, span) = match self.acc.string.take() {
            Some((value, inner)) if self.acc.tokens == 1 => (Some(value), inner),
            _ => (None, start..self.acc.end),
        };
        self.args.push(Arg {
            string,
            span,
            raw: text[start..self.acc.end].to_owned(),
        });
        self.acc = Acc::default();
        self.chain = Chain::default();
        self.pending = None;
        self.after_dot = false;
    }

    fn note(&mut self, span: &Range<usize>) {
        self.acc.tokens += 1;
        self.acc.start.get_or_insert(span.start);
        self.acc.end = span.end;
    }
}

/// What the scanner saw at the end of the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tail {
    /// After `.` (and a partial name): complete a step of `chain`.
    AfterDot { chain: Chain },
    /// Inside an unfinished string argument.
    InString { args: Vec<Arg>, index: usize },
    /// At an argument, before any string (`hasLabel(`, `has('a', P.`…).
    InArg { args: Vec<Arg>, index: usize },
    /// Anywhere else.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scan {
    /// Enclosing calls, outermost first: `levels.last().call` is the call the cursor is inside.
    pub levels: Vec<Level>,
    pub tail: Tail,
    /// The partial word (or string content) being typed and where it is.
    pub partial: String,
    pub replace: Range<usize>,
    /// Closed calls, when scanning for warnings.
    pub sites: Vec<CallSite>,
}

/// Scan `text` (everything before the cursor, or a whole document with `collect_sites`).
pub fn scan(text: &str, collect_sites: bool) -> Scan {
    let tokens = tokenize(text);
    let mut stack: Vec<Frame> = vec![Frame::default()];
    let mut sites = Vec::new();
    let end = text.len();
    for tok in &tokens {
        let top = stack.len() - 1;
        match tok.kind {
            Tok::Ident => {
                let name = text[tok.span.clone()].to_owned();
                let frame = &mut stack[top];
                if !frame.after_dot
                    && (frame.pending.is_some()
                        || !frame.chain.calls.is_empty()
                        || frame.chain.root.is_some())
                {
                    // A new expression at this level (e.g. the next statement on its own line).
                    frame.chain = Chain::default();
                }
                frame.after_dot = false;
                frame.pending = Some((name, tok.span.clone()));
                frame.note(&tok.span);
            }
            Tok::Dot => {
                let frame = &mut stack[top];
                if let Some((name, span)) = frame.pending.take() {
                    if frame.chain.root.is_none() && frame.chain.calls.is_empty() {
                        frame.chain.root = Some(name);
                    } else {
                        // A property segment (`T.id`, `.inV.`): keep it as an argument-less call.
                        frame.chain.calls.push(Call {
                            name,
                            name_span: span,
                            args: Vec::new(),
                        });
                    }
                }
                frame.after_dot = true;
                frame.note(&tok.span);
            }
            Tok::LParen => {
                let frame = &mut stack[top];
                let call = frame.pending.take();
                frame.after_dot = false;
                frame.note(&tok.span);
                stack.push(Frame {
                    call,
                    ..Frame::default()
                });
            }
            Tok::LBracket => {
                stack[top].note(&tok.span);
                stack.push(Frame::default());
            }
            Tok::RParen | Tok::RBracket => {
                if stack.len() == 1 {
                    stack[0].note(&tok.span);
                    continue;
                }
                let mut frame = stack.pop().unwrap_or_default();
                frame.finish_arg(text);
                let parent = stack.last_mut().unwrap_or_else(|| unreachable!());
                parent.note(&tok.span);
                if tok.kind == Tok::RParen
                    && let Some((name, name_span)) = frame.call
                {
                    let call = Call {
                        name,
                        name_span,
                        args: frame.args,
                    };
                    if collect_sites {
                        let mut levels: Vec<Level> = Vec::with_capacity(stack.len());
                        for pair in stack.windows(2) {
                            if let Some((name, span)) = &pair[1].call {
                                levels.push(Level {
                                    chain: pair[0].chain.clone(),
                                    call: Call {
                                        name: name.clone(),
                                        name_span: span.clone(),
                                        args: pair[1].args.clone(),
                                    },
                                });
                            }
                        }
                        let parent = stack.last().unwrap_or_else(|| unreachable!());
                        levels.push(Level {
                            chain: parent.chain.clone(),
                            call: call.clone(),
                        });
                        sites.push(CallSite { levels });
                    }
                    let parent = stack.last_mut().unwrap_or_else(|| unreachable!());
                    parent.chain.calls.push(call);
                }
            }
            Tok::Comma => {
                stack[top].finish_arg(text);
            }
            Tok::Semicolon => {
                if stack.len() == 1 {
                    stack[0] = Frame::default();
                } else {
                    stack[top].note(&tok.span);
                }
            }
            Tok::Str { .. } => {
                let frame = &mut stack[top];
                frame.note(&tok.span);
                if frame.acc.tokens == 1 {
                    let raw = &text[tok.span.clone()];
                    let inner_start = tok.span.start + 1;
                    let closed = matches!(tok.kind, Tok::Str { closed: true });
                    let inner_end = if closed {
                        tok.span.end - 1
                    } else {
                        tok.span.end
                    }
                    .max(inner_start);
                    frame.acc.string = Some((string_value(raw), inner_start..inner_end));
                }
                frame.pending = None;
                frame.after_dot = false;
            }
            Tok::Num | Tok::Other => {
                let frame = &mut stack[top];
                frame.note(&tok.span);
                frame.pending = None;
                frame.after_dot = false;
            }
        }
    }

    // Levels to the cursor.
    let mut levels = Vec::with_capacity(stack.len());
    for pair in stack.windows(2) {
        if let Some((name, span)) = &pair[1].call {
            levels.push(Level {
                chain: pair[0].chain.clone(),
                call: Call {
                    name: name.clone(),
                    name_span: span.clone(),
                    args: pair[1].args.clone(),
                },
            });
        }
    }
    let frame = stack.last().unwrap_or_else(|| unreachable!());
    let last = tokens.last();
    let at_end = |t: &Token| t.span.end == end;
    let in_call = frame.call.is_some();
    let (tail, partial, replace) = match last {
        // `….na|` after a dot, or `….|`
        // `….na|`: the partial name is pending, not yet part of the chain.
        Some(t)
            if t.kind == Tok::Ident
                && at_end(t)
                && frame.pending.is_some()
                && is_after_dot(&tokens) =>
        {
            (
                Tail::AfterDot {
                    chain: frame.chain.clone(),
                },
                text[t.span.clone()].to_owned(),
                t.span.clone(),
            )
        }
        Some(t) if t.kind == Tok::Dot && at_end(t) => (
            Tail::AfterDot {
                chain: frame.chain.clone(),
            },
            String::new(),
            end..end,
        ),
        Some(t)
            if matches!(t.kind, Tok::Str { closed: false })
                && at_end(t)
                && in_call
                && frame.acc.tokens == 1 =>
        {
            let (value, inner) = frame.acc.string.clone().unwrap_or_default();
            (
                Tail::InString {
                    args: frame.args.clone(),
                    index: frame.args.len(),
                },
                value,
                inner,
            )
        }
        Some(t) if t.kind == Tok::Ident && at_end(t) && in_call && frame.acc.tokens == 1 => (
            Tail::InArg {
                args: frame.args.clone(),
                index: frame.args.len(),
            },
            text[t.span.clone()].to_owned(),
            t.span.clone(),
        ),
        Some(t) if in_call && matches!(t.kind, Tok::LParen | Tok::Comma) => (
            Tail::InArg {
                args: frame.args.clone(),
                index: frame.args.len(),
            },
            String::new(),
            end..end,
        ),
        Some(t) if t.kind == Tok::Ident && at_end(t) => {
            (Tail::Other, text[t.span.clone()].to_owned(), t.span.clone())
        }
        _ => (Tail::Other, String::new(), end..end),
    };
    Scan {
        levels,
        tail,
        partial,
        replace,
        sites,
    }
}

/// The last token is an identifier directly after a `.`.
fn is_after_dot(tokens: &[Token]) -> bool {
    tokens.len() >= 2 && tokens[tokens.len() - 2].kind == Tok::Dot
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(chain: &Chain) -> Vec<&str> {
        chain.calls.iter().map(|c| c.name.as_str()).collect()
    }

    #[test]
    fn tokens_cover_strings_numbers_and_closures() {
        let t = tokenize("g.V(1, 'a\\'b').map{ it.get() }.is(-1.5) // c");
        let kinds: Vec<Tok> = t.iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            [
                Tok::Ident,
                Tok::Dot,
                Tok::Ident,
                Tok::LParen,
                Tok::Num,
                Tok::Comma,
                Tok::Str { closed: true },
                Tok::RParen,
                Tok::Dot,
                Tok::Ident,
                Tok::Other,
                Tok::Dot,
                Tok::Ident,
                Tok::LParen,
                Tok::Num,
                Tok::RParen,
            ]
        );
        assert_eq!(string_value("'a\\'b'"), "a'b");
    }

    #[test]
    fn after_dot_with_partial() {
        let s = scan("g.V().hasLabel('airport').ha", false);
        let Tail::AfterDot { chain } = &s.tail else {
            panic!("{s:?}")
        };
        assert_eq!(chain.root.as_deref(), Some("g"));
        assert_eq!(names(chain), ["V", "hasLabel"]);
        assert_eq!(chain.calls[1].args[0].string.as_deref(), Some("airport"));
        assert_eq!(s.partial, "ha");
    }

    #[test]
    fn source_position() {
        let s = scan("g.", false);
        let Tail::AfterDot { chain } = &s.tail else {
            panic!()
        };
        assert_eq!(chain.root.as_deref(), Some("g"));
        assert!(chain.calls.is_empty());
    }

    #[test]
    fn unfinished_string_argument() {
        let s = scan("g.V().has('airport','code','A", false);
        let Tail::InString { args, index } = &s.tail else {
            panic!("{s:?}")
        };
        assert_eq!(*index, 2);
        assert_eq!(args[1].string.as_deref(), Some("code"));
        assert_eq!(s.partial, "A");
        let level = s.levels.last().unwrap();
        assert_eq!(level.call.name, "has");
        assert_eq!(names(&level.chain), ["V"]);
    }

    #[test]
    fn nested_anonymous_traversal() {
        let s = scan("g.V().hasLabel('airport').where(__.out('route').", false);
        assert_eq!(s.levels.len(), 1);
        assert_eq!(s.levels[0].call.name, "where");
        assert_eq!(names(&s.levels[0].chain), ["V", "hasLabel"]);
        let Tail::AfterDot { chain } = &s.tail else {
            panic!()
        };
        assert_eq!(chain.root.as_deref(), Some("__"));
        assert_eq!(names(chain), ["out"]);
    }

    #[test]
    fn argument_start_and_predicate_root() {
        let s = scan("g.V().has('age', P.", false);
        let Tail::AfterDot { chain } = &s.tail else {
            panic!("{s:?}")
        };
        assert_eq!(chain.root.as_deref(), Some("P"));
        let s = scan("g.V().hasLabel(", false);
        assert!(matches!(s.tail, Tail::InArg { index: 0, .. }));
        let s = scan("g.V().has('airport', ", false);
        assert!(matches!(s.tail, Tail::InArg { index: 1, .. }));
    }

    #[test]
    fn new_statement_on_next_line() {
        let s = scan("g.V().count()\ng.", false);
        let Tail::AfterDot { chain } = &s.tail else {
            panic!()
        };
        assert!(chain.calls.is_empty());
        let s = scan("g.V()\n  .out()\n  .", false);
        let Tail::AfterDot { chain } = &s.tail else {
            panic!()
        };
        assert_eq!(names(chain), ["V", "out"]);
    }

    #[test]
    fn sites_record_every_closed_call() {
        let s = scan("g.V().hasLabel('x').where(out('y')).values('z')", true);
        let calls: Vec<&str> = s
            .sites
            .iter()
            .map(|site| site.levels.last().unwrap().call.name.as_str())
            .collect();
        assert_eq!(calls, ["V", "hasLabel", "out", "where", "values"]);
        let out = &s.sites[2];
        assert_eq!(out.levels.len(), 2);
        assert_eq!(out.levels[0].call.name, "where");
    }
}
