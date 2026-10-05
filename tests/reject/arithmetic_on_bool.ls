//~ ERROR `bool` has no arithmetic (`int`, `float` and `f32` do)
//~ RULE operator-type-mismatch

fn main() -> [] int {
    return true + false;
}
