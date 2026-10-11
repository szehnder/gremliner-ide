; Gremlin highlight captures. Later patterns win for the same node.

(line_comment) @comment
(block_comment) @comment
(string) @string
(number) @number
(boolean) @boolean
(null) @constant.builtin

(call name: (identifier) @function.method)

(traversal root: (identifier) @variable)
((traversal root: (identifier) @variable.builtin)
  (#any-of? @variable.builtin "g" "__"))
((traversal root: (identifier) @type)
  (#match? @type "^(P|TextP|T|Order|Scope|Column|Pop|Cardinality|Direction|Operator|Merge|Pick|Barrier|WithOptions|DT)$"))

(property) @constant.builtin

((identifier) @constant.builtin
  (#any-of? @constant.builtin "local" "global" "asc" "desc" "shuffle" "single" "list" "set" "first" "last" "mixed" "OUT" "IN" "BOTH" "keys" "values" "label" "id"))

(assignment name: (identifier) @variable)
"def" @keyword

(lambda) @embedded
["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["." "," ":" ";"] @punctuation.delimiter
"=" @operator
