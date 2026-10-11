; Gremlin highlight captures, using the capture names gpui-kit's highlighter themes (function,
; variable.special, type, constant, ...). The first matching pattern wins for a node, so specific
; patterns come before general ones.

(line_comment) @comment
(block_comment) @comment
(string) @string
(number) @number
(boolean) @boolean
(null) @constant

(call name: (identifier) @function)

((traversal root: (identifier) @variable.special)
  (#any-of? @variable.special "g" "__"))
((traversal root: (identifier) @type)
  (#match? @type "^(P|TextP|T|Order|Scope|Column|Pop|Cardinality|Direction|Operator|Merge|Pick|Barrier|WithOptions|DT)$"))

(traversal root: (identifier) @variable)

(property) @constant

((identifier) @constant
  (#any-of? @constant "local" "global" "asc" "desc" "shuffle" "single" "list" "set" "first" "last" "mixed" "OUT" "IN" "BOTH" "keys" "values" "label" "id"))

(assignment name: (identifier) @variable)
"def" @keyword

(lambda) @embedded
["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["." "," ":" ";"] @punctuation.delimiter
"=" @operator
