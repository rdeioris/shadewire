//! `::tex`: lookups of 2D textures as MaterialX `image` nodes; the other texture shapes and the queries the IR
//! cannot answer are warnings and defaults.

use shadewire::{Op, Type, Value};
use shadewire_nodes::Arg as NodeArg;

use super::Params;
use crate::eval::Eval;
use crate::types::{TextureShape, Ty};
use crate::value::{Gamma, Num, Texture, V, Val};

type R<T> = Result<T, String>;

/// `tex::wrap_mode` as a MaterialX address mode.
fn address(v: &Val) -> &'static str {
    match v.known_int() {
        Some(0) => "clamp",
        Some(2) => "mirror",
        Some(3) => "constant",
        _ => "periodic",
    }
}

/// The colorspace MDL reads a texture in: `gamma_srgb` is sRGB, `gamma_linear` raw, and `gamma_default` sRGB
/// unless the file is a float format.
pub fn colorspace(t: &Texture) -> Option<String> {
    let float_file = {
        let lower = t.file.to_ascii_lowercase();
        lower.ends_with(".exr") || lower.ends_with(".hdr")
    };
    match t.gamma {
        Gamma::Srgb => Some("srgb_texture".to_string()),
        Gamma::Linear => None,
        Gamma::Default if float_file => None,
        Gamma::Default => Some("srgb_texture".to_string()),
    }
}

impl<'p, 'a> Eval<'p, 'a> {
    /// The `color4` sample of texture `t` at `coord` with the wrap modes.
    fn sample(&mut self, t: &Texture, coord: &Val, wrap_u: &Val, wrap_v: &Val, origin: &str) -> R<Num> {
        let coord = Self::n(coord)?;
        let uv = self.node_of(&coord, origin);
        let inputs = [
            ("file", NodeArg::File { path: t.file.clone(), colorspace: colorspace(t) }),
            ("uaddressmode", NodeArg::Text(address(wrap_u).to_string())),
            ("vaddressmode", NodeArg::Text(address(wrap_v).to_string())),
            ("filtertype", NodeArg::Text("linear".to_string())),
            ("texcoord", NodeArg::Node(uv)),
        ];
        let id = self.e.instantiate("image", Type::Color4, &inputs, origin).map_err(|e| e.to_string())?;
        Ok(Num::Node(id))
    }

    pub(super) fn tex(&mut self, name: &str, p: &Params, ret: &Ty, origin: &str) -> R<Val> {
        let texture = match &p.at(0).v {
            V::Texture(t) => Some(t.clone()),
            _ => None,
        };
        match name {
            "texture_isvalid" => Ok(Val::bool(texture.is_some_and(|t| !t.file.is_empty()))),
            "width" | "height" | "depth" => {
                self.warn(format!("tex::{name}: 1024 texels"));
                Ok(Val::int(1024))
            }
            "width_offset" | "height_offset" | "depth_offset" | "first_frame" | "last_frame" => Ok(Val::int(0)),
            "grid_to_object_space" => {
                let columns = (0..4)
                    .map(|i| {
                        let mut c = [0.0f32; 4];
                        c[i] = 1.0;
                        Num::Known(Value::Vector4(c))
                    })
                    .collect();
                Ok(Val::new(ret.clone(), V::Columns(columns)))
            }
            _ if name.starts_with("lookup_") || name.starts_with("texel_") => {
                let Some(t) = texture else { return Err("a lookup without a texture".into()) };
                if t.shape != TextureShape::Tex2d {
                    self.warn(format!("tex::{name} on a {:?} texture: black", t.shape));
                    return Ok(Self::known_of(ret, 0.0));
                }
                if t.file.is_empty() {
                    return Ok(Self::known_of(ret, 0.0));
                }
                for crop in ["crop_u", "crop_v"] {
                    if let Some((_, c)) = p.values.iter().find(|(n, _)| n == crop)
                        && c.known_value() != Some(&Value::Vector2([0.0, 1.0]))
                    {
                        self.warn(format!("tex::{name}: {crop} ignored"));
                    }
                }
                let sample = if name.starts_with("texel_") {
                    // A texel by number, read where a texture of `tex::width` texels has it.
                    self.warn(format!("tex::{name}: a lookup at the texel's centre in 1024 texels"));
                    let parts = self.parts(p.get("coord"), origin)?;
                    let mut uv = Vec::new();
                    for c in parts {
                        let x =
                            self.convert_scalar(&c, crate::types::Scalar::Int, crate::types::Scalar::Float, origin)?;
                        let x = self
                            .emit(Op::Add, &[x, Num::Known(Value::Float(0.5))], Type::Float, origin)
                            .map_err(|e| e.to_string())?;
                        uv.push(
                            self.emit(Op::Divide, &[x, Num::Known(Value::Float(1024.0))], Type::Float, origin)
                                .map_err(|e| e.to_string())?,
                        );
                    }
                    let coord = self.from_parts(&Ty::float_vector(2), uv, origin)?;
                    let repeat = Val::int(1);
                    self.sample(&t, &coord, &repeat, &repeat, origin)?
                } else {
                    self.sample(&t, p.get("coord"), p.get("wrap_u"), p.get("wrap_v"), origin)?
                };
                let n = match ret {
                    Ty::Color => self.emit(Op::Convert, &[sample], Type::Color3, origin),
                    Ty::Scalar(_) => self.emit(Op::Extract(0), &[sample], Type::Float, origin),
                    Ty::Vector(_, 2) => self.emit(Op::Convert, &[sample], Type::Vector2, origin),
                    Ty::Vector(_, 3) => self.emit(Op::Convert, &[sample], Type::Vector3, origin),
                    _ => self.emit(Op::Convert, &[sample], Type::Vector4, origin),
                }
                .map_err(|e| e.to_string())?;
                Ok(Val::new(ret.clone(), V::Num(n)))
            }
            other => Err(format!("{other} is not implemented")),
        }
    }
}
