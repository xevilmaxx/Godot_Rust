# GDScript → Iris coverage

The transpiler (`godot_script_iris::gdtranspile`) imports real-world GDScript
into Iris. This file is the honest contract: what converts 1:1, what desugars,
and what is explicitly unsupported (with the reason). `Import .gd` in the
editor and the MCP `gd_transpile` tool both use it.

Legend: ✅ full support · 🔁 desugared to Iris · ❌ rejected with a message.

## Module / class level

| GDScript | Iris | Status |
|---|---|---|
| `extends BaseClass` | `node` base class | ✅ |
| `class_name X` | `node` name | ✅ |
| `var x / @export var x: T = d` | `prop x: T = d` | ✅ (member vars become props) |
| `const X = v` | `prop X` | 🔁 documented as mutable prop |
| `signal s(...)` | — | ❌ declarations have no Iris equivalent; `emit_signal("s")` → `emit s` ✅ |
| `func f(a: T, b = d) -> R:` | `fn f(a: T)` | ✅ (defaults/returns stripped, dynamic) |
| `static func` | plain `fn` | 🔁 |
| `pass` | (dropped) | ✅ |

## Statements

| GDScript | Iris | Status |
|---|---|---|
| `if/elif/else` | nested `if/else` | ✅ arbitrary chains |
| `while cond:` | `while` (10k-iteration cap) | ✅ |
| `for i in range(n)` / `for x in arr` | `for` (100k-item cap) | ✅ |
| `match` with literal patterns, `_,` default | `if/elif` chain on temp | 🔁 desugared |
| `match` binding patterns | — | ❌ rejected |
| `return [v]`, `print(...)`, `emit_signal("s")` | same-shape | ✅ |
| `x = y`, `+= -= *= /= %=` | same operators | ✅ |
| ternary `a if c else b` in let/assign/return/print | `if/else` | 🔁 desugared |
| `await`, `yield`, coroutines | — | ❌ rejected (no async runtime yet) |
| `super`, `is`, lambdas | — | ❌ rejected |
| `%` string formatting | — | ❌ rejected (use `+` concat) |

## Expressions

`+-*/% **` `== != < <= > >=` `and or not` `in`, member access (`a.b`),
indexing (`a[0]`), calls, parens, numbers/strings/bools/`null`, array literals.
`$Node/Path` becomes the path string (documented: node references are strings
until Iris gains first-class node handles). `PI/TAU/INF` map to literals.
`@export`/`@onready` annotations are accepted and ignored (documented).

## What "full compat" still needs

1. Coroutine runtime (`await` on signals/timers) — the hard one.
2. First-class node references + `$` path resolution against the tree.
3. `match` binding patterns and guards.
4. Typed arrays/dictionaries enforcement (currently dynamic).
5. The GDScript standard library surface (`String` methods, `FileAccess`, … —
   only the Iris builtin set exists today).

Progress is tracked by the transpiler test-suite in
`crates/godot_script_iris/src/gdtranspile.rs` (`basic_script_transpiles_and_runs`,
`match_and_ternary_desugar`, `for_range_and_while`, `rejects_await_and_bad_indent`).
