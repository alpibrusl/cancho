# ids_edges.sh -- the script of tests/commands/script_tests.cho for `cancho ids --edges` (`docs/cli-in-cancho.md` section 13, cancho#816): what each declaration refers to, as JSON. $1 is the
# compiler, $2 a scratch directory. The transcript is the JSON itself: a type in a field, in a signature and in a body, a function called in another module and in its own, one that
# calls itself, a static, a foreign function, and a name of the library (`--std`), which is referred to and not listed.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
cd "$work" || exit 1

cat > shapes.cho <<'CHO'
module shapes;

pub struct Pair {
    a: int,
    b: int,
}

pub enum Shape {
    Dot,
    Box(Pair),
}

// Twice the number.
pub fn twice(x: int) -> [] int {
    return x + x;
}

pub fn area(s: Shape) -> [] int {
    match s {
        Shape::Dot => {
            return 0;
        }
        Shape::Box(p) => {
            return twice(p.a) * p.b;
        }
    }
}
CHO
cat > main.cho <<'CHO'
import shapes;
import std.math;

static squares: [int] {
    let table = alloc_slice[static](4, 0);
    table[1] = shapes.twice(1);
    return table;
}

extern fn labs[&f](ffi: &f Ffi("libc"), n: int) -> [ffi("libc")] int;

fn down(n: int) -> [] int {
    if n <= 0 {
        return 0;
    }
    return down(n - 1);
}

fn helper(p: shapes.Pair) -> [] int {
    return shapes.area(shapes.Shape::Box(p)) + shapes.twice(1) + math.max(1, 2);
}

fn main() -> [] int {
    let p = shapes.Pair { a: 2, b: 3 };
    if helper(p) + down(3) == 16 {
        return 0;
    }
    return 1;
}
CHO
echo "== ids --edges"
"$cancho" ids shapes.cho main.cho --std --edges
echo "status $?"
echo "== the flag before the files, and the lines of ids without it are as they were"
"$cancho" ids --edges shapes.cho main.cho --std > first.json
echo "status $?"
"$cancho" ids shapes.cho main.cho --std --edges | cmp -s - first.json && echo "ok the same answer"
"$cancho" ids shapes.cho main.cho --std | sed 's/ * [0-9a-f]\{64\}$//' | head -3
echo "== a refused program: no JSON"
printf 'fn main() -> [] int {\n    return missing(1);\n}\n' > bad.cho
"$cancho" ids bad.cho --edges > bad.json 2> bad.err
echo "status $? bytes $(wc -c < bad.json | tr -d ' ')"
echo "== --edges is of ids only"
"$cancho" check shapes.cho main.cho --std --edges > /dev/null 2>&1
echo "status $?"
