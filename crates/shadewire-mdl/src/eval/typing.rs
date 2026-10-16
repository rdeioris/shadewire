//! Types as written resolved to MDL types, struct fields, and the value a type constructs by default.

use std::rc::Rc;

use shadewire::{Error, Value};

use super::{Eval, Flow, Frame};
use crate::ast::{ArraySize, DeclKind, QualifiedName, TypeRef};
use crate::program::Entity;
use crate::types::{Len, Scalar, StructId, Ty};
use crate::value::{Df, Gamma, Num, Texture, V, Val};

impl<'p, 'r, 'a> Eval<'p, 'r, 'a> {
    /// The type `t` names in frame `f` (`auto` is `None`).
    pub fn resolve_type(&mut self, f: &mut Frame, t: &TypeRef, line: u32) -> Result<Option<Ty>, Error> {
        let Some(name) = &t.name else { return Ok(None) };
        let base = self.named_type(f.module, name, line)?;
        let ty = match &t.array {
            None => base,
            Some(ArraySize::Unsized) => Ty::Array(Box::new(base), Len::Deferred(String::new())),
            Some(ArraySize::Deferred(n)) => match f.sizes.get(n) {
                Some(&size) => Ty::Array(Box::new(base), Len::Fixed(size)),
                None => Ty::Array(Box::new(base), Len::Deferred(n.clone())),
            },
            Some(ArraySize::Sized(e)) => {
                let mut flow = Flow::new();
                let size = self.expr(f, &mut flow, e)?;
                let Some(n) = size.known_int() else {
                    return Err(self.error(f, line, "an array size must be a constant integer"));
                };
                Ty::Array(Box::new(base), Len::Fixed(n.max(0) as usize))
            }
        };
        Ok(Some(ty))
    }

    /// The type a name refers to in `module`: a reserved type word, a struct, an enum or a typedef.
    pub fn named_type(&mut self, module: usize, name: &QualifiedName, line: u32) -> Result<Ty, Error> {
        if name.is_simple()
            && let Some(ty) = Ty::builtin(name.last())
        {
            return Ok(ty);
        }
        for entity in self.program.lookup(module, name) {
            match entity {
                Entity::Struct(id) => return Ok(Ty::Struct(id)),
                Entity::Enum(id) => return Ok(Ty::Enum(id)),
                Entity::Typedef { module: m, decl } => {
                    let DeclKind::Typedef { ty, .. } = &self.program.modules[m].ast.decls[decl].kind else {
                        unreachable!("a typedef entity names a typedef")
                    };
                    let ty = ty.clone();
                    let mut frame = Frame::new(m, "typedef");
                    let resolved = self.resolve_type(&mut frame, &ty, line)?;
                    return resolved.ok_or_else(|| self.error(&frame, line, "a typedef of auto"));
                }
                _ => {}
            }
        }
        Err(Error::BadValue { at: self.at(module, line, &name.to_string()), message: format!("no type {name}") })
    }

    /// The field types of struct `id`, resolved once.
    pub fn field_types(&mut self, id: StructId) -> Result<Rc<Vec<Ty>>, Error> {
        if let Some(found) = self.field_types.get(&id) {
            return Ok(found.clone());
        }
        let module = self.program.structs[id].module;
        let decl = self.program.struct_decl(id);
        let refs: Vec<_> = decl.fields.iter().map(|f| f.ty.clone()).collect();
        let mut frame = Frame::new(module, &decl.name);
        let mut types = Vec::new();
        for t in &refs {
            let ty = self.resolve_type(&mut frame, t, 0)?.ok_or_else(|| self.error(&frame, 0, "an auto field"))?;
            types.push(ty);
        }
        let types = Rc::new(types);
        self.field_types.insert(id, types.clone());
        Ok(types)
    }

    /// The index of field `name` of struct `id`.
    pub fn field_index(&self, id: StructId, name: &str) -> Option<usize> {
        self.program.struct_decl(id).fields.iter().position(|f| f.name == name)
    }

    /// Field `name` of the struct value `v`.
    pub fn field<'v>(&self, v: &'v Val, name: &str) -> Option<&'v Val> {
        let Ty::Struct(id) = v.ty else { return None };
        let index = self.field_index(id, name)?;
        v.fields().and_then(|f| f.get(index))
    }

    /// The value `ty()` constructs: zero, false, the empty string, an enum's first value, an invalid texture, a
    /// black distribution function, a struct of its field defaults.
    pub fn default_value(&mut self, ty: &Ty, line: u32) -> Result<Val, Error> {
        let zero = |ir: shadewire::Type| Val::known(ty.clone(), Value::zero(ir));
        Ok(match ty {
            Ty::Scalar(Scalar::Bool) => Val::bool(false),
            Ty::Scalar(Scalar::Int) => Val::int(0),
            Ty::Scalar(_) => Val::known(ty.clone(), Value::Float(0.0)),
            Ty::Vector(s, n) if s.is_floating() => zero(ty.ir().expect("a float vector")).with_count(*n),
            Ty::Vector(s, n) => {
                let part = if *s == Scalar::Bool { Value::Boolean(false) } else { Value::Integer(0) };
                Val::new(ty.clone(), V::Parts(vec![Num::Known(part); *n as usize]))
            }
            Ty::Color => zero(shadewire::Type::Color3),
            Ty::Matrix(_, c, r) => {
                let column = Ty::float_vector(*r).ir().expect("a column");
                Val::new(ty.clone(), V::Columns(vec![Num::Known(Value::zero(column)); *c as usize]))
            }
            Ty::String => Val::string(""),
            Ty::Texture(shape) => {
                Val::new(ty.clone(), V::Texture(Texture { shape: *shape, file: String::new(), gamma: Gamma::Default }))
            }
            Ty::LightProfile | Ty::BsdfMeasurement => Val::new(ty.clone(), V::Resource(String::new())),
            Ty::Bsdf | Ty::Edf | Ty::Vdf | Ty::HairBsdf => Val::df(ty.clone(), Df::Default),
            Ty::Enum(id) => Val::new(ty.clone(), V::Enum(self.program.enums[*id].values.first().map_or(0, |v| v.1))),
            Ty::Struct(id) => self.construct_struct(*id, Vec::new(), line)?,
            Ty::Array(e, Len::Fixed(n)) => {
                let element = self.default_value(e, line)?;
                Val::new(ty.clone(), V::Array(vec![element; *n]))
            }
            Ty::Array(_, Len::Deferred(_)) => Val::new(ty.clone(), V::Array(Vec::new())),
        })
    }

    /// Struct `id` from `given` field values (by index); the others take their defaults, evaluated in the
    /// struct's module.
    pub fn construct_struct(&mut self, id: StructId, given: Vec<(usize, Val)>, line: u32) -> Result<Val, Error> {
        let types = self.field_types(id)?;
        let module = self.program.structs[id].module;
        let decl = self.program.struct_decl(id);
        let defaults: Vec<_> = decl.fields.iter().map(|f| f.default.clone()).collect();
        let name = decl.name.clone();
        let mut fields = Vec::with_capacity(types.len());
        for (i, ty) in types.iter().enumerate() {
            let value = match given.iter().find(|(g, _)| *g == i) {
                Some((_, v)) => self.convert(v.clone(), ty, false, line, module)?,
                None => match &defaults[i] {
                    Some(e) => {
                        let mut frame = Frame::new(module, &name);
                        let mut flow = Flow::new();
                        let v = self.expr(&mut frame, &mut flow, e)?;
                        self.convert(v, ty, false, line, module)?
                    }
                    None => self.default_value(ty, line)?,
                },
            };
            fields.push(value);
        }
        Ok(Val::new(Ty::Struct(id), V::Struct(fields)))
    }
}

impl Val {
    /// A known float vector value relabelled to `n` components: `zero` builds by IR type, which for vectors is the
    /// right width already.
    fn with_count(self, _n: u8) -> Val {
        self
    }
}
