# godot_script_iris — Iris scripting (module 06, COMPLETE, v1)

Replaces `modules/gdscript` + `modules/mono` with **Iris**, an AI-native language
specified for reliable LLM generation (9 statements, braces, static Variant types,
deterministic interpreter, `iris check` diagnostics, `iris doc` JSON schema).

Full grammar: `docs/IRIS_SPEC.md`.

## v1 language

```iris
node Mover : Node2D {
  prop speed: float = 200.0
  fn _process(dt: float) {
    position.x += speed * dt
    if position.x > 500 { position.x = 0 }
    for i in range(3) { print i }
    while speed > 1000 { let speed = speed - 1 }
  }
}
```

Expressions: `+-*/% **` `== != < <= > >=` `and or not`, member (`a.b`),
index (`a[0]`), calls (`len str int float abs min max clamp lerp vec2 vec3
color range`), literals, parens. Statements: `let/assign/print/emit/if/while/
for/return/expr`. Loops cap at 10k iterations.

## API

```rust
let file = parse(src)?;            // IrisFile { node_class, props, fns }
let diags = check(&file);          // machine-readable diagnostics
let doc = doc_json(&file);         // agent schema: props/fns/grammar
let mut it = Interpreter::new();
it.vars.insert("dt".into(), Value::Float(0.016));
it.run(&file.fns[0].body);         // output in it.output
```

`SceneTree` presets `position/rotation/scale/visible/...` + script `prop`
defaults before `_process`, and writes back changed node props — so
`position.x += speed * dt` genuinely moves nodes.

## Verify

`cargo test -p godot_script_iris` — 9 tests incl. transpiler.
`cargo run -p editor -- check demo.iris` / `run demo.iris`.

## GDScript import (`gdtranspile`)

`transpile()` converts GDScript subsets to `IrisFile`; `emit_iris()` prints
them back for the Script editor. Supported: `extends`/`class_name`,
`var`/`@export var`/`const`, typed `func`, `if/elif/else`, `while`,
`for x in range()/array`, `match` literals → if-chain, ternary, `print`,
`emit_signal`, compound assignment. Rejected with reasons: `await`/coroutines,
lambdas, `match` bindings, `is`, `%`-formatting, `super`. Full matrix:
`docs/GDSCRIPT_COVERAGE.md`. GUI: AssetLib dock → “Transpile demo.gd”;
MCP: `gd_transpile`.
