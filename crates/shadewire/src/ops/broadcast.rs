use crate::{Type, Value};

pub fn broadcast(value: &Value, to: Type) -> Value {
    let from = value.ty();
    if from == to {
        return value.clone();
    }
    if from.is_scalar() {
        return Value::splat(to, value.as_f32());
    }
    if from.component_count() == to.component_count() {
        return Value::from_floats(to, &value.floats());
    }
    panic!("cannot broadcast {:?} to {:?}", from, to);
}

/// Applies `f` per component after broadcasting every argument to `ty`.
pub fn componentwise(args: &[Value], ty: Type, f: impl Fn(&[f32]) -> f32) -> Value {
    let wide: Vec<Vec<f32>> = args.iter().map(|a| broadcast(a, ty).floats()).collect();
    let mut out = Vec::with_capacity(ty.component_count());
    for c in 0..ty.component_count() {
        let column: Vec<f32> = wide.iter().map(|w| w[c]).collect();
        out.push(f(&column));
    }
    Value::from_floats(ty, &out)
}
