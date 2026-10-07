//! `docs/vcs.md` §8's own acceptance for this slice: an `AddFunction`/
//! `ModifyBody` op built from a real cancho program's *actual*
//! `cancho-id` hashes, not invented strings — checked by hand against
//! `cancho ids`' own output before anything is built on top of this.

use std::collections::BTreeSet;

use cancho_id::identify;
use cancho_syntax::parse;
use cancho_vcs::{Operation, OperationKind};

const SOURCE: &str = "fn f(a: int, b: int) -> [] int { return a + b; }";

fn sig_and_stage(source: &str, name: &str) -> (String, String) {
    let ast = parse(source).expect("this fixture parses");
    let identities = identify(&ast);
    let f = identities.function(name).expect("the fixture declares this function");
    (f.sig.to_hex(), f.body.to_hex())
}

#[test]
fn an_add_function_op_hashes_the_real_cancho_id_hashes() {
    let (sig_id, stage_id) = sig_and_stage(SOURCE, "f");

    // Checked directly against `cancho ids /tmp/sig_check.cho`'s own CLI
    // output on this exact source: `c5e38ed9...77474b8` (sig) and
    // `85d54e87...c589ce` (body) -- `sig_and_stage` above calls the same
    // `cancho_id::identify` the CLI does, so this pins that the two paths
    // agree rather than assuming it.
    assert_eq!(sig_id, "c5e38ed91492e3078e8d7e85a77c385480a24650e3255b24fd28a602b77474b8");
    assert_eq!(stage_id, "85d54e8737a3e94e2356d09c1878ebe47deb28b3840d98bf675ff97602c589ce");

    let mut effects = BTreeSet::new();
    effects.insert("io_write".to_string());

    let op = Operation::new(
        OperationKind::AddFunction {
            sig_id: sig_id.clone(),
            stage_id: stage_id.clone(),
            effects,
            in_file: None,
        },
        1,
        [],
    );

    // The one property an `OpId` exists for: same payload, same identity,
    // computed twice independently rather than compared to itself.
    let again = Operation::new(
        OperationKind::AddFunction {
            sig_id,
            stage_id,
            effects: BTreeSet::from(["io_write".to_string()]),
            in_file: None,
        },
        1,
        [],
    );
    assert_eq!(op.op_id(), again.op_id());
}

#[test]
fn parent_order_does_not_change_the_op_id() {
    let (sig_id, stage_id) = sig_and_stage(SOURCE, "f");
    let kind = OperationKind::ModifyBody {
        sig_id,
        from_stage_id: stage_id.clone(),
        to_stage_id: stage_id,
    };

    let forwards = Operation::new(kind.clone(), 1, ["a".to_string(), "b".to_string()]);
    let backwards = Operation::new(kind, 1, ["b".to_string(), "a".to_string()]);

    assert_eq!(forwards.op_id(), backwards.op_id());
}

#[test]
fn a_different_edition_is_a_different_operation() {
    let (sig_id, stage_id) = sig_and_stage(SOURCE, "f");
    let kind =
        OperationKind::AddFunction { sig_id, stage_id, effects: BTreeSet::new(), in_file: None };

    let edition_1 = Operation::new(kind.clone(), 1, []);
    let edition_2 = Operation::new(kind, 2, []);

    assert_ne!(
        edition_1.op_id(),
        edition_2.op_id(),
        "docs/vcs.md §6: the edition is part of what gets hashed, so a vocabulary \
         change moving the effect row this op names is visible in its own identity"
    );
}

#[test]
fn two_functions_with_different_bodies_get_different_stage_ids() {
    let (sig_a, stage_a) = sig_and_stage("fn f(a: int, b: int) -> [] int { return a + b; }", "f");
    let (sig_b, stage_b) = sig_and_stage("fn f(a: int, b: int) -> [] int { return a - b; }", "f");

    // Same signature (same parameter and return types), different body.
    assert_eq!(sig_a, sig_b, "the signature does not depend on the body");
    assert_ne!(stage_a, stage_b, "the body hash is exactly what changed");

    let modify = Operation::new(
        OperationKind::ModifyBody { sig_id: sig_a, from_stage_id: stage_a, to_stage_id: stage_b },
        1,
        [],
    );
    // No assertion beyond "this constructs and hashes" -- the interesting
    // claim is the one above, that the two `StageId`s actually differ.
    let _ = modify.op_id();
}
