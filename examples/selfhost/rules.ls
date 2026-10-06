module selfhost.rules;

// rules.ls -- the rules a refusal can carry, as the integers the parser and the checker store, and
// the tags the Rust `Rule::tag` spells them with. 0 to 2 are the lexer's (`lexcore.rule_name`); 99 is not a rule but the answer `SKIP`, for a function whose body uses something the port does not check yet.

import selfhost.lexcore as lc;

pub fn r_type_mismatch() -> [] int {
    return 3;
}

pub fn r_literal_out_of_range() -> [] int {
    return 4;
}

pub fn r_unknown_edition() -> [] int {
    return 5;
}

pub fn r_program_shape() -> [] int {
    return 6;
}

pub fn r_pattern_shape() -> [] int {
    return 7;
}

pub fn r_foreign_declaration() -> [] int {
    return 8;
}

pub fn r_region_mismatch() -> [] int {
    return 9;
}

pub fn r_mode_bound_violated() -> [] int {
    return 10;
}

pub fn r_unknown_name() -> [] int {
    return 11;
}

pub fn r_duplicate_declaration() -> [] int {
    return 12;
}

pub fn r_builtin_redeclared() -> [] int {
    return 13;
}

pub fn r_module_not_imported() -> [] int {
    return 14;
}

pub fn r_type_args_not_taken() -> [] int {
    return 15;
}

pub fn r_arity_mismatch() -> [] int {
    return 16;
}

pub fn r_region_not_in_scope() -> [] int {
    return 17;
}

pub fn r_unsized_type() -> [] int {
    return 18;
}

pub fn r_static_item() -> [] int {
    return 19;
}

pub fn r_not_public() -> [] int {
    return 20;
}

pub fn r_infinite_type() -> [] int {
    return 21;
}

pub fn r_enum_has_no_variants() -> [] int {
    return 22;
}

pub fn r_foreign_scope() -> [] int {
    return 23;
}

pub fn r_foreign_boundary_type() -> [] int {
    return 24;
}

pub fn r_effect_not_declared() -> [] int {
    return 25;
}

pub fn r_effect_declared_not_performed() -> [] int {
    return 26;
}

pub fn r_operator_type_mismatch() -> [] int {
    return 27;
}

pub fn r_constant_traps() -> [] int {
    return 28;
}

pub fn r_assign_to_immutable() -> [] int {
    return 29;
}

pub fn r_unreachable_statement() -> [] int {
    return 30;
}

pub fn r_missing_return() -> [] int {
    return 31;
}

pub fn r_not_a_function() -> [] int {
    return 32;
}

pub fn r_skip() -> [] int {
    return 99;
}

pub fn r_literal_form() -> [] int {
    return 0;
}

pub fn rule_tag(r: int) -> [] &static [byte] {
    if r < 3 {
        return lc.rule_name(r);
    }
    if r == 3 {
        return "type-mismatch";
    }
    if r == 4 {
        return "literal-out-of-range";
    }
    if r == 5 {
        return "unknown-edition";
    }
    if r == 6 {
        return "program-shape";
    }
    if r == 7 {
        return "pattern-shape";
    }
    if r == 8 {
        return "foreign-declaration";
    }
    if r == 9 {
        return "region-mismatch";
    }
    if r == 10 {
        return "mode-bound-violated";
    }
    if r == 12 {
        return "duplicate-declaration";
    }
    if r == 13 {
        return "builtin-redeclared";
    }
    if r == 14 {
        return "module-not-imported";
    }
    if r == 15 {
        return "type-args-not-taken";
    }
    if r == 16 {
        return "arity-mismatch";
    }
    if r == 17 {
        return "region-not-in-scope";
    }
    if r == 18 {
        return "unsized-type";
    }
    if r == 19 {
        return "static-item";
    }
    if r == 20 {
        return "not-public";
    }
    if r == 21 {
        return "infinite-type";
    }
    if r == 22 {
        return "enum-has-no-variants";
    }
    if r == 23 {
        return "foreign-scope";
    }
    if r == 24 {
        return "foreign-boundary-type";
    }
    if r == 25 {
        return "effect-not-declared";
    }
    if r == 26 {
        return "effect-declared-not-performed";
    }
    if r == 27 {
        return "operator-type-mismatch";
    }
    if r == 28 {
        return "constant-traps";
    }
    if r == 29 {
        return "assign-to-immutable";
    }
    if r == 30 {
        return "unreachable-statement";
    }
    if r == 31 {
        return "missing-return";
    }
    if r == 32 {
        return "not-a-function";
    }
    if r == 99 {
        return "SKIP";
    }
    return "unknown-name";
}
