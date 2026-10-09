//! The codec: the AST as JSON, and JSON back into an AST
//! (`docs/structured-ingest.md`).
//!
//! The AST is canonical by construction (`canonical-ast.md` §3), so text
//! is one *lossy rendering* of a tree that already has exactly one
//! reasonable shape. This module is the other rendering — data rather
//! than characters — and the same two contracts hold over it that
//! [`crate::print`] holds over text, enforced by the same walker in the
//! conformance suite:
//!
//! 1. **Identity-preserving.** `ast_to_json`, then `json_to_ast`, then
//!    [`crate::print`] gives text whose declarations hash identically to
//!    the source's.
//! 2. **Idempotent.** Ingesting the JSON of the ingested text gives the
//!    same text.
//!
//! The JSON format itself — the value tree, its reader and its writer —
//! lives in [`crate::json`], one concern per file (`CONTRIBUTING.md`).
//! Names are text and never interner indices (`canonical-ast.md` §4.1):
//! an index is a property of one parse, and a cross-process form cannot
//! depend on the order a lexer met names in. Float literals are their
//! **bits** for the reason `canonical-ast.md` §3 gives: `0.0` and
//! `-0.0` compare equal and behave differently, so they are two
//! literals and two JSONs.
//!
//! A refusal here carries a rule like any other (`agent-errors.md` §3):
//! `ingest-json` for data that is not JSON, `ingest-node` for a word the
//! vocabulary does not have, `ingest-arity` for a known word with the
//! wrong shape. The span points into the JSON text, which is the file
//! the reader handed us and the only file there is.

use crate::ast::{
    Ast, BinOp, Block, EffectLabel, Expr, ExprId, Item, Mode, Param, Pattern, StaticDecl, Stmt,
    StmtId, Symbol, TypeExpr, TypeId, UnOp,
};
use crate::json::Json;
use crate::lexer::{is_identifier, is_keyword};
use crate::rules::Rule;
use crate::span::{Diagnostic, Span};

// ---- AST → JSON ------------------------------------------------------------

/// Render one parsed unit as the JSON form (`docs/structured-ingest.md`
/// §2): the file's module header and imports, then its items, with every
/// name as text and every id replaced by the value it names. Spans are
/// absent by construction — the AST's own side tables never reach a
/// cross-process form.
pub fn ast_to_json(ast: &Ast) -> Json {
    // One `parse` is one file, which declares at most one module, so the
    // unit's header lives in the module its items belong to — module 0 for
    // a file that declares nothing (`docs/modules.md` §3), the declared
    // module's own index otherwise. Reading module 0 unconditionally put
    // a declared module's imports and items under a header that says
    // `module m;` while encoding none of them.
    let of_first = ast.items.first().map(|_| ast.module_of(crate::ast::ItemId(0)));
    let index = match of_first {
        Some(module) if !ast.module(module).is_root() => module,
        _ => 0,
    };
    // Imports belong with the header even when the file has no items:
    // `import std.io;` alone is a unit a store can hold.
    let imports_index = if ast.modules.len() > 1 && ast.module(0).imports.is_empty() {
        let declared = (0..ast.modules.len() as u32)
            .find(|i| !ast.module(*i).is_root() && !ast.module(*i).imports.is_empty());
        declared.unwrap_or(index)
    } else {
        0
    };
    let root = if ast.items.is_empty() { ast.module(imports_index) } else { ast.module(index) };
    let mut entries = Vec::new();
    if !root.is_root() {
        entries.push(("module".to_owned(), str(&path_of(ast, &root.path))));
    }
    let imports: Vec<Json> = root
        .imports
        .iter()
        .map(|import| {
            let mut fields = vec![("path".to_owned(), str(&path_of(ast, &import.path)))];
            if ast.name_of(import.alias) != ast.name_of(*import.path.last().expect("nonempty")) {
                fields.push(("as".to_owned(), str(ast.name_of(import.alias))));
            }
            object_owned(fields)
        })
        .collect();
    if !imports.is_empty() {
        entries.push(("imports".to_owned(), Json::Array(imports, Span::new(0, 0))));
    }
    let items: Vec<Json> = (0..ast.items.len()).map(|i| item_to_json(ast, i)).collect();
    entries.push(("items".to_owned(), Json::Array(items, Span::new(0, 0))));
    Json::Object(entries, Span::new(0, 0))
}

fn object(fields: Vec<(&str, Json)>) -> Json {
    Json::Object(fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(), Span::new(0, 0))
}

fn object_owned(fields: Vec<(String, Json)>) -> Json {
    Json::Object(fields, Span::new(0, 0))
}

fn node(kind: &str, fields: Vec<(&str, Json)>) -> Json {
    let mut all = vec![("kind", str(kind))];
    all.extend(fields);
    object(all)
}

fn str(text: &str) -> Json {
    Json::Str(text.to_owned(), Span::new(0, 0))
}

fn integer(value: i64) -> Json {
    Json::Int(value, Span::new(0, 0))
}

fn path_of(ast: &Ast, path: &[Symbol]) -> String {
    path.iter().map(|s| ast.name_of(*s)).collect::<Vec<_>>().join(".")
}

fn ty_to_json(ast: &Ast, id: TypeId) -> Json {
    match ast.ty(id) {
        TypeExpr::Name { name, qualifier, args } => {
            let mut fields = Vec::new();
            if let Some(q) = qualifier {
                fields.push(("qualifier", str(ast.name_of(*q))));
            }
            fields.push(("name", str(ast.name_of(*name))));
            if !args.is_empty() {
                fields.push((
                    "args",
                    Json::Array(
                        args.iter().map(|a| ty_to_json(ast, *a)).collect(),
                        Span::new(0, 0),
                    ),
                ));
            }
            object(fields)
        }
        TypeExpr::Ref { unique, region, inner } => node(
            "ref",
            vec![
                ("unique", Json::Bool(*unique, Span::new(0, 0))),
                ("region", str(ast.name_of(*region))),
                ("inner", ty_to_json(ast, *inner)),
            ],
        ),
        TypeExpr::Slice(inner) => node("slice", vec![("inner", ty_to_json(ast, *inner))]),
        TypeExpr::Tuple(parts) => node(
            "tuple",
            vec![(
                "parts",
                Json::Array(parts.iter().map(|p| ty_to_json(ast, *p)).collect(), Span::new(0, 0)),
            )],
        ),
        TypeExpr::Lit(text) => node("lit", vec![("value", str(text))]),
        TypeExpr::Fn { params, effects, ret } => node(
            "fn",
            vec![
                (
                    "params",
                    Json::Array(
                        params.iter().map(|p| ty_to_json(ast, *p)).collect(),
                        Span::new(0, 0),
                    ),
                ),
                (
                    "effects",
                    Json::Array(
                        effects.iter().map(|e| effect_to_json(ast, e)).collect(),
                        Span::new(0, 0),
                    ),
                ),
                ("ret", ty_to_json(ast, *ret)),
            ],
        ),
    }
}

fn effect_to_json(ast: &Ast, label: &EffectLabel) -> Json {
    let mut fields = vec![("name", str(ast.name_of(label.name)))];
    if let Some(argument) = &label.argument {
        fields.push(("argument", str(argument)));
    }
    object(fields)
}

fn param_to_json(ast: &Ast, param: &Param) -> Json {
    object(vec![("name", str(ast.name_of(param.name))), ("ty", ty_to_json(ast, param.ty))])
}

fn block_to_json(ast: &Ast, block: &Block) -> Json {
    Json::Array(block.stmts.iter().map(|s| stmt_to_json(ast, *s)).collect(), Span::new(0, 0))
}

fn stmt_to_json(ast: &Ast, id: StmtId) -> Json {
    match ast.stmt(id) {
        Stmt::Let { name, mutable, ty, value } => node(
            "let",
            vec![
                ("mutable", Json::Bool(*mutable, Span::new(0, 0))),
                ("name", str(ast.name_of(*name))),
                ("ty", ty.map(|t| ty_to_json(ast, t)).unwrap_or(Json::Null(Span::new(0, 0)))),
                ("value", expr_to_json(ast, *value)),
            ],
        ),
        Stmt::Assign { place, value } => node(
            "assign",
            vec![("place", expr_to_json(ast, *place)), ("value", expr_to_json(ast, *value))],
        ),
        Stmt::Destructure { struct_name, qualifier, fields, value } => {
            let mut all = Vec::new();
            if let Some(q) = qualifier {
                all.push(("qualifier", str(ast.name_of(*q))));
            }
            all.push(("name", str(ast.name_of(*struct_name))));
            all.push((
                "fields",
                Json::Array(fields.iter().map(|f| str(ast.name_of(*f))).collect(), Span::new(0, 0)),
            ));
            all.push(("value", expr_to_json(ast, *value)));
            node("destructure", all)
        }
        Stmt::DestructureTuple { names, value } => node(
            "destructure_tuple",
            vec![
                (
                    "names",
                    Json::Array(
                        names.iter().map(|n| str(ast.name_of(*n))).collect(),
                        Span::new(0, 0),
                    ),
                ),
                ("value", expr_to_json(ast, *value)),
            ],
        ),
        Stmt::Borrow { value, unique, region, body } => node(
            "borrow",
            vec![
                ("value", str(ast.name_of(*value))),
                ("unique", Json::Bool(*unique, Span::new(0, 0))),
                ("region", str(ast.name_of(*region))),
                ("body", block_to_json(ast, body)),
            ],
        ),
        Stmt::Region { region, body } => node(
            "region",
            vec![("region", str(ast.name_of(*region))), ("body", block_to_json(ast, body))],
        ),
        Stmt::Expr(value) => node("expr", vec![("value", expr_to_json(ast, *value))]),
        Stmt::If { cond, then_block, else_block } => node(
            "if",
            vec![
                ("cond", expr_to_json(ast, *cond)),
                ("then", block_to_json(ast, then_block)),
                (
                    "else",
                    else_block
                        .as_ref()
                        .map(|b| block_to_json(ast, b))
                        .unwrap_or(Json::Null(Span::new(0, 0))),
                ),
            ],
        ),
        Stmt::While { cond, body } => node(
            "while",
            vec![("cond", expr_to_json(ast, *cond)), ("body", block_to_json(ast, body))],
        ),
        Stmt::Match { scrutinee, arms } => node(
            "match",
            vec![
                ("scrutinee", expr_to_json(ast, *scrutinee)),
                (
                    "arms",
                    Json::Array(
                        arms.iter()
                            .map(|arm| {
                                object(vec![
                                    ("pattern", pattern_to_json(ast, &arm.pattern)),
                                    ("body", block_to_json(ast, &arm.body)),
                                ])
                            })
                            .collect(),
                        Span::new(0, 0),
                    ),
                ),
            ],
        ),
        Stmt::Return(value) => node("return", vec![("value", expr_to_json(ast, *value))]),
        Stmt::Defer(value) => node("defer", vec![("value", expr_to_json(ast, *value))]),
    }
}

fn pattern_to_json(ast: &Ast, pattern: &Pattern) -> Json {
    match pattern {
        Pattern::Wildcard => node("wildcard", vec![]),
        Pattern::Variant { enum_name, qualifier, variant, bindings } => {
            let mut all = Vec::new();
            if let Some(q) = qualifier {
                all.push(("qualifier", str(ast.name_of(*q))));
            }
            all.push(("name", str(ast.name_of(*enum_name))));
            all.push(("variant", str(ast.name_of(*variant))));
            all.push((
                "bindings",
                Json::Array(
                    bindings
                        .iter()
                        .map(|b| {
                            b.map(|n| str(ast.name_of(n))).unwrap_or(Json::Null(Span::new(0, 0)))
                        })
                        .collect(),
                    Span::new(0, 0),
                ),
            ));
            node("variant", all)
        }
    }
}

fn expr_to_json(ast: &Ast, id: ExprId) -> Json {
    match ast.expr(id) {
        Expr::Int(value) => node("int", vec![("value", integer(*value))]),
        // The bits are a u64/u32 bit pattern, so they cross as a number's
        // text: a pattern above `i64::MAX` has no integer form the reader
        // keeps, and reinterpreting one as signed was how `0.0`'s high bit
        // came back negative.
        Expr::Float(bits) => {
            node("float", vec![("bits", Json::Number(bits.to_string(), Span::new(0, 0)))])
        }
        Expr::F32(bits) => {
            node("f32", vec![("bits", Json::Number(bits.to_string(), Span::new(0, 0)))])
        }
        Expr::Bool(value) => node("bool", vec![("value", Json::Bool(*value, Span::new(0, 0)))]),
        Expr::Str(text) => node("str", vec![("value", str(text))]),
        Expr::Name(name) => node("name", vec![("name", str(ast.name_of(*name)))]),
        Expr::StructLit { name, qualifier, fields } => {
            let mut all = Vec::new();
            if let Some(q) = qualifier {
                all.push(("qualifier", str(ast.name_of(*q))));
            }
            all.push(("name", str(ast.name_of(*name))));
            all.push((
                "fields",
                Json::Array(
                    fields
                        .iter()
                        .map(|(name, value)| {
                            object(vec![
                                ("name", str(ast.name_of(*name))),
                                ("value", expr_to_json(ast, *value)),
                            ])
                        })
                        .collect(),
                    Span::new(0, 0),
                ),
            ));
            node("struct_lit", all)
        }
        Expr::Field { base, name } => node(
            "field",
            vec![("base", expr_to_json(ast, *base)), ("name", str(ast.name_of(*name)))],
        ),
        Expr::Tuple(parts) => node(
            "tuple",
            vec![(
                "parts",
                Json::Array(parts.iter().map(|p| expr_to_json(ast, *p)).collect(), Span::new(0, 0)),
            )],
        ),
        Expr::TupleField { base, index } => node(
            "tuple_field",
            vec![("base", expr_to_json(ast, *base)), ("index", integer(i64::from(*index)))],
        ),
        Expr::Variant { enum_name, qualifier, variant, args } => {
            let mut all = Vec::new();
            if let Some(q) = qualifier {
                all.push(("qualifier", str(ast.name_of(*q))));
            }
            all.push(("name", str(ast.name_of(*enum_name))));
            all.push(("variant", str(ast.name_of(*variant))));
            all.push((
                "args",
                Json::Array(args.iter().map(|a| expr_to_json(ast, *a)).collect(), Span::new(0, 0)),
            ));
            node("variant", all)
        }
        Expr::Unary { op, operand } => node(
            "unary",
            vec![("op", str(unop_name(*op))), ("operand", expr_to_json(ast, *operand))],
        ),
        Expr::Binary { op, lhs, rhs } => node(
            "binary",
            vec![
                ("op", str(binop_name(*op))),
                ("lhs", expr_to_json(ast, *lhs)),
                ("rhs", expr_to_json(ast, *rhs)),
            ],
        ),
        Expr::Call { callee, qualifier, args } => {
            let mut all = Vec::new();
            if let Some(q) = qualifier {
                all.push(("qualifier", str(ast.name_of(*q))));
            }
            all.push(("name", str(ast.name_of(*callee))));
            all.push((
                "args",
                Json::Array(args.iter().map(|a| expr_to_json(ast, *a)).collect(), Span::new(0, 0)),
            ));
            node("call", all)
        }
        Expr::Index { base, index } => node(
            "index",
            vec![("base", expr_to_json(ast, *base)), ("index", expr_to_json(ast, *index))],
        ),
        Expr::Slice { base, start, end } => node(
            "slice",
            vec![
                ("base", expr_to_json(ast, *base)),
                ("start", expr_to_json(ast, *start)),
                ("end", expr_to_json(ast, *end)),
            ],
        ),
        Expr::Alloc { region, value } => node(
            "alloc",
            vec![("region", str(ast.name_of(*region))), ("value", expr_to_json(ast, *value))],
        ),
        Expr::AllocSlice { region, count, fill } => node(
            "alloc_slice",
            vec![
                ("region", str(ast.name_of(*region))),
                ("count", expr_to_json(ast, *count)),
                ("fill", expr_to_json(ast, *fill)),
            ],
        ),
    }
}

fn unop_name(op: UnOp) -> &'static str {
    match op {
        UnOp::Neg => "-",
        UnOp::Not => "!",
        UnOp::BitNot => "~",
        UnOp::Deref => "*",
    }
}

fn binop_name(op: BinOp) -> &'static str {
    match op {
        BinOp::And => "&&",
        BinOp::Or => "||",
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
    }
}

fn mode_to_json(mode: Option<Mode>) -> Json {
    match mode {
        None => Json::Null(Span::new(0, 0)),
        Some(Mode::Val) => str("val"),
        Some(Mode::Res) => str("res"),
    }
}

fn fn_to_json(ast: &Ast, decl: &crate::ast::FnDecl) -> Json {
    node(
        "fn",
        vec![
            ("public", Json::Bool(decl.public, Span::new(0, 0))),
            ("name", str(ast.name_of(decl.name))),
            (
                "generics",
                Json::Array(
                    decl.generics.iter().map(|g| str(ast.name_of(*g))).collect(),
                    Span::new(0, 0),
                ),
            ),
            (
                "bounds",
                Json::Array(
                    decl.bounds.iter().map(|b| mode_to_json(*b)).collect(),
                    Span::new(0, 0),
                ),
            ),
            (
                "regions",
                Json::Array(
                    decl.regions.iter().map(|r| str(ast.name_of(*r))).collect(),
                    Span::new(0, 0),
                ),
            ),
            (
                "outlives",
                Json::Array(
                    decl.outlives
                        .iter()
                        .map(|(inner, outer)| {
                            object(vec![
                                ("inner", str(ast.name_of(*inner))),
                                ("outer", str(ast.name_of(*outer))),
                            ])
                        })
                        .collect(),
                    Span::new(0, 0),
                ),
            ),
            (
                "params",
                Json::Array(
                    decl.params.iter().map(|p| param_to_json(ast, p)).collect(),
                    Span::new(0, 0),
                ),
            ),
            (
                "effects",
                Json::Array(
                    decl.effects.iter().map(|e| effect_to_json(ast, e)).collect(),
                    Span::new(0, 0),
                ),
            ),
            ("ret", ty_to_json(ast, decl.ret)),
            ("body", block_to_json(ast, &decl.body)),
        ],
    )
}

fn extern_to_json(ast: &Ast, decl: &crate::ast::ExternDecl) -> Json {
    node(
        "extern",
        vec![
            ("name", str(ast.name_of(decl.name))),
            (
                "regions",
                Json::Array(
                    decl.regions.iter().map(|r| str(ast.name_of(*r))).collect(),
                    Span::new(0, 0),
                ),
            ),
            (
                "params",
                Json::Array(
                    decl.params.iter().map(|p| param_to_json(ast, p)).collect(),
                    Span::new(0, 0),
                ),
            ),
            (
                "effects",
                Json::Array(
                    decl.effects.iter().map(|e| effect_to_json(ast, e)).collect(),
                    Span::new(0, 0),
                ),
            ),
            ("ret", ty_to_json(ast, decl.ret)),
            ("symbol", str(&decl.symbol)),
        ],
    )
}

fn struct_to_json(ast: &Ast, decl: &crate::ast::StructDecl) -> Json {
    node(
        "struct",
        vec![
            ("public", Json::Bool(decl.public, Span::new(0, 0))),
            ("name", str(ast.name_of(decl.name))),
            ("mode", mode_to_json(decl.mode)),
            (
                "generics",
                Json::Array(
                    decl.generics.iter().map(|g| str(ast.name_of(*g))).collect(),
                    Span::new(0, 0),
                ),
            ),
            (
                "bounds",
                Json::Array(
                    decl.bounds.iter().map(|b| mode_to_json(*b)).collect(),
                    Span::new(0, 0),
                ),
            ),
            (
                "fields",
                Json::Array(
                    decl.fields
                        .iter()
                        .map(|f| {
                            object(vec![
                                ("name", str(ast.name_of(f.name))),
                                ("ty", ty_to_json(ast, f.ty)),
                            ])
                        })
                        .collect(),
                    Span::new(0, 0),
                ),
            ),
        ],
    )
}

fn enum_to_json(ast: &Ast, decl: &crate::ast::EnumDecl) -> Json {
    node(
        "enum",
        vec![
            ("public", Json::Bool(decl.public, Span::new(0, 0))),
            ("name", str(ast.name_of(decl.name))),
            ("mode", mode_to_json(decl.mode)),
            (
                "generics",
                Json::Array(
                    decl.generics.iter().map(|g| str(ast.name_of(*g))).collect(),
                    Span::new(0, 0),
                ),
            ),
            (
                "bounds",
                Json::Array(
                    decl.bounds.iter().map(|b| mode_to_json(*b)).collect(),
                    Span::new(0, 0),
                ),
            ),
            (
                "variants",
                Json::Array(
                    decl.variants
                        .iter()
                        .map(|v| {
                            object(vec![
                                ("name", str(ast.name_of(v.name))),
                                (
                                    "payload",
                                    Json::Array(
                                        v.payload.iter().map(|p| ty_to_json(ast, *p)).collect(),
                                        Span::new(0, 0),
                                    ),
                                ),
                            ])
                        })
                        .collect(),
                    Span::new(0, 0),
                ),
            ),
        ],
    )
}

fn static_to_json(ast: &Ast, decl: &StaticDecl) -> Json {
    node(
        "static",
        vec![
            ("public", Json::Bool(decl.public, Span::new(0, 0))),
            ("name", str(ast.name_of(decl.name))),
            ("ty", ty_to_json(ast, decl.ty)),
            ("body", block_to_json(ast, &decl.body)),
        ],
    )
}

fn item_to_json(ast: &Ast, index: usize) -> Json {
    match &ast.items[index] {
        Item::Fn(decl) => fn_to_json(ast, decl),
        Item::Extern(decl) => extern_to_json(ast, decl),
        Item::Struct(decl) => struct_to_json(ast, decl),
        Item::Enum(decl) => enum_to_json(ast, decl),
        Item::Static(decl) => static_to_json(ast, decl),
    }
}

// ---- JSON → AST --------------------------------------------------------

/// Read the JSON form back into an `Ast` (`docs/structured-ingest.md` §2).
///
/// The tree is indistinguishable from a parsed one — print it, hash it,
/// check it. Refusals carry the three ingest rules, with spans into the
/// JSON text, which is the only file there is.
///
/// The built unit's items are edition 1, for the same reason a reparsed
/// `print` output's are: the canonical text form carries no `edition N;`
/// marker, and neither does the JSON form. A JSON that tries to set one
/// is refused rather than silently dropped.
pub fn json_to_ast(json: &Json) -> Result<Ast, Diagnostic> {
    let mut builder = Builder { ast: Ast::new() };
    builder.unit(json)?;
    Ok(builder.ast)
}

/// Builds the tree into a fresh `Ast`, interned names and arena pushes in
/// the same order a parser would have produced them. `Ast::new`, not
/// `Default::default()`: the prelude's names and the root module are the
/// start a parser gives, and an ingested tree is indistinguishable from
/// a parsed one only if it starts the same way.
struct Builder {
    ast: Ast,
}

impl Builder {
    // -- helpers, each one refusal kind -------------------------------

    /// A name that must be a cancho identifier and not a keyword: the
    /// two things a lexer would have decided before any node existed.
    fn name(&mut self, json: &Json, what: &str) -> Result<Symbol, Diagnostic> {
        let Json::Str(text, span) = json else {
            return Err(Diagnostic::new(
                Rule::IngestArity,
                format!("{what} is a name, which is a string"),
                json.span(),
            ));
        };
        if !is_identifier(text) {
            return Err(Diagnostic::new(
                Rule::IngestNode,
                format!("`{text}` is not a name the lexer would make one token of"),
                *span,
            ));
        }
        if is_keyword(text) {
            return Err(Diagnostic::new(
                Rule::IngestNode,
                format!("`{text}` is a keyword, which never names anything"),
                *span,
            ));
        }
        Ok(self.ast.symbols.intern(text))
    }

    fn string<'a>(&self, json: &'a Json, what: &str) -> Result<&'a str, Diagnostic> {
        match json {
            Json::Str(text, _) => Ok(text),
            other => {
                Err(Diagnostic::new(Rule::IngestArity, format!("{what} is a string"), other.span()))
            }
        }
    }

    fn integer(&self, json: &Json, what: &str) -> Result<i64, Diagnostic> {
        match json {
            Json::Int(value, _) => Ok(*value),
            other => Err(Diagnostic::new(
                Rule::IngestArity,
                format!("{what} is an integer"),
                other.span(),
            )),
        }
    }

    /// A literal's bit pattern: an unsigned integer, arriving as either an
    /// integer or a number's text, because a pattern above `i64::MAX` has
    /// no integer form the reader keeps.
    fn bits(&self, json: &Json, width: u32, what: &str) -> Result<u64, Diagnostic> {
        let text = match json {
            Json::Int(value, _) => value.to_string(),
            Json::Number(text, _) => text.clone(),
            other => {
                return Err(Diagnostic::new(
                    Rule::IngestArity,
                    format!("{what} is a bit pattern: a non-negative integer"),
                    other.span(),
                ));
            }
        };
        let limit = if width == 32 { u32::MAX as u64 } else { u64::MAX };
        let value: u64 = text.parse().map_err(|_| {
            Diagnostic::new(
                Rule::IngestArity,
                format!("{what} is a bit pattern: a non-negative integer, found {text}"),
                json.span(),
            )
        })?;
        if value > limit {
            return Err(Diagnostic::new(
                Rule::IngestArity,
                format!("{what} does not fit in {width} bits"),
                json.span(),
            ));
        }
        Ok(value)
    }

    fn boolean(&self, json: &Json, what: &str) -> Result<bool, Diagnostic> {
        match json {
            Json::Bool(value, _) => Ok(*value),
            other => Err(Diagnostic::new(
                Rule::IngestArity,
                format!("{what} is a boolean"),
                other.span(),
            )),
        }
    }

    fn array<'a>(&self, json: &'a Json, what: &str) -> Result<&'a [Json], Diagnostic> {
        match json {
            Json::Array(items, _) => Ok(items),
            other => {
                Err(Diagnostic::new(Rule::IngestArity, format!("{what} is an array"), other.span()))
            }
        }
    }

    fn object<'a>(&self, json: &'a Json, what: &str) -> Result<&'a [(String, Json)], Diagnostic> {
        match json {
            Json::Object(entries, _) => Ok(entries),
            other => Err(Diagnostic::new(
                Rule::IngestArity,
                format!("{what} is an object"),
                other.span(),
            )),
        }
    }

    /// The one field a node must have, with its presence the node's own
    /// rule: a missing field is a malformed node, not an unknown one.
    fn required<'a>(&self, json: &'a Json, key: &str) -> Result<&'a Json, Diagnostic> {
        json.field(key).ok_or_else(|| {
            Diagnostic::new(Rule::IngestArity, format!("the node needs a `{key}`"), json.span())
        })
    }

    fn mode(&mut self, json: &Json) -> Result<Option<Mode>, Diagnostic> {
        match json {
            Json::Null(_) => Ok(None),
            Json::Str(text, _) => match text.as_str() {
                "val" => Ok(Some(Mode::Val)),
                "res" => Ok(Some(Mode::Res)),
                other => Err(Diagnostic::new(
                    Rule::IngestNode,
                    format!("`{other}` is not a mode; the modes are `val` and `res`"),
                    json.span(),
                )),
            },
            other => Err(Diagnostic::new(
                Rule::IngestArity,
                "a mode is `\"val\"`, `\"res\"` or null",
                other.span(),
            )),
        }
    }

    // -- the unit ------------------------------------------------------

    fn unit(&mut self, json: &Json) -> Result<(), Diagnostic> {
        let entries = self.object(json, "a unit")?;
        let mut module_path: Option<String> = None;
        let mut imports: Vec<(String, Option<String>)> = Vec::new();
        let mut deferred: Vec<Json> = Vec::new();
        for (key, value) in entries {
            match key.as_str() {
                "module" => {
                    module_path = Some(self.string(value, "`module`")?.to_owned());
                }
                "imports" => {
                    for import in self.array(value, "`imports`")? {
                        let mut path = None;
                        let mut alias = None;
                        for (field, value) in self.object(import, "an import")? {
                            match field.as_str() {
                                "path" => {
                                    path =
                                        Some(self.string(value, "an import's `path`")?.to_owned())
                                }
                                "as" => {
                                    alias = Some(self.string(value, "an import's `as`")?.to_owned())
                                }
                                other => {
                                    return Err(Diagnostic::new(
                                        Rule::IngestNode,
                                        format!("an import has no `{other}` field"),
                                        value.span(),
                                    ));
                                }
                            }
                        }
                        let Some(path) = path else {
                            return Err(Diagnostic::new(
                                Rule::IngestArity,
                                "an import needs a `path`",
                                import.span(),
                            ));
                        };
                        imports.push((path, alias));
                    }
                }
                "items" => {
                    let items = self.array(value, "`items`")?;
                    if items.is_empty() {
                        return Err(Diagnostic::new(
                            Rule::IngestArity,
                            "a unit has at least one item",
                            value.span(),
                        ));
                    }
                    deferred.extend(items.iter().cloned());
                }
                "edition" => {
                    return Err(Diagnostic::new(
                        Rule::IngestNode,
                        "the JSON form has no `edition` field: an ingested unit is the current edition, as a reparsed `print` output is",
                        value.span(),
                    ));
                }
                other => {
                    return Err(Diagnostic::new(
                        Rule::IngestNode,
                        format!("a unit has no `{other}` field"),
                        value.span(),
                    ));
                }
            }
        }
        let module = match &module_path {
            Some(path) => {
                let mut segments = Vec::new();
                for segment in path.split('.') {
                    segments.push(self.name(
                        &Json::Str(segment.to_owned(), json.span()),
                        "a module path segment",
                    )?);
                }
                self.ast.module_named(&segments)
            }
            None => 0,
        };
        for (path, alias) in imports {
            let mut segments = Vec::new();
            for segment in path.split('.') {
                segments.push(
                    self.name(
                        &Json::Str(segment.to_owned(), json.span()),
                        "an import path segment",
                    )?,
                );
            }
            let alias = match alias {
                Some(name) => self.name(&Json::Str(name, json.span()), "an import's alias")?,
                None => *segments.last().expect("a path has a last segment"),
            };
            self.ast.modules[module as usize].imports.push(crate::ast::Import {
                path: segments,
                alias,
                span: Span::new(0, 0),
            });
        }
        for item in deferred {
            self.item_in(&item, module)?;
        }
        Ok(())
    }
}

impl Builder {
    // -- items ---------------------------------------------------------

    fn item_in(&mut self, json: &Json, module: u32) -> Result<(), Diagnostic> {
        let kind = self.required(json, "kind")?;
        let kind = self.string(kind, "a node's `kind`")?;
        let item = match kind {
            "fn" => self.fn_item(json)?,
            "extern" => self.extern_item(json)?,
            "struct" => self.struct_item(json)?,
            "enum" => self.enum_item(json)?,
            "static" => self.static_item(json)?,
            other => {
                return Err(Diagnostic::new(
                    Rule::IngestNode,
                    format!(
                        "`{other}` is not an item kind; the items are fn, extern, struct, enum and static"
                    ),
                    kind_json_span(json),
                ));
            }
        };
        self.ast.push_item_in(item, Span::new(0, 0), module, 1);
        Ok(())
    }

    fn fn_item(&mut self, json: &Json) -> Result<crate::ast::Item, Diagnostic> {
        let public = match json.field("public") {
            Some(value) => self.boolean(value, "`public`")?,
            None => false,
        };
        let name = self.name(self.required(json, "name")?, "an fn's `name`")?;
        let generics = self.name_list(json.field("generics"), "a generic parameter")?;
        let bounds = self.bounds(json.field("bounds"))?;
        let regions = self.name_list(json.field("regions"), "a region parameter")?;
        let mut outlives = Vec::new();
        if let Some(list) = json.field("outlives") {
            for pair in self.array(list, "`outlives`")? {
                let inner = self.name(self.required(pair, "inner")?, "an outlives `inner`")?;
                let outer = self.name(self.required(pair, "outer")?, "an outlives `outer`")?;
                outlives.push((inner, outer));
            }
        }
        let mut params = Vec::new();
        for param in self.array(self.required(json, "params")?, "an fn's `params`")? {
            let name = self.name(self.required(param, "name")?, "a parameter's `name`")?;
            let ty = self.ty(self.required(param, "ty")?)?;
            params.push(crate::ast::Param { name, ty });
        }
        let mut effects = Vec::new();
        if let Some(list) = json.field("effects") {
            for label in self.array(list, "an effect row")? {
                effects.push(self.effect(label)?);
            }
        }
        let ret = self.ty(self.required(json, "ret")?)?;
        let body = self.block(self.required(json, "body")?)?;
        Ok(crate::ast::Item::Fn(crate::ast::FnDecl {
            name,
            public,
            generics,
            bounds,
            regions,
            outlives,
            params,
            effects,
            ret,
            body,
        }))
    }

    fn extern_item(&mut self, json: &Json) -> Result<crate::ast::Item, Diagnostic> {
        let name = self.name(self.required(json, "name")?, "an extern fn's `name`")?;
        let regions = self.name_list(json.field("regions"), "a region parameter")?;
        let mut params = Vec::new();
        for param in self.array(self.required(json, "params")?, "an extern fn's `params`")? {
            let name = self.name(self.required(param, "name")?, "a parameter's `name`")?;
            let ty = self.ty(self.required(param, "ty")?)?;
            params.push(crate::ast::Param { name, ty });
        }
        let mut effects = Vec::new();
        if let Some(list) = json.field("effects") {
            for label in self.array(list, "an effect row")? {
                effects.push(self.effect(label)?);
            }
        }
        let ret = self.ty(self.required(json, "ret")?)?;
        let symbol =
            self.string(self.required(json, "symbol")?, "an extern fn's `symbol`")?.to_owned();
        Ok(crate::ast::Item::Extern(crate::ast::ExternDecl {
            name,
            regions,
            params,
            effects,
            ret,
            symbol,
        }))
    }

    fn struct_item(&mut self, json: &Json) -> Result<crate::ast::Item, Diagnostic> {
        let public = match json.field("public") {
            Some(value) => self.boolean(value, "`public`")?,
            None => false,
        };
        let name = self.name(self.required(json, "name")?, "a struct's `name`")?;
        let mode = match json.field("mode") {
            Some(value) => self.mode(value)?,
            None => None,
        };
        let generics = self.name_list(json.field("generics"), "a generic parameter")?;
        let bounds = self.bounds(json.field("bounds"))?;
        let mut fields = Vec::new();
        for field in self.array(self.required(json, "fields")?, "a struct's `fields`")? {
            let name = self.name(self.required(field, "name")?, "a field's `name`")?;
            let ty = self.ty(self.required(field, "ty")?)?;
            fields.push(crate::ast::FieldDecl { name, ty });
        }
        Ok(crate::ast::Item::Struct(crate::ast::StructDecl {
            name,
            public,
            mode,
            generics,
            bounds,
            fields,
        }))
    }

    fn enum_item(&mut self, json: &Json) -> Result<crate::ast::Item, Diagnostic> {
        let public = match json.field("public") {
            Some(value) => self.boolean(value, "`public`")?,
            None => false,
        };
        let name = self.name(self.required(json, "name")?, "an enum's `name`")?;
        let mode = match json.field("mode") {
            Some(value) => self.mode(value)?,
            None => None,
        };
        let generics = self.name_list(json.field("generics"), "a generic parameter")?;
        let bounds = self.bounds(json.field("bounds"))?;
        let mut variants = Vec::new();
        for variant in self.array(self.required(json, "variants")?, "an enum's `variants`")? {
            let name = self.name(self.required(variant, "name")?, "a variant's `name`")?;
            let mut payload = Vec::new();
            if let Some(list) = variant.field("payload") {
                for ty in self.array(list, "a variant's `payload`")? {
                    payload.push(self.ty(ty)?);
                }
            }
            variants.push(crate::ast::VariantDecl { name, payload });
        }
        Ok(crate::ast::Item::Enum(crate::ast::EnumDecl {
            name,
            public,
            mode,
            generics,
            bounds,
            variants,
        }))
    }

    fn static_item(&mut self, json: &Json) -> Result<crate::ast::Item, Diagnostic> {
        let public = match json.field("public") {
            Some(value) => self.boolean(value, "`public`")?,
            None => false,
        };
        let name = self.name(self.required(json, "name")?, "a static's `name`")?;
        let ty = self.ty(self.required(json, "ty")?)?;
        let body = self.block(self.required(json, "body")?)?;
        Ok(crate::ast::Item::Static(crate::ast::StaticDecl { name, public, ty, body }))
    }

    fn name_list(&mut self, json: Option<&Json>, what: &str) -> Result<Vec<Symbol>, Diagnostic> {
        let Some(json) = json else { return Ok(Vec::new()) };
        let mut out = Vec::new();
        for name in self.array(json, "a parameter list")? {
            out.push(self.name(name, what)?);
        }
        Ok(out)
    }

    fn bounds(&mut self, json: Option<&Json>) -> Result<Vec<Option<Mode>>, Diagnostic> {
        let Some(json) = json else { return Ok(Vec::new()) };
        let mut out = Vec::new();
        for bound in self.array(json, "a bounds list")? {
            out.push(self.mode(bound)?);
        }
        Ok(out)
    }

    fn effect(&mut self, json: &Json) -> Result<crate::ast::EffectLabel, Diagnostic> {
        let name = self.name(self.required(json, "name")?, "an effect label's `name`")?;
        let argument = match json.field("argument") {
            Some(Json::Null(_)) | None => None,
            Some(Json::Str(text, _)) => Some(text.clone()),
            Some(other) => {
                return Err(Diagnostic::new(
                    Rule::IngestArity,
                    "an effect label's `argument` is a string or null",
                    other.span(),
                ));
            }
        };
        Ok(crate::ast::EffectLabel { name, argument })
    }

    // -- types --------------------------------------------------------

    fn ty(&mut self, json: &Json) -> Result<TypeId, Diagnostic> {
        let built = match json {
            Json::Object(entries, _) => {
                // The plain form first: `{"name": "int"}` with its optional
                // `qualifier` and `args` is a named type and carries no
                // `kind` — the same shape the encoder writes.
                if !entries.iter().any(|(k, _)| k == "kind") {
                    let name = self.required(json, "name")?;
                    let name = self.name(name, "a type's `name`")?;
                    let qualifier = match json.field("qualifier") {
                        Some(Json::Null(_)) | None => None,
                        Some(value) => Some(self.name(value, "a type's `qualifier`")?),
                    };
                    let mut args = Vec::new();
                    if let Some(list) = json.field("args") {
                        for arg in self.array(list, "a type's `args`")? {
                            args.push(self.ty(arg)?);
                        }
                    }
                    crate::ast::TypeExpr::Name { name, qualifier, args }
                } else {
                    let kind = entries
                        .iter()
                        .find(|(k, _)| k == "kind")
                        .map(|(_, v)| v)
                        .expect("checked above");
                    let kind = self.string(kind, "a type's `kind`")?;
                    match kind {
                        "ref" => {
                            let unique =
                                self.boolean(self.required(json, "unique")?, "a ref's `unique`")?;
                            let region =
                                self.name(self.required(json, "region")?, "a ref's `region`")?;
                            let inner = self.ty(self.required(json, "inner")?)?;
                            crate::ast::TypeExpr::Ref { unique, region, inner }
                        }
                        "slice" => {
                            crate::ast::TypeExpr::Slice(self.ty(self.required(json, "inner")?)?)
                        }
                        "tuple" => {
                            let mut parts = Vec::new();
                            for part in
                                self.array(self.required(json, "parts")?, "a tuple's `parts`")?
                            {
                                parts.push(self.ty(part)?);
                            }
                            crate::ast::TypeExpr::Tuple(parts)
                        }
                        "lit" => crate::ast::TypeExpr::Lit(
                            self.string(self.required(json, "value")?, "a lit's `value`")?
                                .to_owned(),
                        ),
                        "fn" => {
                            let mut params = Vec::new();
                            for param in
                                self.array(self.required(json, "params")?, "an fn type's `params`")?
                            {
                                params.push(self.ty(param)?);
                            }
                            let mut effects = Vec::new();
                            if let Some(list) = json.field("effects") {
                                for label in self.array(list, "an effect row")? {
                                    effects.push(self.effect(label)?);
                                }
                            }
                            let ret = self.ty(self.required(json, "ret")?)?;
                            crate::ast::TypeExpr::Fn { params, effects, ret }
                        }
                        other => {
                            return Err(Diagnostic::new(
                                Rule::IngestNode,
                                format!(
                                    "`{other}` is not a type kind; the types are ref, slice, tuple, lit and fn"
                                ),
                                kind_json_span(json),
                            ));
                        }
                    }
                }
            }
            other => {
                return Err(Diagnostic::new(
                    Rule::IngestArity,
                    "a type is an object: `{\"name\": ...}` or `{\"kind\": ...}`",
                    other.span(),
                ));
            }
        };
        Ok(self.ast.push_type(built, Span::new(0, 0)))
    }

    // -- statements and expressions -----------------------------------

    fn block(&mut self, json: &Json) -> Result<crate::ast::Block, Diagnostic> {
        let mut stmts = Vec::new();
        for stmt in self.array(json, "a block")? {
            stmts.push(self.stmt(stmt)?);
        }
        Ok(crate::ast::Block { stmts })
    }

    fn stmt(&mut self, json: &Json) -> Result<crate::ast::StmtId, Diagnostic> {
        let kind = self.string(self.required(json, "kind")?, "a statement's `kind`")?;
        let built = match kind {
            "let" => {
                let mutable = match json.field("mutable") {
                    Some(value) => self.boolean(value, "`mutable`")?,
                    None => false,
                };
                let name = self.name(self.required(json, "name")?, "a let's `name`")?;
                let ty = match json.field("ty") {
                    Some(Json::Null(_)) | None => None,
                    Some(value) => Some(self.ty(value)?),
                };
                let value = self.expr(self.required(json, "value")?)?;
                crate::ast::Stmt::Let { name, mutable, ty, value }
            }
            "assign" => {
                let place = self.expr(self.required(json, "place")?)?;
                let value = self.expr(self.required(json, "value")?)?;
                crate::ast::Stmt::Assign { place, value }
            }
            "destructure" => {
                let qualifier = match json.field("qualifier") {
                    Some(Json::Null(_)) | None => None,
                    Some(value) => Some(self.name(value, "a destructure's `qualifier`")?),
                };
                let struct_name =
                    self.name(self.required(json, "name")?, "a destructure's `name`")?;
                let mut fields = Vec::new();
                for field in
                    self.array(self.required(json, "fields")?, "a destructure's `fields`")?
                {
                    fields.push(self.name(field, "a destructured field")?);
                }
                let value = self.expr(self.required(json, "value")?)?;
                crate::ast::Stmt::Destructure { struct_name, qualifier, fields, value }
            }
            "destructure_tuple" => {
                let mut names = Vec::new();
                for name in
                    self.array(self.required(json, "names")?, "a tuple destructure's `names`")?
                {
                    names.push(self.name(name, "a tuple destructure's binding")?);
                }
                let value = self.expr(self.required(json, "value")?)?;
                crate::ast::Stmt::DestructureTuple { names, value }
            }
            "borrow" => {
                let value = self.name(self.required(json, "value")?, "a borrow's `value`")?;
                let unique = self.boolean(self.required(json, "unique")?, "a borrow's `unique`")?;
                let region = self.name(self.required(json, "region")?, "a borrow's `region`")?;
                let body = self.block(self.required(json, "body")?)?;
                crate::ast::Stmt::Borrow { value, unique, region, body }
            }
            "region" => {
                let region = self.name(self.required(json, "region")?, "a region's `region`")?;
                let body = self.block(self.required(json, "body")?)?;
                crate::ast::Stmt::Region { region, body }
            }
            "expr" => crate::ast::Stmt::Expr(self.expr(self.required(json, "value")?)?),
            "if" => {
                let cond = self.expr(self.required(json, "cond")?)?;
                let then_block = self.block(self.required(json, "then")?)?;
                let else_block = match json.field("else") {
                    Some(Json::Null(_)) | None => None,
                    Some(value) => Some(self.block(value)?),
                };
                crate::ast::Stmt::If { cond, then_block, else_block }
            }
            "while" => {
                let cond = self.expr(self.required(json, "cond")?)?;
                let body = self.block(self.required(json, "body")?)?;
                crate::ast::Stmt::While { cond, body }
            }
            "match" => {
                let scrutinee = self.expr(self.required(json, "scrutinee")?)?;
                let mut arms = Vec::new();
                for arm in self.array(self.required(json, "arms")?, "a match's `arms`")? {
                    let pattern = self.pattern(self.required(arm, "pattern")?)?;
                    let body = self.block(self.required(arm, "body")?)?;
                    arms.push(crate::ast::MatchArm { pattern, body });
                }
                crate::ast::Stmt::Match { scrutinee, arms }
            }
            "return" => crate::ast::Stmt::Return(self.expr(self.required(json, "value")?)?),
            "defer" => crate::ast::Stmt::Defer(self.expr(self.required(json, "value")?)?),
            other => {
                return Err(Diagnostic::new(
                    Rule::IngestNode,
                    format!(
                        "`{other}` is not a statement kind; the statements are let, assign, destructure, destructure_tuple, borrow, region, expr, if, while, match, return and defer"
                    ),
                    kind_json_span(json),
                ));
            }
        };
        Ok(self.ast.push_stmt(built, Span::new(0, 0)))
    }

    fn pattern(&mut self, json: &Json) -> Result<crate::ast::Pattern, Diagnostic> {
        let kind = self.string(self.required(json, "kind")?, "a pattern's `kind`")?;
        match kind {
            "wildcard" => Ok(crate::ast::Pattern::Wildcard),
            "variant" => {
                let qualifier = match json.field("qualifier") {
                    Some(Json::Null(_)) | None => None,
                    Some(value) => Some(self.name(value, "a variant pattern's `qualifier`")?),
                };
                let enum_name =
                    self.name(self.required(json, "name")?, "a variant pattern's `name`")?;
                let variant =
                    self.name(self.required(json, "variant")?, "a variant pattern's `variant`")?;
                let mut bindings = Vec::new();
                for binding in
                    self.array(self.required(json, "bindings")?, "a variant pattern's `bindings`")?
                {
                    match binding {
                        Json::Null(_) => bindings.push(None),
                        other => bindings.push(Some(self.name(other, "a pattern binding")?)),
                    }
                }
                Ok(crate::ast::Pattern::Variant { enum_name, qualifier, variant, bindings })
            }
            other => Err(Diagnostic::new(
                Rule::IngestNode,
                format!("`{other}` is not a pattern kind; the patterns are wildcard and variant"),
                kind_json_span(json),
            )),
        }
    }

    fn expr(&mut self, json: &Json) -> Result<ExprId, Diagnostic> {
        let kind = self.string(self.required(json, "kind")?, "an expression's `kind`")?;
        let built = match kind {
            "int" => {
                let value =
                    self.integer(self.required(json, "value")?, "an int literal's `value`")?;
                Expr::Int(value)
            }
            "float" => {
                let bits =
                    self.bits(self.required(json, "bits")?, 64, "a float literal's `bits`")?;
                Expr::Float(bits)
            }
            "f32" => {
                let bits =
                    self.bits(self.required(json, "bits")?, 32, "an f32 literal's `bits`")?;
                Expr::F32(bits as u32)
            }
            "bool" => {
                let value =
                    self.boolean(self.required(json, "value")?, "a bool literal's `value`")?;
                Expr::Bool(value)
            }
            "str" => {
                let value =
                    self.string(self.required(json, "value")?, "a str literal's `value`")?;
                Expr::Str(value.to_owned())
            }
            "name" => {
                Expr::Name(self.name(self.required(json, "name")?, "a name expression's `name`")?)
            }
            "struct_lit" => {
                let qualifier = match json.field("qualifier") {
                    Some(Json::Null(_)) | None => None,
                    Some(value) => Some(self.name(value, "a struct literal's `qualifier`")?),
                };
                let name = self.name(self.required(json, "name")?, "a struct literal's `name`")?;
                let mut fields = Vec::new();
                for field in
                    self.array(self.required(json, "fields")?, "a struct literal's `fields`")?
                {
                    let field_name =
                        self.name(self.required(field, "name")?, "a literal field's `name`")?;
                    let value = self.expr(self.required(field, "value")?)?;
                    fields.push((field_name, value));
                }
                Expr::StructLit { name, qualifier, fields }
            }
            "field" => {
                let base = self.expr(self.required(json, "base")?)?;
                let name = self.name(self.required(json, "name")?, "a field access's `name`")?;
                Expr::Field { base, name }
            }
            "tuple" => {
                let mut parts = Vec::new();
                for part in self.array(self.required(json, "parts")?, "a tuple's `parts`")? {
                    parts.push(self.expr(part)?);
                }
                if parts.is_empty() {
                    return Err(Diagnostic::new(
                        Rule::IngestArity,
                        "a tuple has at least one part",
                        json.span(),
                    ));
                }
                Expr::Tuple(parts)
            }
            "tuple_field" => {
                let base = self.expr(self.required(json, "base")?)?;
                let index =
                    self.integer(self.required(json, "index")?, "a tuple field's `index`")?;
                if index < 0 {
                    return Err(Diagnostic::new(
                        Rule::IngestArity,
                        "a tuple field's `index` is a position, so never negative",
                        json.span(),
                    ));
                }
                Expr::TupleField { base, index: index as u32 }
            }
            "variant" => {
                let qualifier = match json.field("qualifier") {
                    Some(Json::Null(_)) | None => None,
                    Some(value) => Some(self.name(value, "a variant's `qualifier`")?),
                };
                let enum_name = self.name(self.required(json, "name")?, "a variant's `name`")?;
                let variant =
                    self.name(self.required(json, "variant")?, "a variant's `variant`")?;
                let mut args = Vec::new();
                if let Some(list) = json.field("args") {
                    for arg in self.array(list, "a variant's `args`")? {
                        args.push(self.expr(arg)?);
                    }
                }
                Expr::Variant { enum_name, qualifier, variant, args }
            }
            "unary" => {
                let op = self.string(self.required(json, "op")?, "a unary operator")?;
                let op = match op {
                    "-" => crate::ast::UnOp::Neg,
                    "!" => crate::ast::UnOp::Not,
                    "~" => crate::ast::UnOp::BitNot,
                    "*" => crate::ast::UnOp::Deref,
                    other => {
                        return Err(Diagnostic::new(
                            Rule::IngestNode,
                            format!(
                                "`{other}` is not a unary operator; they are `-`, `!`, `~` and `*`"
                            ),
                            json.span(),
                        ));
                    }
                };
                let operand = self.expr(self.required(json, "operand")?)?;
                Expr::Unary { op, operand }
            }
            "binary" => {
                let op = self.string(self.required(json, "op")?, "a binary operator")?;
                let op = match op {
                    "&&" => crate::ast::BinOp::And,
                    "||" => crate::ast::BinOp::Or,
                    "+" => crate::ast::BinOp::Add,
                    "-" => crate::ast::BinOp::Sub,
                    "*" => crate::ast::BinOp::Mul,
                    "/" => crate::ast::BinOp::Div,
                    "%" => crate::ast::BinOp::Rem,
                    "==" => crate::ast::BinOp::Eq,
                    "!=" => crate::ast::BinOp::Ne,
                    "<" => crate::ast::BinOp::Lt,
                    "<=" => crate::ast::BinOp::Le,
                    ">" => crate::ast::BinOp::Gt,
                    ">=" => crate::ast::BinOp::Ge,
                    "&" => crate::ast::BinOp::BitAnd,
                    "|" => crate::ast::BinOp::BitOr,
                    "^" => crate::ast::BinOp::BitXor,
                    "<<" => crate::ast::BinOp::Shl,
                    ">>" => crate::ast::BinOp::Shr,
                    other => {
                        return Err(Diagnostic::new(
                            Rule::IngestNode,
                            format!("`{other}` is not a binary operator"),
                            json.span(),
                        ));
                    }
                };
                let lhs = self.expr(self.required(json, "lhs")?)?;
                let rhs = self.expr(self.required(json, "rhs")?)?;
                Expr::Binary { op, lhs, rhs }
            }
            "call" => {
                let qualifier = match json.field("qualifier") {
                    Some(Json::Null(_)) | None => None,
                    Some(value) => Some(self.name(value, "a call's `qualifier`")?),
                };
                let callee = self.name(self.required(json, "name")?, "a call's `name`")?;
                let mut args = Vec::new();
                for arg in self.array(self.required(json, "args")?, "a call's `args`")? {
                    args.push(self.expr(arg)?);
                }
                Expr::Call { callee, qualifier, args }
            }
            "index" => {
                let base = self.expr(self.required(json, "base")?)?;
                let index = self.expr(self.required(json, "index")?)?;
                Expr::Index { base, index }
            }
            "slice" => {
                let base = self.expr(self.required(json, "base")?)?;
                let start = self.expr(self.required(json, "start")?)?;
                let end = self.expr(self.required(json, "end")?)?;
                Expr::Slice { base, start, end }
            }
            "alloc" => {
                let region = self.name(self.required(json, "region")?, "an alloc's `region`")?;
                let value = self.expr(self.required(json, "value")?)?;
                Expr::Alloc { region, value }
            }
            "alloc_slice" => {
                let region =
                    self.name(self.required(json, "region")?, "an alloc_slice's `region`")?;
                let count = self.expr(self.required(json, "count")?)?;
                let fill = self.expr(self.required(json, "fill")?)?;
                Expr::AllocSlice { region, count, fill }
            }
            other => {
                return Err(Diagnostic::new(
                    Rule::IngestNode,
                    format!("`{other}` is not an expression kind"),
                    kind_json_span(json),
                ));
            }
        };
        Ok(self.ast.push_expr(built, Span::new(0, 0)))
    }
}

/// The span of a node's `kind` field, so an unknown-vocabulary refusal
/// points at the word rather than the whole node.
fn kind_json_span(json: &Json) -> Span {
    json.field("kind").map(|k| k.span()).unwrap_or_else(|| json.span())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::{read_json, write_json};

    fn round_trip(src: &str) -> String {
        let ast = crate::parse(src).expect("the fixture parses");
        let json = ast_to_json(&ast);
        let text = write_json(&json);
        let back =
            json_to_ast(&read_json(&text).expect("the JSON reads")).expect("the JSON builds");
        crate::print(&back)
    }

    #[test]
    fn a_function_round_trips_through_json() {
        let src = "fn twice(n: int) -> [] int { return n + n; }";
        let ast = crate::parse(src).unwrap();
        let expected = crate::print(&ast);
        assert_eq!(round_trip(src), expected);
    }

    #[test]
    fn json_is_a_fixed_point_of_ingest() {
        let src = "fn twice(n: int) -> [] int { return n + n; }";
        let first = round_trip(src);
        assert_eq!(round_trip(&first), first, "ingesting the ingested text moved");
    }

    #[test]
    fn a_module_and_its_imports_round_trip() {
        let src = "module m;\nimport std.io;\nfn f() -> [] int { return 0; }";
        let ast = crate::parse(src).unwrap();
        let expected = crate::print(&ast);
        assert_eq!(round_trip(src), expected);
    }

    #[test]
    fn every_statement_kind_round_trips() {
        let src = "fn f[&r](s: &r [int]) -> [] int {\
                    let x = s[0];\
                    var y = x;\
                    y = y + 1;\
                    if y > 0 { return y; } else { y = 0; }\
                    while y < 10 { y = y + 1; }\
                    match Shape::Rect(w, h) { Shape::Rect(a, b) => { return a; } _ => { return 0; } }\
                    return y;\
                   }";
        let ast = crate::parse(src).unwrap();
        let expected = crate::print(&ast);
        assert_eq!(round_trip(src), expected);
    }

    #[test]
    fn not_json_is_refused_with_its_own_rule() {
        let err = read_json("this is not json").unwrap_err();
        assert_eq!(err.rule, Rule::IngestJson);
    }

    #[test]
    fn an_unknown_node_kind_is_refused_with_its_own_rule() {
        let text = "{\"items\": [{\"kind\": \"fnx\", \"name\": \"f\"}]}";
        let json = read_json(text).unwrap();
        let err = json_to_ast(&json).unwrap_err();
        assert_eq!(err.rule, Rule::IngestNode);
        assert!(err.message.contains("fnx"), "{}", err.message);
    }

    #[test]
    fn a_known_node_with_a_missing_field_is_refused_with_its_own_rule() {
        let text = "{\"items\": [{\"kind\": \"fn\"}]}";
        let json = read_json(text).unwrap();
        let err = json_to_ast(&json).unwrap_err();
        assert_eq!(err.rule, Rule::IngestArity);
        assert!(err.message.contains("name"), "{}", err.message);
    }

    #[test]
    fn a_keyword_never_names_anything() {
        let text = "{\"items\": [{\"kind\": \"fn\", \"name\": \"let\"}]}";
        let json = read_json(text).unwrap();
        let err = json_to_ast(&json).unwrap_err();
        assert_eq!(err.rule, Rule::IngestNode);
        assert!(err.message.contains("keyword"), "{}", err.message);
    }

    #[test]
    fn an_edition_field_is_refused_rather_than_dropped() {
        let text = "{\"edition\": 5, \"items\": []}";
        let json = read_json(text).unwrap();
        let err = json_to_ast(&json).unwrap_err();
        assert_eq!(err.rule, Rule::IngestNode);
        assert!(err.message.contains("edition"), "{}", err.message);
    }
}
