# refusals.sh -- the script of tests/commands/script_tests.cho for `conformance/refusals.rs` (deleted): refusals as data, `check --output json` (docs/agent-errors.md). $1 is the compiler, $2 a scratch
# directory; run from the root of the repository. The JSON is read by python3 (`json.load`, which refuses a raw control character in a string), the gate's own dependency, as RFC 8259 asks.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
root=$(pwd)
work=$2

# The `--target T` of a fixture that names one on a `//~ TARGET T` line (a refusal that is a property of a target), else nothing.
target_of() { sed -n 's/^[ \t]*\/\/~ TARGET \(.*\)$/\1/p' "$1" | head -1 | tr -d ' \t'; }
check() { # FILE JSONOUT PROSEOUT: check a reject fixture in both forms
    target=$(target_of "$1")
    if [ -n "$target" ]; then
        "$cancho" check "$1" --target "$target" > /dev/null 2> "$3"
        "$cancho" check "$1" --output json --target "$target" > "$2" 2> /dev/null
    else
        "$cancho" check "$1" > /dev/null 2> "$3"
        "$cancho" check "$1" --output json > "$2" 2> /dev/null
    fi
}

# docs/agent-errors.md section 4: every independent refusal, not the first. Three ill-typed functions in three independent bodies.
echo "== every independent refusal is reported"
cat > "$work/three.cho" << 'EOF'
fn a() -> [] int { return true; }
fn b() -> [] int { return oops; }
fn c() -> [] bool { return 1; }
fn main() -> [] int { return 0; }
EOF
"$cancho" check "$work/three.cho" > /dev/null 2> "$work/prose.txt"
echo "prose refusals: $(grep -c 'error:' "$work/prose.txt")"
"$cancho" check "$work/three.cho" --output json > "$work/three.json" 2> /dev/null
echo "exit $?"
echo "json refusals: $(grep -c '"rule":' "$work/three.json")"
# In source order, which is the order checking already runs in.
sed -n 's/.*"line": \([0-9]*\).*/\1/p' "$work/three.json" | tr '\n' ' '
echo

# Section 6: the JSON adds fields beside the prose and rewrites none of it; the sentence a person reads is the sentence the object carries.
echo "== the prose is unchanged by the json"
n=0
bad=0
skipped=0
for path in tests/reject/*.cho; do
    # The WebAssembly target is the Rust compiler's until #486: the compiler in cancho answers nothing for a fixture that names one.
    if [ -n "$(target_of "$path")" ]; then skipped=$((skipped + 1)); continue; fi
    n=$((n + 1))
    check "$path" "$work/body.json" "$work/prose.txt"
    message=$(sed -n 's/^[ \t]*"message": "\(.*\)",$/\1/p' "$work/body.json" | head -1 | sed -e 's/\\"/"/g' -e 's/\\\\/\\/g')
    if [ -z "$message" ]; then echo "FAILS $path reported no message"; bad=$((bad + 1)); continue; fi
    grep -qF -- "$message" "$work/prose.txt" || { echo "FAILS $path says one thing as prose and another as data"; bad=$((bad + 1)); }
done
echo "fixtures: $n, skipped for a target: $skipped"
[ $n -gt 100 ] && echo "ok more than a hundred fixtures"
[ $bad = 0 ] && echo "ok prose and data agree"

# Section 5, read by a JSON parser rather than by eye: every reject fixture's answer is valid JSON, and every refusal has a position except section 1.1's three about the program as a whole.
echo "== every refusal is valid JSON with a position"
bad=0
for path in tests/reject/*.cho; do
    if [ -n "$(target_of "$path")" ]; then continue; fi
    name=$(basename "$path" .cho)
    check "$path" "$work/body.json" "$work/prose.txt"
    python3 -I -c 'import json, sys; json.load(sys.stdin)' < "$work/body.json" 2> "$work/json.err" || { echo "FAILS $name answered invalid JSON: $(tail -1 "$work/json.err")"; bad=$((bad + 1)); }
    unplaced=$(grep -c '"position": null' "$work/body.json")
    case $name in
        main_must_return_int|main_takes_the_world|no_main) want=1 ;;
        *) want=0 ;;
    esac
    [ "$unplaced" = "$want" ] || { echo "FAILS $name has $unplaced refusals with no position, wanted $want"; bad=$((bad + 1)); }
done
[ $bad = 0 ] && echo "ok every answer is valid JSON and placed"

# A program with nothing wrong answers an empty list and exits 0.
echo "== a clean program answers an empty list"
"$cancho" check examples/hello.cho --output json > "$work/clean.json"
echo "exit $?"
cat "$work/clean.json"
exit 0
