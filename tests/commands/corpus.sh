# corpus.sh -- the script of tests/commands/script_tests.cho that was crates/cancho/tests/conformance/corpus.rs (docs/gate.md G4, wave 4): the corpus (every accept and reject fixture, every example) and the command line
# around it. The three walkers (`accepted_programs_build_and_run`, `refused_programs_are_refused_with_the_stated_reason`, `every_example_runs_and_prints_what_it_says`) only built tests/run.cho and ran it; stage `walk` of
# scripts/gate.sh builds and runs the same program over the same three kinds, so they are that stage, and what is checked here is that the stage still names the three kinds. $1 is the compiler, $2 a scratch directory;
# run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_lib.sh
root=$(pwd)

begin the_three_walkers_are_stage_walk_of_the_gate
grep -q 'for kind in reject accept examples; do' scripts/gate.sh || fail "stage walk of scripts/gate.sh no longer walks reject, accept and examples"
grep -q 'build tests/run.cho tests/support/harness.cho --std' scripts/gate.sh || fail "stage walk of scripts/gate.sh no longer builds tests/run.cho"
end

# `docs/agent-errors.md` section 3.2: every rule in the catalogue (tests/rules.golden) is a rule some fixture of tests/reject reaches, and no fixture names a rule the catalogue has not. `not-public` and `internal` are the
# two the single-file harness cannot reach by construction (`pub` is about reaching another module, and `internal` names the compiler's own failures); `ingest-*` are the three a `.cho` fixture cannot reach at all, which
# name the refusals of the JSON ingest path (docs/structured-ingest.md section 3).
begin every_rule_has_a_fixture
fresh rules
covered_elsewhere="not-public internal ingest-arity ingest-json ingest-node"
sed -n 's/^\/\/~ RULE //p' tests/reject/*.cho | sed 's/ *$//' | sort -u > "$d/declared.txt"
sort -u tests/rules.golden > "$d/catalogue.txt"
missing=
while read -r tag; do
    case " $covered_elsewhere " in *" $tag "*) continue ;; esac
    grep -qx "$tag" "$d/declared.txt" || missing="$missing $tag"
done < "$d/catalogue.txt"
same "$missing" "" "rules that exist and no fixture reaches"
unknown=
while read -r tag; do
    grep -qx "$tag" "$d/catalogue.txt" || unknown="$unknown $tag"
done < "$d/declared.txt"
same "$unknown" "" "tags a fixture declares and the catalogue has not"
# The count is written down: a catalogue or a corpus that shrank is seen.
echo "rules: $(wc -l < "$d/catalogue.txt" | tr -d ' ') in the catalogue, $(wc -l < "$d/declared.txt" | tr -d ' ') declared by a fixture"
end

begin run_builds_and_executes_in_one_step
r run examples/hello.cho
same "$out" "Hello, world!" "what it prints"
same "$code" 0 "its exit status"
end

# `examples/wordfreq` is the capstone: more than one file, arguments, file IO, the heap, matching through references, and slices. The single-file walker cannot reach it (a directory has no `.cho` extension).
begin the_multi_file_example_builds_and_runs
fresh wordfreq
r build examples/wordfreq/main.cho examples/wordfreq/text.cho examples/wordfreq/counts.cho --std -o "$d/wordfreq"; succeeds build
# With no arguments it counts its own sample. The list is built by prepending, so the order is reverse first-seen.
"$d/wordfreq" > "$d/out.txt" 2> /dev/null
same "$?" 0 "eight distinct words"
same "$(cat "$d/out.txt")" "dog 1
lazy 1
over 1
jumps 1
fox 2
brown 1
quick 1
the 3" "the words of its own sample"
# Given a path it counts that file instead.
printf 'alpha beta alpha\ngamma beta alpha\n' > "$d/doc.txt"
same "$("$d/wordfreq" "$d/doc.txt")" "gamma 1
beta 2
alpha 3" "the words of a file"
end

# `examples/modular/`: two modules and a root, with a qualifier, an `as`, a private helper and a module importing another.
begin the_modular_example_builds_and_runs
fresh modular
r build examples/modular/main.cho examples/modular/counts.cho examples/modular/text.cho -o "$d/modular"; succeeds build
"$d/modular" > "$d/out.txt" 2> /dev/null
same "$?" 0 "exit status"
same "$(cat "$d/out.txt")" "seen 3, total 60
60" "what it prints"
end

# `examples/buffer/` is the library docs/boxed-slices.md section 4 describes: growing is allocate-copy-end, written down rather than built in.
begin the_growable_buffer_example_builds_and_runs
fresh buffer
r build examples/buffer/main.cho examples/buffer/buffer.cho -o "$d/buffer"; succeeds build
"$d/buffer" > "$d/out.txt" 2> /dev/null
same "$?" 0 "thirty-one bytes built from a one-byte buffer"
same "$(cat "$d/out.txt")" "counting: 1 4 9 16 25 36 49 64" "what it prints"
end

# `examples/slab/` is section 9's `Gen` hatch (docs/sharing.md section 3). The line that matters is the last one: a handle whose slot was removed comes back `Missing`, a value and not a dangling pointer.
begin the_slab_example_builds_and_runs
fresh slab
r build examples/slab/main.cho examples/slab/slab.cho --std -o "$d/slab"; succeeds build
"$d/slab" > "$d/out.txt" 2> /dev/null
same "$?" 0 "one slot live at the end, so drop_slab said 1"
same "$(cat "$d/out.txt")" "live handle:  7
after remove: missing
new handle:   9
old handle:   missing" "what it prints"
end

begin emitting_a_bare_object_file_works
fresh emit
r build examples/hello.cho --emit obj -o "$d/hello.o"; succeeds "build --emit obj"
[ -s "$d/hello.o" ] || fail "no object file, or an empty one"
end

begin a_wrong_command_line_is_a_usage_error_not_a_refusal
r frobnicate
same "$code" 2 "exit status"
has "$err" "usage:" "what it says"
end
