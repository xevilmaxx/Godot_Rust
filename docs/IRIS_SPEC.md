# Iris — AI-native scripting for godot_rs (v1, replaces GDScript/C#)

## Why not GDScript or C#?

| | GDScript | C# (mono) | Iris |
|---|---|---|---|
| Deterministic parse for LLMs | ❌ indentation + dynamic | ❌ huge grammar | ✅ 9 statements, braces |
| Toolchain | ✅ none | ❌ .NET SDK | ✅ none (built in) |
| JSON-schema API export | ❌ | ❌ | ✅ `editor doc` → `iris.json` |
| Machine diagnostics | ❌ | ❌ | ✅ `editor check` JSON |

## Example

```iris
# demo.iris
node MySprite : Node2D {
  prop speed: float = 200.0
  prop title: str = "hello"

  fn _ready() {
    print title
  }

  fn _process(dt: float) {
    position.x += speed * dt
    if position.x > 500 {
      position.x = 0
    }
  }
}
```

## Grammar (v1)

```
file     := "node" IDENT ":" IDENT "{" item* "}"
item     := prop | fn
prop     := "prop" IDENT ":" TYPE "=" LITERAL
fn       := "fn" IDENT "(" args ")" "{" stmt* "}"
args     := (IDENT ":" TYPE ("," IDENT ":" TYPE)*)?
stmt     := let | assign | print | emit | if | while | for | return | expr ";"
let      := "let" IDENT ("=" expr)            # "let x = 1", optional ": type"
assign   := TARGET ("=" | "+=" | "-=" | "*=" | "/=" | "%=") expr
print    := "print" expr
emit     := "emit" expr                        # observable log line "emit ..."
if       := "if" expr "{" stmt* "}" ("else" "{" stmt* "}")?
while    := "while" expr "{" stmt* "}"         # max 10,000 iterations
for      := "for" IDENT "in" expr "{" stmt* "}"# array/int range, max 100k items
return   := "return" expr?
expr     := or-expr                            # see operator table below
```

Operators (GDScript precedence): `*` `/` `%` `**` bind tightest, then `+` `-`,
comparisons (`==` `!=` `<` `<=` `>` `>=`), `not`, `and`, `or`. Unary `-`/`not`,
member (`position.x`), index (`a[0]`), calls, parens. `#` starts a comment
(unless inside a string).

Builtins: `len str int float abs min max clamp lerp vec2 vec3 color range`.
Types: `nil bool int float str vec2 vec3 vec4 color rect2 nodepath array dict`
(`check` warns on unknown prop types with fix hints — agents use this to self-repair).

## Lifecycle

`_ready` runs once on first frame, `_process(dt)` every idle frame (needs
`enable_process(id, true, _)`), `_physics_process(dt)` every physics tick.
`SceneTree` presets `dt`, `name`, node props (`position`, `rotation`, `scale`,
`visible`, …) and script `prop` defaults as variables before each call, and
writes back changed node props — `position.x += speed * dt` moves the node.

## AI contract

1. Agent calls MCP `inspect` → gets ClassDB JSON + scene summary.
2. Agent calls MCP `iris_doc` (or `editor doc`) → gets node schema + grammar ref.
3. Agent writes `.iris` → runs `editor check file.iris` → fixes diagnostics.
4. Agent runs `editor run file.iris` → observes `print` output.
5. Agent calls `set_parameter` / `execute` to apply to live scene.
6. `choose model` persists per-agent last choice (SolidCraft pattern).

## Roadmap

- v1 (this commit): full expressions, member/index assignment, bounded
  `while`/`for`, `else`/`else if`, builtins, `Value`↔`Variant` bridge, scene
  prop preset/write-back, `doc_json`.
- v2: string interpolation, `match`, closures, bytecode compiler, debugger
  parity (`core/debugger/` + `scene/debugger/`), LSP hooks.
