//! `::state`: the renderer state as context reads. Internal space is world space.

use shadewire::{ContextRead, Op, Space, TransformKind, Type, Value};

use super::Params;
use crate::eval::Eval;
use crate::types::{Scalar, Ty};
use crate::value::{Num, V, Val};

type R<T> = Result<T, String>;

/// `state::coordinate_space`'s value as a space: internal is world.
fn space(v: &Val) -> R<Space> {
    match v.known_int() {
        Some(1) => Ok(Space::Object),
        Some(0) | Some(2) => Ok(Space::World),
        _ => Err("a coordinate space known only at run time".into()),
    }
}

impl<'p, 'r, 'a> Eval<'p, 'r, 'a> {
    /// The shading normal: the material's `geometry.normal` while its surface is evaluated.
    pub fn shading_normal(&mut self, origin: &str) -> Num {
        match self.normal {
            Some(id) => Num::Node(id),
            None => self.context(ContextRead::Normal(Space::World), origin),
        }
    }

    /// The texture space index `i` reads; spaces other than the first read it, with a warning.
    fn texture_space(&mut self, _name: &str, v: &Val) -> R<u32> {
        let i = v.known_int().ok_or("a texture space known only at run time")?;
        Ok(i.max(0) as u32)
    }

    pub(super) fn state(&mut self, name: &str, p: &Params, ret: &Ty, origin: &str) -> R<Val> {
        let v3 = Ty::Vector(Scalar::Float, 3);
        let vec3 = |n: Num| Val::new(Ty::Vector(Scalar::Float, 3), V::Num(n));
        Ok(match name {
            "position" => vec3(self.context(ContextRead::Position(Space::World), origin)),
            "normal" => vec3(self.shading_normal(origin)),
            "geometry_normal" => vec3(self.context(ContextRead::Normal(Space::World), origin)),
            "texture_coordinate" => {
                let index = self.texture_space(name, p.at(0))?;
                let uv = self.context(ContextRead::TexCoord { index, ty: Type::Vector2 }, origin);
                let u = self.extract(&uv, 0, origin)?;
                let v = self.extract(&uv, 1, origin)?;
                self.from_parts(&v3, vec![u, v, Num::Known(Value::Float(0.0))], origin)?
            }
            "texture_tangent_u" | "geometry_tangent_u" => {
                self.texture_space(name, p.at(0))?;
                vec3(self.context(ContextRead::Tangent(Space::World), origin))
            }
            "texture_tangent_v" | "geometry_tangent_v" => {
                self.texture_space(name, p.at(0))?;
                vec3(self.context(ContextRead::Bitangent(Space::World), origin))
            }
            "tangent_space" => {
                self.texture_space(name, p.at(0))?;
                let t = self.context(ContextRead::Tangent(Space::World), origin);
                let b = self.context(ContextRead::Bitangent(Space::World), origin);
                let n = self.shading_normal(origin);
                Val::new(ret.clone(), V::Columns(vec![t, b, n]))
            }
            "texture_space_max" => Val::int(1),
            "direction" | "motion" => Val::known(v3, Value::Vector3([0.0; 3])),
            "animation_time" => Val::new(Ty::FLOAT, V::Num(self.context(ContextRead::Time, origin))),
            "transform_point" | "transform_vector" | "transform_normal" => {
                let (from, to) = (space(p.get("from"))?, space(p.get("to"))?);
                let value = p.at(2).clone();
                if from == to {
                    return Ok(value);
                }
                let kind = match name {
                    "transform_point" => TransformKind::Point,
                    "transform_vector" => TransformKind::Vector,
                    _ => TransformKind::Normal,
                };
                let x = Self::n(&value)?;
                let n = self
                    .emit(Op::Transform { kind, from, to }, &[x], Type::Vector3, origin)
                    .map_err(|e| e.to_string())?;
                vec3(n)
            }
            "transform_scale" => {
                let (from, to) = (space(p.get("from"))?, space(p.get("to"))?);
                if from != to {
                    self.warn("state::transform_scale between object and world space: the scale as it is");
                }
                p.get("scale").clone()
            }
            "transform" => {
                let (from, to) = (space(p.get("from"))?, space(p.get("to"))?);
                if from != to {
                    self.warn("state::transform between object and world space: identity");
                }
                let mut columns = Vec::new();
                for i in 0..4 {
                    let mut c = [0.0f32; 4];
                    c[i] = 1.0;
                    columns.push(Num::Known(Value::Vector4(c)));
                }
                Val::new(ret.clone(), V::Columns(columns))
            }
            "rounded_corner_normal" => {
                self.warn("state::rounded_corner_normal: the shading normal");
                vec3(self.shading_normal(origin))
            }
            "meters_per_scene_unit" | "scene_units_per_meter" => Val::float(1.0),
            "object_id" => Val::int(0),
            "wavelength_min" => Val::float(380.0),
            "wavelength_max" => Val::float(780.0),
            "wavelength_base" => {
                let w = [450.0, 550.0, 650.0].map(Val::float).to_vec();
                Val::new(ret.clone(), V::Array(w))
            }
            other => return Err(format!("{other} is not implemented")),
        })
    }
}
