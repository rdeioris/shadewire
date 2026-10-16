//! MDL source to [`ast`](crate::ast): the pest grammar in `mdl.pest`, then a walk over its pairs.

use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use pest::Parser;
use pest::iterators::{Pair, Pairs};
use pest::pratt_parser::{Assoc, Op, PrattParser};
use shadewire::{Error, Location};

use crate::ast::*;

#[derive(pest_derive::Parser)]
#[grammar = "mdl.pest"]
struct MdlParser;

type P<'i> = Pair<'i, Rule>;

/// How deep statements and expressions may nest (each operand of an operator chain, each prefix and postfix counts as
/// a level): what is built, evaluated and dropped recursively stays well inside the reader's stack.
pub const MAX_NESTING: usize = 2048;

thread_local! {
    /// The byte offset each line of the text being built starts at: pest's own `line_col` scans from the start.
    static LINE_STARTS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    /// How deep the walk is now.
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    /// The line where the walk first went past `MAX_NESTING`.
    static TOO_DEEP: Cell<Option<u32>> = const { Cell::new(None) };
}

/// Runs `build` with the line table of `text` in place and the nesting count at zero; `Error::Limit` naming `file`
/// when the text nests past `MAX_NESTING`.
fn walk<T>(text: &str, file: Option<&str>, build: impl FnOnce() -> T) -> Result<T, Error> {
    let starts = std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1)).collect();
    let previous = LINE_STARTS.with(|l| l.replace(starts));
    let (depth, deep) = (DEPTH.replace(0), TOO_DEEP.replace(None));
    let out = build();
    LINE_STARTS.with(|l| l.replace(previous));
    let too_deep = TOO_DEEP.replace(deep);
    DEPTH.set(depth);
    match too_deep {
        None => Ok(out),
        Some(line) => {
            drop(out);
            Err(Error::Limit(format!(
                "{}:{line}: statements and expressions nested more than {MAX_NESTING} deep",
                file.unwrap_or("<module>")
            )))
        }
    }
}

/// `build`'s result `levels` deeper than now; `stub`'s once that is past `MAX_NESTING` (the walk then fails).
fn nested<T>(at: u32, levels: usize, build: impl FnOnce() -> T, stub: impl FnOnce() -> T) -> T {
    let depth = DEPTH.get() + levels;
    if depth > MAX_NESTING {
        if TOO_DEEP.get().is_none() {
            TOO_DEEP.set(Some(at));
        }
        return stub();
    }
    DEPTH.set(depth);
    let out = build();
    DEPTH.set(depth - levels);
    out
}

fn stub_expr(at: u32) -> Expr {
    Expr::new(ExprKind::Literal(Literal::Int(0)), at)
}

/// Parses a whole module. `file` names it in errors.
pub fn parse_module(text: &str, file: Option<&str>) -> Result<Module, Error> {
    let mut pairs = MdlParser::parse(Rule::module, text).map_err(|e| parse_error(e, file))?;
    let module = pairs.next().expect("a module");
    walk(text, file, || build_module(module))
}

/// Parses one expression, for tests and for values given on the command line.
pub fn parse_expr(text: &str) -> Result<Expr, Error> {
    let mut pairs = MdlParser::parse(Rule::expr, text).map_err(|e| parse_error(e, None))?;
    let pair = pairs.next().expect("an expression");
    if pair.as_span().end() != text.trim_end().len() {
        let at = Location { file: None, line: 1, path: String::new() };
        return Err(Error::Parse { at, message: format!("unexpected input after {:?}", pair.as_str()) });
    }
    walk(text, None, || expr(pair))
}

fn parse_error(e: pest::error::Error<Rule>, file: Option<&str>) -> Error {
    let line = match e.line_col {
        pest::error::LineColLocation::Pos((l, _)) | pest::error::LineColLocation::Span((l, _), _) => l as u32,
    };
    let col = match e.line_col {
        pest::error::LineColLocation::Pos((_, c)) | pest::error::LineColLocation::Span((_, c), _) => c,
    };
    let expected = match &e.variant {
        pest::error::ErrorVariant::ParsingError { positives, .. } => {
            let names: Vec<String> = positives.iter().take(6).map(|r| format!("{r:?}")).collect();
            format!("expected {}", names.join(", "))
        }
        pest::error::ErrorVariant::CustomError { message } => message.clone(),
    };
    if matches!(&e.variant, pest::error::ErrorVariant::CustomError { message } if message.contains("limit")) {
        // pest's own guard on its recursion: the text nests too deep to parse.
        return Error::Limit(format!(
            "{}:{line}: column {col}: nested too deep to parse ({expected})",
            file.unwrap_or("<module>")
        ));
    }
    let at = Location { file: file.map(str::to_string), line, path: String::new() };
    Error::Parse { at, message: format!("column {col}: {expected}") }
}

fn line(p: &P) -> u32 {
    let offset = p.as_span().start();
    LINE_STARTS.with(|l| l.borrow().partition_point(|&start| start <= offset)) as u32
}

fn build_module(p: P) -> Module {
    let mut module = Module { version: (1, 0), imports: Vec::new(), annotations: Vec::new(), decls: Vec::new() };
    for item in p.into_inner() {
        match item.as_rule() {
            Rule::mdl_version => {
                let text = item.into_inner().find(|i| i.as_rule() == Rule::float_lit).expect("a version").as_str();
                let text = text.trim_end_matches(['f', 'F', 'd', 'D']);
                let mut parts = text.split('.');
                let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
                let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                module.version = (major, minor);
            }
            Rule::import_decl => module.imports.extend(imports_of(item)),
            Rule::module_annotation => {
                let block = item.into_inner().find(|i| i.as_rule() == Rule::annotation_block);
                module.annotations = block.map(annotations).unwrap_or_default();
            }
            Rule::global_decl => module.decls.extend(global(item)),
            Rule::EOI => {}
            other => unreachable!("module item {other:?}"),
        }
    }
    module
}

fn import(p: P) -> Import {
    let inner = p.into_inner().next().expect("an import");
    let at = line(&inner);
    match inner.as_rule() {
        Rule::import_stmt => {
            let mut qualified = inner.into_inner().filter(|i| i.as_rule() == Rule::qualified_import);
            qualified_import(qualified.next().expect("a qualified import"), at)
        }
        Rule::using_stmt => {
            let mut export = false;
            let mut path = None;
            let mut names = Some(Vec::new());
            for i in inner.into_inner() {
                match i.as_rule() {
                    Rule::export => export = true,
                    Rule::import_path => path = Some(import_path(i)),
                    Rule::star => names = None,
                    Rule::ident => {
                        if let Some(names) = names.as_mut() {
                            names.push(i.as_str().to_string());
                        }
                    }
                    _ => {}
                }
            }
            Import::Using { export, path: path.expect("a using path"), names, line: at }
        }
        Rule::using_alias => {
            let mut name = String::new();
            let mut target = ImportPath { prefix: Prefix::None, parts: Vec::new() };
            for i in inner.into_inner() {
                match i.as_rule() {
                    Rule::ident => name = i.as_str().to_string(),
                    Rule::string_lit => target.parts.push(unescape(&i.as_str()[1..i.as_str().len() - 1])),
                    Rule::import_path => target = import_path(i),
                    _ => {}
                }
            }
            Import::Alias { name, target, line: at }
        }
        other => unreachable!("import {other:?}"),
    }
}

/// Every import of an `import_decl`: `import a::*, b::c;` is two.
pub(crate) fn imports_of(p: P) -> Vec<Import> {
    let inner = p.clone().into_inner().next().expect("an import");
    if inner.as_rule() == Rule::import_stmt {
        let at = line(&inner);
        inner.into_inner().filter(|i| i.as_rule() == Rule::qualified_import).map(|q| qualified_import(q, at)).collect()
    } else {
        vec![import(p)]
    }
}

fn prefix(p: P) -> Prefix {
    match p.as_str() {
        "::" => Prefix::Absolute,
        ".::" => Prefix::Current,
        s => Prefix::Parent((s.len() / 4) as u32),
    }
}

fn path_part(p: &P) -> String {
    let s = p.as_str();
    if s.starts_with('\'') || s.starts_with('"') { unescape(&s[1..s.len() - 1]) } else { s.to_string() }
}

fn import_path(p: P) -> ImportPath {
    let mut path = ImportPath { prefix: Prefix::None, parts: Vec::new() };
    for i in p.into_inner() {
        match i.as_rule() {
            Rule::import_prefix => path.prefix = prefix(i),
            _ => path.parts.push(path_part(&i)),
        }
    }
    path
}

fn qualified_import(p: P, at: u32) -> Import {
    let mut path = ImportPath { prefix: Prefix::None, parts: Vec::new() };
    let mut name = None;
    let items: Vec<P> = p.into_inner().collect();
    let last = items.len() - 1;
    for (n, i) in items.into_iter().enumerate() {
        match i.as_rule() {
            Rule::import_prefix => path.prefix = prefix(i),
            Rule::star => name = None,
            _ if n == last => name = Some(path_part(&i)),
            _ => path.parts.push(path_part(&i)),
        }
    }
    Import::Qualified { path, name, line: at }
}

/// A global declaration; an import among them (MDL wants imports first, files do not always) may be several.
fn global(p: P) -> Vec<Decl> {
    let at = line(&p);
    let mut export = false;
    for i in p.into_inner() {
        match i.as_rule() {
            Rule::export => export = true,
            Rule::import_decl => {
                return imports_of(i)
                    .into_iter()
                    .map(|import| {
                        let export = matches!(import, Import::Using { export: true, .. });
                        Decl { export, kind: DeclKind::Import(import), line: at }
                    })
                    .collect();
            }
            _ => return vec![Decl { export, kind: decl_kind(i), line: at }],
        }
    }
    unreachable!("an empty declaration")
}

fn decl_kind(p: P) -> DeclKind {
    match p.as_rule() {
        Rule::annotation_decl => {
            let mut name = String::new();
            let mut params_ = Vec::new();
            for i in p.into_inner() {
                match i.as_rule() {
                    Rule::ident => name = i.as_str().to_string(),
                    Rule::param_list => params_ = params(i),
                    _ => {}
                }
            }
            DeclKind::Annotation { name, params: params_ }
        }
        Rule::struct_category_decl => {
            let name = p.into_inner().find(|i| i.as_rule() == Rule::ident).expect("a name").as_str().to_string();
            DeclKind::StructCategory { name }
        }
        Rule::struct_decl => DeclKind::Struct(structure(p)),
        Rule::enum_decl => DeclKind::Enum(enumeration(p)),
        Rule::typedef_decl => {
            let mut inner = p.into_inner().filter(|i| i.as_rule() != Rule::kw_typedef);
            let ty = type_ref(inner.next().expect("a type"));
            let name = inner.next().expect("a name").as_str().to_string();
            DeclKind::Typedef { ty, name }
        }
        Rule::const_decl => {
            let mut ty = None;
            let mut declarators = Vec::new();
            for i in p.into_inner() {
                match i.as_rule() {
                    Rule::type_ => ty = Some(type_ref(i)),
                    Rule::const_declarator => declarators.push(declarator(i)),
                    _ => {}
                }
            }
            DeclKind::Const { ty: ty.expect("a type"), declarators }
        }
        Rule::function_decl => DeclKind::Function(function(p)),
        other => unreachable!("declaration {other:?}"),
    }
}

fn structure(p: P) -> Struct {
    let mut s =
        Struct { declarative: false, name: String::new(), category: None, annotations: Vec::new(), fields: Vec::new() };
    for i in p.into_inner() {
        match i.as_rule() {
            Rule::declarative => s.declarative = true,
            Rule::ident => s.name = i.as_str().to_string(),
            Rule::qualified_name => s.category = Some(qualified_name(i)),
            Rule::annotation_block => s.annotations = annotations(i),
            Rule::field_decl => {
                let mut f = Field {
                    ty: TypeRef { frequency: None, name: None, array: None },
                    name: String::new(),
                    default: None,
                    annotations: Vec::new(),
                };
                for j in i.into_inner() {
                    match j.as_rule() {
                        Rule::type_ => f.ty = type_ref(j),
                        Rule::ident => f.name = j.as_str().to_string(),
                        Rule::assign_expr => f.default = Some(expr(j)),
                        Rule::annotation_block => f.annotations = annotations(j),
                        _ => {}
                    }
                }
                s.fields.push(f);
            }
            _ => {}
        }
    }
    s
}

fn enumeration(p: P) -> Enum {
    let mut e = Enum { name: String::new(), annotations: Vec::new(), values: Vec::new() };
    for i in p.into_inner() {
        match i.as_rule() {
            Rule::ident => e.name = i.as_str().to_string(),
            Rule::annotation_block => e.annotations = annotations(i),
            Rule::enum_value => {
                let mut v = EnumValue { name: String::new(), value: None };
                for j in i.into_inner() {
                    match j.as_rule() {
                        Rule::ident => v.name = j.as_str().to_string(),
                        Rule::assign_expr => v.value = Some(expr(j)),
                        _ => {}
                    }
                }
                e.values.push(v);
            }
            _ => {}
        }
    }
    e
}

fn declarator(p: P) -> Declarator {
    let at = line(&p);
    let mut d = Declarator { name: String::new(), init: Init::None, annotations: Vec::new(), line: at };
    for i in p.into_inner() {
        match i.as_rule() {
            Rule::ident => d.name = i.as_str().to_string(),
            Rule::arg_list => d.init = Init::Args(args(i)),
            Rule::cond_expr | Rule::assign_expr => d.init = Init::Value(expr(i)),
            Rule::annotation_block => d.annotations = annotations(i),
            _ => {}
        }
    }
    d
}

fn function(p: P) -> Function {
    let at = line(&p);
    let mut f = Function {
        declarative: false,
        ret: TypeRef { frequency: None, name: None, array: None },
        name: String::new(),
        params: Some(Vec::new()),
        frequency: None,
        annotations: Vec::new(),
        body: Body::None,
        line: at,
    };
    let mut seen_name = false;
    for i in p.into_inner() {
        match i.as_rule() {
            Rule::declarative => f.declarative = true,
            Rule::type_ => f.ret = type_ref(i),
            // An annotation block before the name annotates the return type; MDL gives it no meaning.
            Rule::annotation_block if !seen_name => {}
            Rule::annotation_block => f.annotations = annotations(i),
            Rule::ident => {
                f.name = i.as_str().to_string();
                seen_name = true;
            }
            Rule::variant_params => f.params = None,
            Rule::param_list => f.params = Some(params(i)),
            Rule::frequency => f.frequency = Some(frequency(i)),
            Rule::compound_stmt => f.body = Body::Block(block(i)),
            Rule::expr => f.body = Body::Expr(expr(i)),
            _ => {}
        }
    }
    f
}

fn params(p: P) -> Vec<Param> {
    p.into_inner()
        .filter(|i| i.as_rule() == Rule::param)
        .map(|i| {
            let at = line(&i);
            let mut param = Param {
                ty: TypeRef { frequency: None, name: None, array: None },
                name: String::new(),
                default: None,
                annotations: Vec::new(),
                line: at,
            };
            for j in i.into_inner() {
                match j.as_rule() {
                    Rule::type_ => param.ty = type_ref(j),
                    Rule::ident => param.name = j.as_str().to_string(),
                    Rule::assign_expr => param.default = Some(expr(j)),
                    Rule::annotation_block => param.annotations = annotations(j),
                    _ => {}
                }
            }
            param
        })
        .collect()
}

fn frequency(p: P) -> Frequency {
    if p.as_str() == "uniform" { Frequency::Uniform } else { Frequency::Varying }
}

fn type_ref(p: P) -> TypeRef {
    let mut t = TypeRef { frequency: None, name: None, array: None };
    for i in p.into_inner() {
        match i.as_rule() {
            Rule::frequency => t.frequency = Some(frequency(i)),
            Rule::auto => t.name = None,
            Rule::array_type => {
                let (name, size) = array_type(i);
                t.name = Some(name);
                t.array = size;
            }
            _ => {}
        }
    }
    t
}

fn array_type(p: P) -> (QualifiedName, Option<ArraySize>) {
    let text = p.as_str().to_string();
    let mut name = None;
    let mut size = None;
    for i in p.into_inner() {
        match i.as_rule() {
            Rule::qualified_name => name = Some(qualified_name(i)),
            Rule::array_size => size = Some(array_size(i)),
            _ => {}
        }
    }
    if size.is_none() && text.trim_end().ends_with(']') {
        size = Some(ArraySize::Unsized);
    }
    (name.expect("a type name"), size)
}

fn array_size(p: P) -> ArraySize {
    let inner = p.into_inner().next().expect("a size");
    match inner.as_rule() {
        Rule::deferred_size => {
            ArraySize::Deferred(inner.into_inner().next().expect("a size name").as_str().to_string())
        }
        _ => ArraySize::Sized(Box::new(expr(inner))),
    }
}

fn qualified_name(p: P) -> QualifiedName {
    let mut name = QualifiedName { absolute: false, parts: Vec::new() };
    for i in p.into_inner() {
        match i.as_rule() {
            Rule::absolute => name.absolute = true,
            _ => name.parts.push(path_part(&i)),
        }
    }
    name
}

fn annotations(p: P) -> Vec<Annotation> {
    p.into_inner()
        .filter(|i| i.as_rule() == Rule::annotation)
        .map(|i| {
            let mut inner = i.into_inner();
            let name = qualified_name(inner.next().expect("a name"));
            let args = inner.next().map(args).unwrap_or_default();
            Annotation { name, args }
        })
        .collect()
}

fn args(p: P) -> Vec<Arg> {
    p.into_inner()
        .filter(|i| i.as_rule() == Rule::arg)
        .map(|i| {
            let mut name = None;
            let mut value = None;
            for j in i.into_inner() {
                match j.as_rule() {
                    Rule::ident => name = Some(j.as_str().to_string()),
                    Rule::named_colon => {}
                    _ => value = Some(expr(j)),
                }
            }
            Arg { name, value: value.expect("an argument value") }
        })
        .collect()
}

// ---- statements

fn block(p: P) -> Vec<Stmt> {
    p.into_inner().filter(|i| i.as_rule() == Rule::stmt).map(stmt).collect()
}

fn stmt(p: P) -> Stmt {
    let at = line(&p);
    nested(at, 1, || stmt_here(p, at), || Stmt { kind: StmtKind::Block(Vec::new()), line: at })
}

fn stmt_here(p: P, at: u32) -> Stmt {
    let inner = if p.as_rule() == Rule::stmt { p.into_inner().next().expect("a statement") } else { p };
    let kind = match inner.as_rule() {
        Rule::compound_stmt => StmtKind::Block(block(inner)),
        Rule::if_stmt => {
            let mut cond = None;
            let mut branches = Vec::new();
            for i in inner.into_inner() {
                match i.as_rule() {
                    Rule::expr => cond = Some(expr(i)),
                    Rule::stmt => branches.push(stmt(i)),
                    _ => {}
                }
            }
            let mut branches = branches.into_iter();
            let then = Box::new(branches.next().expect("a then branch"));
            let otherwise = branches.next().map(Box::new);
            StmtKind::If { cond: cond.expect("a condition"), then, otherwise }
        }
        Rule::switch_stmt => {
            let mut selector = None;
            let mut cases = Vec::new();
            for i in inner.into_inner() {
                match i.as_rule() {
                    Rule::expr => selector = Some(expr(i)),
                    Rule::switch_case => {
                        let mut label = None;
                        let mut body = Vec::new();
                        for j in i.into_inner() {
                            match j.as_rule() {
                                Rule::expr => label = Some(expr(j)),
                                Rule::stmt => body.push(stmt(j)),
                                _ => {}
                            }
                        }
                        cases.push((label, body));
                    }
                    _ => {}
                }
            }
            StmtKind::Switch { selector: selector.expect("a selector"), cases }
        }
        Rule::while_stmt => {
            let mut cond = None;
            let mut body = None;
            for i in inner.into_inner() {
                match i.as_rule() {
                    Rule::expr => cond = Some(expr(i)),
                    Rule::stmt => body = Some(stmt(i)),
                    _ => {}
                }
            }
            StmtKind::While { cond: cond.expect("a condition"), body: Box::new(body.expect("a body")) }
        }
        Rule::do_stmt => {
            let mut cond = None;
            let mut body = None;
            for i in inner.into_inner() {
                match i.as_rule() {
                    Rule::expr => cond = Some(expr(i)),
                    Rule::stmt => body = Some(stmt(i)),
                    _ => {}
                }
            }
            StmtKind::DoWhile { body: Box::new(body.expect("a body")), cond: cond.expect("a condition") }
        }
        Rule::for_stmt => {
            let mut init = None;
            let mut cond = None;
            let mut update = None;
            let mut body = None;
            for i in inner.into_inner() {
                match i.as_rule() {
                    Rule::var_decl | Rule::expr_stmt => init = Some(Box::new(stmt(i))),
                    Rule::for_cond => cond = i.into_inner().next().map(expr),
                    Rule::for_update => update = i.into_inner().next().map(expr),
                    Rule::stmt => body = Some(Box::new(stmt(i))),
                    _ => {}
                }
            }
            StmtKind::For { init, cond, update, body: body.expect("a body") }
        }
        Rule::break_stmt => StmtKind::Break,
        Rule::continue_stmt => StmtKind::Continue,
        Rule::return_stmt => StmtKind::Return(inner.into_inner().find(|i| i.as_rule() == Rule::expr).map(expr)),
        Rule::struct_decl | Rule::enum_decl | Rule::typedef_decl | Rule::const_decl => StmtKind::Decl(decl_kind(inner)),
        Rule::var_decl => {
            let mut ty = None;
            let mut declarators = Vec::new();
            for i in inner.into_inner() {
                match i.as_rule() {
                    Rule::type_ => ty = Some(type_ref(i)),
                    Rule::var_declarator => declarators.push(declarator(i)),
                    _ => {}
                }
            }
            StmtKind::Var { ty: ty.expect("a type"), declarators }
        }
        Rule::expr_stmt => StmtKind::Expr(inner.into_inner().next().map(expr)),
        other => unreachable!("statement {other:?}"),
    };
    Stmt { kind, line: at }
}

// ---- expressions

fn pratt() -> &'static PrattParser<Rule> {
    static PRATT: OnceLock<PrattParser<Rule>> = OnceLock::new();
    PRATT.get_or_init(|| {
        PrattParser::new()
            .op(Op::infix(Rule::op_lor, Assoc::Left))
            .op(Op::infix(Rule::op_land, Assoc::Left))
            .op(Op::infix(Rule::op_bor, Assoc::Left))
            .op(Op::infix(Rule::op_bxor, Assoc::Left))
            .op(Op::infix(Rule::op_band, Assoc::Left))
            .op(Op::infix(Rule::op_eq, Assoc::Left) | Op::infix(Rule::op_ne, Assoc::Left))
            .op(Op::infix(Rule::op_lt, Assoc::Left)
                | Op::infix(Rule::op_le, Assoc::Left)
                | Op::infix(Rule::op_gt, Assoc::Left)
                | Op::infix(Rule::op_ge, Assoc::Left))
            .op(Op::infix(Rule::op_shl, Assoc::Left)
                | Op::infix(Rule::op_shr, Assoc::Left)
                | Op::infix(Rule::op_shru, Assoc::Left))
            .op(Op::infix(Rule::op_add, Assoc::Left) | Op::infix(Rule::op_sub, Assoc::Left))
            .op(Op::infix(Rule::op_mul, Assoc::Left)
                | Op::infix(Rule::op_div, Assoc::Left)
                | Op::infix(Rule::op_rem, Assoc::Left))
    })
}

fn bin_op(rule: Rule) -> BinOp {
    match rule {
        Rule::op_lor => BinOp::LogicalOr,
        Rule::op_land => BinOp::LogicalAnd,
        Rule::op_bor => BinOp::BitOr,
        Rule::op_bxor => BinOp::BitXor,
        Rule::op_band => BinOp::BitAnd,
        Rule::op_eq => BinOp::Eq,
        Rule::op_ne => BinOp::Ne,
        Rule::op_lt => BinOp::Lt,
        Rule::op_le => BinOp::Le,
        Rule::op_gt => BinOp::Gt,
        Rule::op_ge => BinOp::Ge,
        Rule::op_shl => BinOp::Shl,
        Rule::op_shr => BinOp::Shr,
        Rule::op_shru => BinOp::ShrUnsigned,
        Rule::op_add => BinOp::Add,
        Rule::op_sub => BinOp::Sub,
        Rule::op_mul => BinOp::Mul,
        Rule::op_div => BinOp::Div,
        Rule::op_rem => BinOp::Rem,
        other => unreachable!("operator {other:?}"),
    }
}

/// An operator chain: as deep as it has operators.
fn binary(pairs: Pairs<Rule>, at: u32) -> Expr {
    let operators = pairs.len() / 2;
    let build = || {
        pratt()
            .map_primary(expr)
            .map_infix(|lhs, op, rhs| {
                let at = lhs.line;
                Expr::new(ExprKind::Binary { op: bin_op(op.as_rule()), lhs: Box::new(lhs), rhs: Box::new(rhs) }, at)
            })
            .parse(pairs)
    };
    nested(at, operators, build, || stub_expr(at))
}

fn assign_op(text: &str) -> Option<BinOp> {
    Some(match text {
        "=" => return None,
        "*=" => BinOp::Mul,
        "/=" => BinOp::Div,
        "%=" => BinOp::Rem,
        "+=" => BinOp::Add,
        "-=" => BinOp::Sub,
        "<<=" => BinOp::Shl,
        ">>=" => BinOp::Shr,
        ">>>=" => BinOp::ShrUnsigned,
        "&=" => BinOp::BitAnd,
        "^=" => BinOp::BitXor,
        "|=" => BinOp::BitOr,
        other => unreachable!("assignment {other}"),
    })
}

fn expr(p: P) -> Expr {
    let at = line(&p);
    nested(at, 1, || expr_here(p, at), || stub_expr(at))
}

fn expr_here(p: P, at: u32) -> Expr {
    match p.as_rule() {
        Rule::expr => {
            let mut items: Vec<Expr> = p.into_inner().map(expr).collect();
            if items.len() == 1 { items.pop().expect("one") } else { Expr::new(ExprKind::Sequence(items), at) }
        }
        Rule::assign_expr => {
            let items: Vec<P> = p.into_inner().collect();
            if items.len() == 1 {
                return expr(items.into_iter().next().expect("one"));
            }
            let mut items = items.into_iter();
            let target = expr(items.next().expect("a target"));
            let op = assign_op(items.next().expect("an operator").as_str());
            let value = expr(items.next().expect("a value"));
            Expr::new(ExprKind::Assign { op, target: Box::new(target), value: Box::new(value) }, at)
        }
        Rule::cond_expr => {
            let items: Vec<P> = p.into_inner().collect();
            if items.len() == 1 {
                return expr(items.into_iter().next().expect("one"));
            }
            let mut items = items.into_iter().map(expr);
            let cond = items.next().expect("a condition");
            let then = items.next().expect("a then value");
            let otherwise = items.next().expect("an else value");
            Expr::new(
                ExprKind::Ternary { cond: Box::new(cond), then: Box::new(then), otherwise: Box::new(otherwise) },
                at,
            )
        }
        Rule::binary_expr => {
            let inner = p.into_inner();
            if inner.len() == 1 { expr(inner.peek().expect("one")) } else { binary(inner, at) }
        }
        Rule::unary_expr => {
            let items: Vec<P> = p.into_inner().collect();
            let (prefixes, last) = items.split_at(items.len() - 1);
            let operand = last[0].clone();
            let mut value = nested(at, prefixes.len(), || expr(operand), || stub_expr(at));
            for op in prefixes.iter().rev() {
                let op = match op.as_str() {
                    "-" => UnaryOp::Neg,
                    "+" => UnaryOp::Plus,
                    "!" => UnaryOp::Not,
                    "~" => UnaryOp::BitNot,
                    "++" => UnaryOp::PreInc,
                    "--" => UnaryOp::PreDec,
                    other => unreachable!("prefix {other}"),
                };
                value = Expr::new(ExprKind::Unary { op, arg: Box::new(value) }, at);
            }
            value
        }
        Rule::let_expr => {
            let mut decls = Vec::new();
            let mut body = None;
            for i in p.into_inner() {
                match i.as_rule() {
                    Rule::var_decl => decls.push(stmt(i)),
                    Rule::assign_expr => body = Some(expr(i)),
                    _ => {}
                }
            }
            Expr::new(ExprKind::Let { decls, body: Box::new(body.expect("a let body")) }, at)
        }
        Rule::postfix_expr => {
            let mut inner = p.into_inner();
            let primary = inner.next().expect("a primary");
            let mut value = nested(at, inner.len(), || expr(primary), || stub_expr(at));
            for post in inner {
                let post = post.into_inner().next().expect("a postfix");
                let at = line(&post);
                value = match post.as_rule() {
                    Rule::inc => Expr::new(ExprKind::PostInc(Box::new(value)), at),
                    Rule::dec => Expr::new(ExprKind::PostDec(Box::new(value)), at),
                    Rule::member => {
                        let name = post.into_inner().next().expect("a member").as_str().to_string();
                        Expr::new(ExprKind::Member { base: Box::new(value), name }, at)
                    }
                    Rule::arg_list => Expr::new(ExprKind::Call { callee: Box::new(value), args: args(post) }, at),
                    Rule::index => {
                        let index = expr(post.into_inner().next().expect("an index"));
                        Expr::new(ExprKind::Index { base: Box::new(value), index: Box::new(index) }, at)
                    }
                    other => unreachable!("postfix {other:?}"),
                };
            }
            value
        }
        Rule::primary => expr(p.into_inner().next().expect("a primary")),
        Rule::paren => expr(p.into_inner().next().expect("an expression")),
        Rule::literal => Expr::new(ExprKind::Literal(literal(p)), at),
        Rule::qualified_name => Expr::new(ExprKind::Name(qualified_name(p)), at),
        Rule::array_ctor => {
            let mut elem = None;
            let mut size = None;
            for i in p.into_inner() {
                match i.as_rule() {
                    Rule::qualified_name => elem = Some(qualified_name(i)),
                    Rule::array_size => size = Some(array_size(i)),
                    _ => {}
                }
            }
            Expr::new(ExprKind::ArrayType { elem: elem.expect("an element type"), size }, at)
        }
        Rule::cast_expr => {
            let mut ty = None;
            let mut arg = None;
            for i in p.into_inner() {
                match i.as_rule() {
                    Rule::array_type => {
                        let (name, array) = array_type(i);
                        ty = Some(TypeRef { frequency: None, name: Some(name), array });
                    }
                    Rule::unary_expr => arg = Some(expr(i)),
                    _ => {}
                }
            }
            Expr::new(ExprKind::Cast { ty: ty.expect("a type"), arg: Box::new(arg.expect("an argument")) }, at)
        }
        other => unreachable!("expression {other:?}"),
    }
}

fn literal(p: P) -> Literal {
    let inner: Vec<P> = p.into_inner().collect();
    let first = &inner[0];
    match first.as_rule() {
        Rule::float_lit => {
            let s = first.as_str();
            let double = s.ends_with(['d', 'D']);
            let digits = s.trim_end_matches(['f', 'F', 'd', 'D']);
            Literal::Float(digits.parse().unwrap_or(0.0), double)
        }
        Rule::int_lit => {
            let s = first.as_str();
            let value = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                i64::from_str_radix(hex, 16).unwrap_or(0)
            } else if s.len() > 1 && s.starts_with('0') {
                i64::from_str_radix(&s[1..], 8).unwrap_or(0)
            } else {
                s.parse().unwrap_or(0)
            };
            Literal::Int(value)
        }
        Rule::bool_lit => Literal::Bool(first.as_str() == "true"),
        Rule::string_lit => {
            Literal::String(inner.iter().map(|s| unescape(&s.as_str()[1..s.as_str().len() - 1])).collect())
        }
        other => unreachable!("literal {other:?}"),
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('a') => out.push('\u{7}'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests;
