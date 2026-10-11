/**
 * tree-sitter-gremlin (ADR-0014).
 *
 * A deliberately generic grammar: a traversal is a root followed by `.segment` calls, so new Gremlin steps
 * never need grammar changes. Step knowledge (names, signatures, mutation flags, docs) lives in the
 * analysis layer's catalogue (`steps.toml`). Groovy closures are parsed as opaque `lambda` nodes.
 *
 * Regenerate with `cargo xtask grammar` (needs tree-sitter-cli; building the crate does not).
 */

const commaSep = (rule) => optional(seq(rule, repeat(seq(',', rule)), optional(',')));
const commaSep1 = (rule) => seq(rule, repeat(seq(',', rule)), optional(','));

module.exports = grammar({
  name: 'gremlin',

  extras: ($) => [/\s/, $.line_comment, $.block_comment],

  word: ($) => $.identifier,


  rules: {
    source_file: ($) => repeat(choice($._statement, ';')),

    _statement: ($) => choice($.assignment, $._expression),

    assignment: ($) =>
      seq(optional('def'), field('name', $.identifier), '=', field('value', $._expression)),

    _expression: ($) =>
      choice(
        $.traversal,
        $.identifier,
        $.string,
        $.number,
        $.boolean,
        $.null,
        $.list,
        $.map,
        $.lambda,
        $.parenthesized,
      ),

    // `g.V().out('knows')`, `__.out()`, `P.gt(30)`, `T.id`, and a bare anonymous `out('knows')`.
    traversal: ($) =>
      prec.left(
        choice(
          seq(field('root', $.identifier), repeat1(seq('.', $._segment))),
          seq(field('root', $.call), repeat(seq('.', $._segment))),
        ),
      ),

    _segment: ($) => choice($.call, alias($.identifier, $.property)),

    // An identifier directly followed by `(` or `{` is always a call.
    call: ($) =>
      prec.left(
        1,
        seq(
          field('name', $.identifier),
          choice(
            seq(field('arguments', $.arguments), optional(field('closure', $.lambda))),
            field('closure', $.lambda),
          ),
        ),
      ),

    arguments: ($) => seq('(', commaSep($._expression), ')'),

    parenthesized: ($) => seq('(', $._expression, ')'),

    list: ($) => seq('[', commaSep($._expression), ']'),

    map: ($) => seq('[', choice(commaSep1($.pair), ':'), ']'),

    pair: ($) => seq(field('key', $._map_key), ':', field('value', $._expression)),

    _map_key: ($) => choice($.identifier, $.string, $.number, $.parenthesized, $.traversal),

    // Opaque Groovy closure: balanced braces, contents not analysed.
    lambda: ($) => seq('{', repeat(choice($._lambda_text, $.lambda, $.string)), '}'),
    _lambda_text: (_) => token(prec(-1, /[^{}'"]+/)),

    identifier: (_) => /[A-Za-z_$][A-Za-z0-9_$]*/,

    string: (_) =>
      choice(
        seq("'", /([^'\\\n]|\\.)*/, "'"),
        seq('"', /([^"\\\n]|\\.)*/, '"'),
      ),

    // Integers, decimals, exponents, hex, and type suffixes: Groovy (L, d, f, g) and gremlin-lang (m, n, ...).
    number: (_) =>
      token(
        seq(
          optional('-'),
          choice(
            /0[xX][0-9a-fA-F]+[lL]?/,
            seq(/\d+(\.\d+)?/, optional(/[eE][+-]?\d+/), optional(/[lLdDfFmMnNbBsSiIgG]/)),
          ),
        ),
      ),

    boolean: (_) => choice('true', 'false'),
    null: (_) => 'null',

    line_comment: (_) => token(seq('//', /[^\n]*/)),
    block_comment: (_) => token(seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/')),
  },
});
