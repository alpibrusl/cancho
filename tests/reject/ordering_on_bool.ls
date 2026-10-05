//~ ERROR `bool` has no ordering (`int`, `float` and `f32` do)
//~ RULE operator-type-mismatch

fn main() -> [] int {
    if true < false {
        return 0;
    }
    return 1;
}
