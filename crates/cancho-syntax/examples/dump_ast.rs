//! The oracle for `examples/selfhost/parser.cho` (`docs/self-hosting.md`, stage 2).
//!
//! Reads a source file on standard input, parses it with the Rust parser, and writes
//! the tree as a **postfix** listing, one node per line, children before their parent:
//! exactly the order the parser builds them in, which is the order a parser that never
//! allocates a tree can print them in. A node says how many children of each kind it
//! takes off the stack, so the listing is unambiguous without brackets. A refusal is a
//! single line, `ERR rule-tag start end`.
//!
//!     E.Int 7 @ 4 5
//!     E.Name x @ 8 9
//!     E.Binary Add @ 4 9
//!
//! Not compared: float values (the listing has the literal's span and whether it is an
//! `f32`, not its bits; the Rust side stores bits and the port has no float parser of
//! its own yet), symbol ids,
//! and message text. Strings are written as hex.
//!
//!     cargo run -q -p cancho-syntax --example dump_ast < some_file.cho

use std::io::Read;

use cancho_syntax::ast::*;
use cancho_syntax::span::Span;
use cancho_syntax::{Ast, SourceMap, parse_into};

struct Dump<'a> {
    ast: &'a Ast,
    out: String,
}

fn hex(s: &str) -> String {
    if s.is_empty() {
        return "-".to_owned();
    }
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

impl<'a> Dump<'a> {
    fn name(&self, sym: Symbol) -> &str {
        self.ast.name_of(sym)
    }

    fn qual(&self, q: Option<Symbol>) -> String {
        q.map_or("-".to_owned(), |s| self.name(s).to_owned())
    }

    fn list(&self, names: &[Symbol]) -> String {
        let parts: Vec<&str> = names.iter().map(|s| self.name(*s)).collect();
        format!("[{}]", parts.join(","))
    }

    fn bounded(&self, names: &[Symbol], bounds: &[Option<Mode>]) -> String {
        let parts: Vec<String> = names
            .iter()
            .zip(bounds)
            .map(|(s, b)| match b {
                Some(Mode::Val) => format!("{}:val", self.name(*s)),
                Some(Mode::Res) => format!("{}:res", self.name(*s)),
                None => self.name(*s).to_owned(),
            })
            .collect();
        format!("[{}]", parts.join(","))
    }

    fn effects(&self, row: &[EffectLabel]) -> String {
        let parts: Vec<String> = row
            .iter()
            .map(|e| match &e.argument {
                Some(a) => format!("{}:{}", self.name(e.name), hex(a)),
                None => self.name(e.name).to_owned(),
            })
            .collect();
        format!("[{}]", parts.join(","))
    }

    fn say(&mut self, text: String) {
        self.out.push_str(&text);
        self.out.push('\n');
    }

    fn effects_line(&mut self, row: &[EffectLabel]) {
        let text = self.effects(row);
        self.say(format!("H.Effects {text}"));
    }

    fn generics_line(
        &mut self,
        names: &[Symbol],
        bounds: &[Option<Mode>],
        regions: &[Symbol],
        outlives: &[(Symbol, Symbol)],
    ) {
        let outlives: Vec<String> =
            outlives.iter().map(|(a, b)| format!("{}<={}", self.name(*a), self.name(*b))).collect();
        self.say(format!(
            "H.Generics {} {} [{}]",
            self.bounded(names, bounds),
            self.list(regions),
            outlives.join(",")
        ));
    }

    fn line(&mut self, text: String, span: Span) {
        self.say(format!("{text} @ {} {}", span.start, span.end));
    }

    fn ty(&mut self, id: TypeId) {
        let span = self.ast.type_span(id);
        match self.ast.ty(id).clone() {
            TypeExpr::Name { name, qualifier, args } => {
                for a in &args {
                    self.ty(*a);
                }
                let text =
                    format!("T.Name {} {} {}", self.qual(qualifier), self.name(name), args.len());
                self.line(text, span);
            }
            TypeExpr::Ref { unique, region, inner } => {
                self.ty(inner);
                let text = format!("T.Ref {} {}", u8::from(unique), self.name(region));
                self.line(text, span);
            }
            TypeExpr::Slice(inner) => {
                self.ty(inner);
                self.line("T.Slice".to_owned(), span);
            }
            TypeExpr::Tuple(parts) => {
                for p in &parts {
                    self.ty(*p);
                }
                self.line(format!("T.Tuple {}", parts.len()), span);
            }
            TypeExpr::Lit(text) => self.line(format!("T.Lit {}", hex(&text)), span),
            TypeExpr::Fn { params, effects, ret } => {
                for p in &params {
                    self.ty(*p);
                }
                self.effects_line(&effects);
                self.ty(ret);
                self.line(format!("T.Fn {}", params.len()), span);
            }
        }
    }

    fn expr(&mut self, id: ExprId) {
        let span = self.ast.expr_span(id);
        match self.ast.expr(id).clone() {
            Expr::Int(v) => self.line(format!("E.Int {v}"), span),
            Expr::Float(_) => self.line("E.Float".to_owned(), span),
            Expr::F32(_) => self.line("E.F32".to_owned(), span),
            Expr::Bool(b) => self.line(format!("E.Bool {}", u8::from(b)), span),
            Expr::Str(s) => self.line(format!("E.Str {}", hex(&s)), span),
            Expr::Name(n) => {
                let text = format!("E.Name {}", self.name(n));
                self.line(text, span);
            }
            Expr::StructLit { name, qualifier, fields } => {
                for (f, v) in &fields {
                    self.say(format!("F.Name {}", self.name(*f)));
                    self.expr(*v);
                }
                let text = format!(
                    "E.StructLit {} {} {}",
                    self.qual(qualifier),
                    self.name(name),
                    fields.len()
                );
                self.line(text, span);
            }
            Expr::Field { base, name } => {
                self.expr(base);
                let text = format!("E.Field {}", self.name(name));
                self.line(text, span);
            }
            Expr::Tuple(parts) => {
                for p in &parts {
                    self.expr(*p);
                }
                self.line(format!("E.Tuple {}", parts.len()), span);
            }
            Expr::TupleField { base, index } => {
                self.expr(base);
                self.line(format!("E.TupleField {index}"), span);
            }
            Expr::Variant { enum_name, qualifier, variant, args } => {
                for a in &args {
                    self.expr(*a);
                }
                let text = format!(
                    "E.Variant {} {} {} {}",
                    self.qual(qualifier),
                    self.name(enum_name),
                    self.name(variant),
                    args.len()
                );
                self.line(text, span);
            }
            Expr::Unary { op, operand } => {
                self.expr(operand);
                self.line(format!("E.Unary {op:?}"), span);
            }
            Expr::Binary { op, lhs, rhs } => {
                self.expr(lhs);
                self.expr(rhs);
                self.line(format!("E.Binary {op:?}"), span);
            }
            Expr::Call { callee, qualifier, args } => {
                for a in &args {
                    self.expr(*a);
                }
                let text =
                    format!("E.Call {} {} {}", self.qual(qualifier), self.name(callee), args.len());
                self.line(text, span);
            }
            Expr::Index { base, index } => {
                self.expr(base);
                self.expr(index);
                self.line("E.Index".to_owned(), span);
            }
            Expr::Slice { base, start, end } => {
                self.expr(base);
                self.expr(start);
                self.expr(end);
                self.line("E.Slice".to_owned(), span);
            }
            Expr::Alloc { region, value } => {
                self.expr(value);
                let text = format!("E.Alloc {}", self.name(region));
                self.line(text, span);
            }
            Expr::AllocSlice { region, count, fill } => {
                self.expr(count);
                self.expr(fill);
                let text = format!("E.AllocSlice {}", self.name(region));
                self.line(text, span);
            }
        }
    }

    fn block(&mut self, block: &Block) -> usize {
        for s in &block.stmts {
            self.stmt(*s);
        }
        block.stmts.len()
    }

    fn pattern(&self, p: &Pattern) -> String {
        match p {
            Pattern::Wildcard => "_".to_owned(),
            Pattern::Variant { enum_name, qualifier, variant, bindings } => {
                let parts: Vec<&str> =
                    bindings.iter().map(|b| b.map_or("_", |s| self.name(s))).collect();
                format!(
                    "{} {} {} [{}]",
                    self.qual(*qualifier),
                    self.name(*enum_name),
                    self.name(*variant),
                    parts.join(",")
                )
            }
        }
    }

    fn stmt(&mut self, id: StmtId) {
        let span = self.ast.stmt_span(id);
        match self.ast.stmt(id).clone() {
            Stmt::Let { name, mutable, ty, value } => {
                if let Some(t) = ty {
                    self.ty(t);
                }
                self.expr(value);
                let text = format!(
                    "S.Let {} {} {}",
                    u8::from(mutable),
                    self.name(name),
                    u8::from(ty.is_some())
                );
                self.line(text, span);
            }
            Stmt::Assign { place, value } => {
                self.expr(place);
                self.expr(value);
                self.line("S.Assign".to_owned(), span);
            }
            Stmt::Destructure { struct_name, qualifier, fields, value } => {
                self.say(format!("H.Names {}", self.list(&fields)));
                self.expr(value);
                let text =
                    format!("S.Destructure {} {}", self.qual(qualifier), self.name(struct_name));
                self.line(text, span);
            }
            Stmt::DestructureTuple { names, value } => {
                self.say(format!("H.Names {}", self.list(&names)));
                self.expr(value);
                self.line("S.DestructureTuple".to_owned(), span);
            }
            Stmt::Borrow { value, unique, region, body } => {
                let n = self.block(&body);
                let text = format!(
                    "S.Borrow {} {} {} {n}",
                    self.name(value),
                    u8::from(unique),
                    self.name(region)
                );
                self.line(text, span);
            }
            Stmt::Region { region, body } => {
                let n = self.block(&body);
                let text = format!("S.Region {} {n}", self.name(region));
                self.line(text, span);
            }
            Stmt::Expr(e) => {
                self.expr(e);
                self.line("S.Expr".to_owned(), span);
            }
            Stmt::If { cond, then_block, else_block } => {
                self.expr(cond);
                let n = self.block(&then_block);
                let m = match &else_block {
                    Some(b) => self.block(b).to_string(),
                    None => "-".to_owned(),
                };
                self.line(format!("S.If {n} {m}"), span);
            }
            Stmt::While { cond, body } => {
                self.expr(cond);
                let n = self.block(&body);
                self.line(format!("S.While {n}"), span);
            }
            Stmt::Match { scrutinee, arms } => {
                self.expr(scrutinee);
                for arm in &arms {
                    self.say(format!("A.Pat {}", self.pattern(&arm.pattern)));
                    let n = self.block(&arm.body);
                    self.say(format!("A.Arm {n}"));
                }
                self.line(format!("S.Match {}", arms.len()), span);
            }
            Stmt::Return(e) => {
                self.expr(e);
                self.line("S.Return".to_owned(), span);
            }
            Stmt::Defer(e) => {
                self.expr(e);
                self.line("S.Defer".to_owned(), span);
            }
        }
    }

    fn mode(m: Option<Mode>) -> &'static str {
        match m {
            Some(Mode::Val) => "val",
            Some(Mode::Res) => "res",
            None => "-",
        }
    }

    fn item(&mut self, id: ItemId) {
        let span = self.ast.item_span(id);
        let module = &self.ast.module(self.ast.module_of(id)).path;
        let module = if module.is_empty() {
            "-".to_owned()
        } else {
            module.iter().map(|s| self.name(*s)).collect::<Vec<_>>().join(".")
        };
        let place = format!("mod={module} ed={}", self.ast.edition_of(id));
        match self.ast.item(id).clone() {
            Item::Fn(f) => {
                self.generics_line(&f.generics, &f.bounds, &f.regions, &f.outlives);
                for p in &f.params {
                    self.say(format!("H.Param {}", self.name(p.name)));
                    self.ty(p.ty);
                }
                self.effects_line(&f.effects);
                self.ty(f.ret);
                let n = self.block(&f.body);
                let text =
                    format!("I.Fn {} {} {n} {place}", self.name(f.name), u8::from(f.public),);
                self.line(text, span);
            }
            Item::Extern(e) => {
                self.generics_line(&[], &[], &e.regions, &[]);
                for p in &e.params {
                    self.say(format!("H.Param {}", self.name(p.name)));
                    self.ty(p.ty);
                }
                self.effects_line(&e.effects);
                self.ty(e.ret);
                let text = format!("I.Extern {} {} {place}", self.name(e.name), hex(&e.symbol));
                self.line(text, span);
            }
            Item::Struct(s) => {
                self.generics_line(&s.generics, &s.bounds, &[], &[]);
                for f in &s.fields {
                    self.say(format!("H.Field {}", self.name(f.name)));
                    self.ty(f.ty);
                }
                let text = format!(
                    "I.Struct {} {} {} {place}",
                    self.name(s.name),
                    u8::from(s.public),
                    Self::mode(s.mode),
                );
                self.line(text, span);
            }
            Item::Enum(e) => {
                self.generics_line(&e.generics, &e.bounds, &[], &[]);
                for v in &e.variants {
                    for t in &v.payload {
                        self.ty(*t);
                    }
                    self.say(format!("V.Variant {} {}", self.name(v.name), v.payload.len()));
                }
                let text = format!(
                    "I.Enum {} {} {} {place}",
                    self.name(e.name),
                    u8::from(e.public),
                    Self::mode(e.mode),
                );
                self.line(text, span);
            }
            Item::Static(s) => {
                self.ty(s.ty);
                let n = self.block(&s.body);
                let text =
                    format!("I.Static {} {} {n} {place}", self.name(s.name), u8::from(s.public));
                self.line(text, span);
            }
        }
    }

    fn imports(&mut self) {
        for module in &self.ast.modules {
            let at = if module.path.is_empty() {
                "-".to_owned()
            } else {
                module.path.iter().map(|s| self.name(*s)).collect::<Vec<_>>().join(".")
            };
            for import in &module.imports {
                let path: Vec<&str> = import.path.iter().map(|s| self.name(*s)).collect();
                let line = format!(
                    "M.Import {at} {} {} @ {} {}\n",
                    path.join("."),
                    self.name(import.alias),
                    import.span.start,
                    import.span.end
                );
                self.out.push_str(&line);
            }
        }
    }
}

/// The listing for a program of several files, parsed one after another into one AST as the
/// compiler does (`parse_into`, each file at its own base offset in a `SourceMap`), or the one
/// refusal line. The conformance test (`crates/cancho/tests/conformance/selfhost.rs`) includes
/// this file and calls this.
pub fn listing_files(files: &[String]) -> String {
    let mut map = SourceMap::new();
    let mut ast = Ast::new();
    let bases: Vec<u32> = files.iter().map(|text| map.add("file", text.clone())).collect();
    for (text, base) in files.iter().zip(bases) {
        if let Err(d) = parse_into(&mut ast, text, base) {
            return format!("ERR {} {} {}\n", d.rule.tag(), d.span.start, d.span.end);
        }
    }
    let mut dump = Dump { ast: &ast, out: String::new() };
    for i in 0..ast.items.len() {
        dump.item(ItemId(i as u32));
    }
    dump.imports();
    dump.out
}

/// The listing for `source`, or the one refusal line.
pub fn listing(source: &str) -> String {
    listing_files(&[source.to_owned()])
}

/// Split a stream of files, each a line `FILE <length>` and then that many bytes, into the
/// files. The programs of `examples/selfhost` read their input the same way.
pub fn read_stream(bytes: &[u8]) -> Vec<String> {
    let mut files = Vec::new();
    let mut at = 0;
    while bytes[at..].starts_with(b"FILE ") {
        let line_end = at + bytes[at..].iter().position(|b| *b == b'\n').expect("a header line");
        let length: usize = std::str::from_utf8(&bytes[at + 5..line_end])
            .expect("a header")
            .parse()
            .expect("a length");
        let text = &bytes[line_end + 1..line_end + 1 + length];
        files.push(String::from_utf8_lossy(text).into_owned());
        at = line_end + 1 + length;
    }
    files
}

#[allow(dead_code)]
fn main() {
    let mut source = Vec::new();
    std::io::stdin().read_to_end(&mut source).expect("stdin");
    if std::env::args().any(|a| a == "--files") {
        print!("{}", listing_files(&read_stream(&source)));
    } else {
        print!("{}", listing(&String::from_utf8_lossy(&source)));
    }
}
