module selfhost.types;

// types.ls -- the types of the lex-sys checker's bodies, written in lex-sys.
//
// Stage 3d of the self-hosting spike (`docs/self-hosting.md` section 6, epic #295): a port of the
// part of `lex-sys-types` and `lower/mod.rs` that gives types, regions and the coercions between
// them their meaning: `Unifier::unify`, `unify_regions`, `expect_type`, `outlives`.
//
// A type is an integer. 1 to 6 are the scalars and the unit (`int`, `byte`, `float`, `f32`,
// `bool`, `()`), which are not in the table, so a scalar costs nothing; 8 and up index a table in
// the state of four slots a type: its kind, and what the kind keeps.
//
//     REF    a unique (1) or shared (0), b the region, c the referent
//     SLICE  a the element
//
// A region is an integer too: 0 and up a region parameter of the function being checked, -1 the
// `static` region, and 1000000 and up a variable, which is bound to a region or free
// (`unbound`). The table and the variables are the function's own: `check_function` resets both.
// Only what the first slice of references needs is here: references to scalars and to slices of
// scalars, in regions that are parameters, `static`, or variables a call makes.

import selfhost.ast;
import selfhost.rules;
import selfhost.pass1;

pub fn first_table() -> [] int {
    return 8;
}

fn ref_kind() -> [] int {
    return 7;
}

fn slice_kind() -> [] int {
    return 8;
}

fn named_kind() -> [] int {
    return 9;
}

fn unbound() -> [] int {
    return 0 - 100;
}

pub fn static_region() -> [] int {
    return 0 - 1;
}

fn var_base() -> [] int {
    return 1000000;
}

// ----------------------------------------------------------------- reset ---

pub fn reset[&s](st: &!s [int]) -> [] int {
    st[24] = first_table();
    st[26] = 0;
    return 0;
}

// ---------------------------------------------------------------- types ---

fn make[&s](st: &!s [int], kind: int, a: int, b: int, c: int) -> [] int {
    let id = st[24];
    let at = st[25] + 4 * (id - first_table());
    st[at] = kind;
    st[at + 1] = a;
    st[at + 2] = b;
    st[at + 3] = c;
    st[24] = id + 1;
    return id;
}

pub fn make_ref[&s](st: &!s [int], unique: bool, area: int, inner: int) -> [] int {
    return make(st, ref_kind(), ast.flag(unique), area, inner);
}

// A struct the file declares, by the node of its declaration.
pub fn make_named[&s](st: &!s [int], item: int) -> [] int {
    return make(st, named_kind(), item, 0, 0);
}

pub fn make_slice[&s](st: &!s [int], element: int) -> [] int {
    return make(st, slice_kind(), element, 0, 0);
}

fn slot[&s](st: &!s [int], ty: int, k: int) -> [] int {
    return st[st[25] + 4 * (ty - first_table()) + k];
}

pub fn is_ref[&s](st: &!s [int], ty: int) -> [] bool {
    return ty >= first_table() && slot(st, ty, 0) == ref_kind();
}

pub fn is_slice[&s](st: &!s [int], ty: int) -> [] bool {
    return ty >= first_table() && slot(st, ty, 0) == slice_kind();
}

pub fn is_named[&s](st: &!s [int], ty: int) -> [] bool {
    return ty >= first_table() && slot(st, ty, 0) == named_kind();
}

pub fn named_item[&s](st: &!s [int], ty: int) -> [] int {
    return slot(st, ty, 1);
}

pub fn ref_unique[&s](st: &!s [int], ty: int) -> [] bool {
    return slot(st, ty, 1) != 0;
}

// The region of a reference, followed through the variables that are bound.
pub fn ref_region[&s](st: &!s [int], ty: int) -> [] int {
    return resolve_region(st, slot(st, ty, 2));
}

pub fn ref_inner[&s](st: &!s [int], ty: int) -> [] int {
    return slot(st, ty, 3);
}

pub fn slice_element[&s](st: &!s [int], ty: int) -> [] int {
    return slot(st, ty, 1);
}

// The element type of a reference to a slice (`element_of`), or 0 if the type is not one.
pub fn element_of[&s](st: &!s [int], ty: int) -> [] int {
    if is_ref(st, ty) && is_slice(st, ref_inner(st, ty)) {
        return slice_element(st, ref_inner(st, ty));
    }
    return 0;
}

// ------------------------------------------------------------- regions ---

pub fn is_var(area: int) -> [] bool {
    return area >= var_base();
}

// A new region variable, free.
pub fn fresh_region[&s](st: &!s [int]) -> [] int {
    let v = st[26];
    st[st[27] + v] = unbound();
    st[26] = v + 1;
    return var_base() + v;
}

// Follow bound variables until the head is not a bound variable.
pub fn resolve_region[&s](st: &!s [int], area: int) -> [] int {
    var current = area;
    var go = true;
    while go && is_var(current) {
        let bound = st[st[27] + (current - var_base())];
        if bound == unbound() {
            go = false;
        } else {
            current = bound;
        }
    }
    return current;
}

// Make two regions one, or say they are different (`unify_regions`): a free variable takes
// the other region, and two regions that are not variables must already be equal.
pub fn unify_regions[&s](st: &!s [int], expected: int, found: int) -> [] bool {
    let a = resolve_region(st, expected);
    let b = resolve_region(st, found);
    if a == b {
        return true;
    }
    if is_var(a) {
        st[st[27] + (a - var_base())] = b;
        return true;
    }
    if is_var(b) {
        st[st[27] + (b - var_base())] = a;
        return true;
    }
    return false;
}

// Does the region parameter `from` reach `target` along the function's `where` pairs, in at
// most `fuel` steps? `where a <= b` is the pair that leads from `a` to `b`.
fn reaches[&s, &x](st: &!s [int], text: &x [byte], from: int, target: int, fuel: int) -> [] bool {
    if from == target {
        return true;
    }
    if fuel == 0 {
        return false;
    }
    var k = 0;
    while pass1.nth_outlive(st, st[28], k) >= 0 {
        let inner = pass1.nth_outlive(st, st[28], k);
        if pass1.param_index(st, text, st[28], true, inner) == from {
            let next = pass1.param_index(st, text, st[28], true, inner + 2);
            if reaches(st, text, next, target, fuel - 1) {
                return true;
            }
        }
        k = k + 1;
    }
    return false;
}

// Does region `outer` outlive region `inner` (`outlives`)? A region parameter outlives another
// as the declaration's `where` clauses say, reflexively and transitively; `static` outlives all;
// a variable that is still free outlives nothing but itself.
pub fn outlives[&s, &x](st: &!s [int], text: &x [byte], outer: int, inner: int) -> [] bool {
    if outer == inner {
        return true;
    }
    if outer == static_region() {
        return true;
    }
    if inner == static_region() {
        return false;
    }
    if outer >= 0 && inner >= 0 && !is_var(outer) && !is_var(inner) {
        return reaches(st, text, inner, outer, pass1.count_params(st, st[28], true) + 1);
    }
    return false;
}

// ------------------------------------------------------ equality, coercion ---

// What unifying two types came to: 0 they are the same, 1 they mismatch (a `type-mismatch`),
// 2 their regions are different (a `region-mismatch`). A free region variable is bound to the
// region it meets.
pub fn unify[&s](st: &!s [int], expected: int, found: int) -> [] int {
    if expected < first_table() || found < first_table() {
        if expected == found {
            return 0;
        }
        return 1;
    }
    if is_ref(st, expected) && is_ref(st, found) {
        if ref_unique(st, expected) != ref_unique(st, found) {
            return 1;
        }
        if !unify_regions(st, slot(st, expected, 2), slot(st, found, 2)) {
            return 2;
        }
        return unify(st, ref_inner(st, expected), ref_inner(st, found));
    }
    if is_slice(st, expected) && is_slice(st, found) {
        return unify(st, slice_element(st, expected), slice_element(st, found));
    }
    if is_named(st, expected) && is_named(st, found) && named_item(st, expected) == named_item(st, found) {
        return 0;
    }
    return 1;
}

fn report[&s](st: &!s [int], status: int, from: int, to: int) -> [] int {
    if status == 1 {
        ast.fail(st, rules.r_type_mismatch(), from, to);
    }
    if status == 2 {
        ast.fail(st, rules.r_region_mismatch(), from, to);
    }
    return status;
}

// `expect_type`: `found` is usable where `expected` is wanted. Equality, with two coercions on
// top and no others: a reference whose region outlives the expected one is accepted, and a
// unique reference where a shared one is expected, never the reverse. The referent is invariant.
pub fn expect_type[&s, &x](st: &!s [int], text: &x [byte], expected: int, found: int, from: int, to: int) -> [] int {
    if is_ref(st, expected) && is_ref(st, found) {
        let want_unique = ref_unique(st, expected);
        let got_unique = ref_unique(st, found);
        if want_unique == got_unique || !want_unique && got_unique {
            let wanted = ref_region(st, expected);
            let given = ref_region(st, found);
            if is_var(wanted) || is_var(given) {
                unify_regions(st, wanted, given);
            } else if !outlives(st, text, given, wanted) {
                ast.fail(st, rules.r_reference_escapes_region(), from, to);
                return 0;
            }
            return report(st, unify(st, ref_inner(st, expected), ref_inner(st, found)), from, to);
        }
    }
    return report(st, unify(st, expected, found), from, to);
}
