use crate::interpret::ContextSource;
use crate::{Op, TransformKind, Type, Value};

pub fn eval(op: &Op, args: &[Value], ty: Type, ctx: &mut dyn ContextSource) -> Value {
    let Op::Transform { kind, from, to } = op else {
        panic!("{op:?} is not a space op");
    };
    let v = args[0].floats();
    let moved = ctx.transform(*kind, *from, *to, [v[0], v[1], v[2]]);
    let out = if *kind == TransformKind::Normal {
        let len = (moved[0] * moved[0] + moved[1] * moved[1] + moved[2] * moved[2]).sqrt();
        [moved[0] / len, moved[1] / len, moved[2] / len]
    } else {
        moved
    };
    Value::from_floats(ty, &out)
}

#[cfg(test)]
mod tests {
    use super::eval;
    use crate::interpret::ContextSource;
    use crate::{ContextRead, Op, Space, TransformKind, Type, Value};

    struct Doubler;
    impl ContextSource for Doubler {
        fn read(&mut self, _: &ContextRead) -> Value {
            unreachable!()
        }
        fn transform(&mut self, _: TransformKind, _: Space, _: Space, v: [f32; 3]) -> [f32; 3] {
            [v[0] * 2.0, v[1] * 2.0, v[2] * 2.0]
        }
    }

    fn op(kind: TransformKind) -> Op {
        Op::Transform { kind, from: Space::Model, to: Space::World }
    }

    #[test]
    fn point_and_vector_keep_the_host_result_but_normal_is_normalized() {
        let v = [Value::Vector3([0.0, 0.0, 1.5])];
        assert_eq!(eval(&op(TransformKind::Point), &v, Type::Vector3, &mut Doubler), Value::Vector3([0.0, 0.0, 3.0]));
        assert_eq!(eval(&op(TransformKind::Vector), &v, Type::Vector3, &mut Doubler), Value::Vector3([0.0, 0.0, 3.0]));
        assert_eq!(eval(&op(TransformKind::Normal), &v, Type::Vector3, &mut Doubler), Value::Vector3([0.0, 0.0, 1.0]));
    }
}
