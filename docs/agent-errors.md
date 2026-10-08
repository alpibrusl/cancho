# Refusals a machine can read

> **Status: measured, then designed, then built (#59).**
>
> lex-lang's `docs/AGENT_GUIDELINES.md` is a contract with a *reader
> that is a program*: `lex check --output json` answers a stable
> `rule_tag`, and §4's rule is **repair, don't regenerate**. Before
> #59, cancho had none of that: 196 must-reject fixtures, each of
> which answered a sentence written for a person.
>
> §1 is the measurement that motivated the slice, kept because the
> method outlives the state it measured; it describes cancho as it
> was when #59 opened, not as it is. §2 is the part of lex-lang's
> answer that transferred and the part that could not, and the line
> between them is not where the guidelines page puts it. §5 is the
> shape on the wire as it ships today; the catalogue is
> `crates/cancho-syntax/src/rules.rs`, and the plural is real: `check`
> answers every independent refusal, not the first.

---

## 1. What a refusal gave a machine before #59

`check` run over every fixture in `tests/reject/`, at the revision
where #59 opened (the corpus has grown since; this is the reading the
slice was measured against, not the current one):

| | |
|---|---|
| refusals carrying `file:line:col` | **193 / 196** |
| distinct message *shapes*, with names and numbers normalised out | **125** |
| shapes that occur exactly **once** | **101** |
| errors reported per invocation | **1** |
| machine-readable form | **none** |

Three things follow, and they are independent problems.

### 1.1 There is no vocabulary, only prose

A program that wants to know *which rule it broke* has 125 sentence
patterns to match, and 101 of them it will meet once. lex-lang answers
that question with one field: **16** `rule_tag`s cover every type error
it can raise.

The three refusals with no location are program-level — `no main
function`, and the two about `main`'s shape. They have nowhere to point
because the thing that is wrong is the program rather than a span in it,
which is a real answer and still leaves a consumer with a message shape
it has to special-case.

### 1.2 One error per invocation

PLACEHOLDER_REST_OF_FILE
