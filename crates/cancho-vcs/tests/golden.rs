//! `docs/vcs.md` §8's own acceptance for this slice: an `AddFunction`/
//! `ModifyBody` op built from a real cancho program's *actual*
//! `cancho ids` hashes, not invented strings — checked by hand against
//! `cancho ids`' own output before anything is built on top of this.

use std::collections::BTreeSet;

use cancho_vcs::{Operation, OperationKind};

// The identities `cancho ids` wrote for `fn f(a: int, b: int) -> [] int { return a + b; }` when the Rust `cancho-id`
// computed them (the compiler in cancho writes the same ones, `tests/commands/front_end_names.sh`): literals now, the
// crate that computed them being deleted (`docs/removing-rust.md`, Phase 5).
const SIG: &str = "c5e38ed91492e3078e8d7e85a77c385480a24650e3255b24fd28a602b77474b8";
const BODY: &str = "85d54e8737a3e94e2356d09c1878ebe47deb28b3840d98bf675ff97602c589ce";
// The body of the same function with `a - b`: a different hash under the same signature.
const BODY_MINUS: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[test]
fn an_add_function_op_hashes_the_real_cancho_id_hashes() {
    let (sig_id, stage_id) = (SIG.to_owned(), BODY.to_owned());

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
    let (sig_id, stage_id) = (SIG.to_owned(), BODY.to_owned());
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
    let (sig_id, stage_id) = (SIG.to_owned(), BODY.to_owned());
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
    assert_ne!(BODY, BODY_MINUS);
    let modify = Operation::new(
        OperationKind::ModifyBody {
            sig_id: SIG.to_owned(),
            from_stage_id: BODY.to_owned(),
            to_stage_id: BODY_MINUS.to_owned(),
        },
        1,
        [],
    );
    let same = Operation::new(
        OperationKind::ModifyBody {
            sig_id: SIG.to_owned(),
            from_stage_id: BODY.to_owned(),
            to_stage_id: BODY.to_owned(),
        },
        1,
        [],
    );
    assert_ne!(modify.op_id(), same.op_id());
}
