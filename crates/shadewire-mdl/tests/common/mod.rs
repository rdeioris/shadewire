//! Helpers for the writer's tests: `mdlc` (the MDL SDK's compiler) as a second reader of what the writer writes.
#![allow(dead_code)]

use std::process::Command;

/// Checks `text` as MDL module `module` with `mdlc` (`SHADEWIRE_MDLC`, or on PATH): `Ok(false)` when the tool is
/// absent, `Err` with its output and the numbered text when it rejects the module.
pub fn mdlc(text: &str, module: &str) -> Result<bool, String> {
    let Some(exe) = shadewire::shader::fixtures::tool("mdlc") else {
        return Ok(false);
    };
    let dir = std::env::temp_dir().join("shadewire-mdlc").join(module);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{module}.mdl")), text).map_err(|e| e.to_string())?;
    let out = Command::new(exe)
        .arg("-p")
        .arg(&dir)
        .arg("--no-resolve-resources")
        .arg(format!("::{module}"))
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(true);
    }
    let numbered: Vec<String> = text.lines().enumerate().map(|(i, l)| format!("{:4} {l}", i + 1)).collect();
    Err(format!(
        "mdlc rejected {module}:\n{}{}\n\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
        numbered.join("\n")
    ))
}

/// `mdlc` must accept `text` when it is installed.
pub fn assert_mdlc(text: &str, module: &str) {
    match mdlc(text, module) {
        Ok(true) => {}
        Ok(false) => eprintln!("mdlc not found (SHADEWIRE_MDLC): {module} not checked"),
        Err(e) => panic!("{e}"),
    }
}

use std::collections::BTreeMap;

use shadewire::interpret::eval_surface;
use shadewire::testing::{Probe, ProbeTextures};
use shadewire::{ClosureId, ClosureNode, Dag};

/// A lobe of a closure tree with what reaches it: its kind, the weight its path gives it (the lobe's own `weight`
/// input, every `multiply` factor and `mix` side on the way, as a color), and its other inputs at a probe.
#[derive(Clone, Debug, PartialEq)]
pub struct FlatLobe {
    pub kind: String,
    pub weight: [f32; 3],
    pub inputs: BTreeMap<String, Vec<f32>>,
}

/// What a BSDF does with light, as Shadewire's closure functions hold it: its response and its throughput.
pub type Scatter = ([f32; 3], [f32; 3]);

/// A lobe's own scatter, from its kind and its inputs.
pub type LobeScatter = dyn Fn(&str, &BTreeMap<String, Vec<f32>>) -> Scatter;

/// What `dag`'s BSDF tree scatters at `probe`, its nodes combined as Shadewire's closure functions combine them
/// (`closure_base.wgsl`), each lobe's own scatter from `lobe` (its kind and its inputs, `weight` among them).
pub fn bsdf_scatter(dag: &Dag, probe: &mut Probe, lobe: &LobeScatter) -> Scatter {
    let fields: BTreeMap<String, Vec<f32>> =
        eval_surface(dag, probe, &ProbeTextures).into_iter().map(|(n, v)| (n, v.floats())).collect();
    let none = ([0.0; 3], [1.0; 3]);
    let Some(graph) = &dag.surface.closure else { return none };
    let Some(root) = graph.bsdf else { return none };
    fn eval(
        graph: &shadewire::ClosureGraph,
        id: ClosureId,
        fields: &BTreeMap<String, Vec<f32>>,
        lobe: &LobeScatter,
    ) -> Scatter {
        let each = |f: &dyn Fn(usize) -> f32| [f(0), f(1), f(2)];
        let as3 = |v: &[f32]| if v.len() == 1 { [v[0]; 3] } else { [v[0], v[1], v[2]] };
        match &graph.nodes[id as usize] {
            ClosureNode::Lobe { kind, inputs, .. } => {
                let values = inputs.iter().map(|(input, field)| (input.clone(), fields[field].clone())).collect();
                lobe(kind, &values)
            }
            ClosureNode::Layer { top, base } => {
                let ((rt, tt), (rb, tb)) = (eval(graph, *top, fields, lobe), eval(graph, *base, fields, lobe));
                (each(&|i| rt[i] + rb[i] * tt[i]), each(&|i| tt[i] * tb[i]))
            }
            ClosureNode::Mix { fg, bg, mix } => {
                let ((rf, tf), (rb, tb)) = (eval(graph, *fg, fields, lobe), eval(graph, *bg, fields, lobe));
                let m = fields[mix][0];
                (each(&|i| rb[i] + (rf[i] - rb[i]) * m), each(&|i| tb[i] + (tf[i] - tb[i]) * m))
            }
            ClosureNode::Add { in1, in2 } => {
                let ((r1, t1), (r2, t2)) = (eval(graph, *in1, fields, lobe), eval(graph, *in2, fields, lobe));
                (each(&|i| r1[i] + r2[i]), each(&|i| (t1[i] + t2[i] - 1.0).max(0.0)))
            }
            ClosureNode::Multiply { in1, in2 } => {
                let (r, t) = eval(graph, *in1, fields, lobe);
                let w = as3(&fields[in2]);
                (each(&|i| r[i] * w[i].clamp(0.0, 1.0)), t)
            }
        }
    }
    eval(graph, root, &fields, lobe)
}

/// The lobes of `dag`'s BSDF tree then its EDF tree, tops before bases, evaluated at `probe`. A uniform EDF's color
/// folds into its weight, as MDL writes it (`tint(color, diffuse_edf())`). The base of a weighted layer
/// (`mix(fg = layer(top, base), bg = base, w)`) is one lobe at the whole weight, under the top at `w`.
pub fn flat_lobes(dag: &Dag, probe: &mut Probe) -> Vec<FlatLobe> {
    let fields: BTreeMap<String, Vec<f32>> =
        eval_surface(dag, probe, &ProbeTextures).into_iter().map(|(n, v)| (n, v.floats())).collect();
    let Some(graph) = &dag.surface.closure else { return Vec::new() };
    let as3 = |v: &[f32]| if v.len() == 1 { [v[0]; 3] } else { [v[0], v[1], v[2]] };
    let times = |a: [f32; 3], b: [f32; 3]| [a[0] * b[0], a[1] * b[1], a[2] * b[2]];
    fn walk(
        graph: &shadewire::ClosureGraph,
        id: ClosureId,
        weight: [f32; 3],
        fields: &BTreeMap<String, Vec<f32>>,
        out: &mut Vec<FlatLobe>,
        as3: &dyn Fn(&[f32]) -> [f32; 3],
        times: &dyn Fn([f32; 3], [f32; 3]) -> [f32; 3],
    ) {
        match &graph.nodes[id as usize] {
            ClosureNode::Lobe { kind, inputs, closures, .. } => {
                let mut w = weight;
                let mut values = BTreeMap::new();
                for (input, field) in inputs {
                    let v = fields[field].clone();
                    if input == "weight" || (kind == "uniform_edf" && input == "color") {
                        w = times(w, as3(&v));
                    } else {
                        values.insert(input.clone(), v);
                    }
                }
                out.push(FlatLobe { kind: kind.clone(), weight: w, inputs: values });
                if let Some(base) = closures.get("base") {
                    walk(graph, *base, weight, fields, out, as3, times);
                }
            }
            ClosureNode::Layer { top, base } => {
                walk(graph, *top, weight, fields, out, as3, times);
                walk(graph, *base, weight, fields, out, as3, times);
            }
            ClosureNode::Mix { fg, bg, mix } => {
                let m = fields[mix][0];
                if let ClosureNode::Layer { top, base } = &graph.nodes[*fg as usize]
                    && base == bg
                {
                    walk(graph, *top, times(weight, [m; 3]), fields, out, as3, times);
                    walk(graph, *base, weight, fields, out, as3, times);
                    return;
                }
                walk(graph, *fg, times(weight, [m; 3]), fields, out, as3, times);
                walk(graph, *bg, times(weight, [1.0 - m; 3]), fields, out, as3, times);
            }
            ClosureNode::Add { in1, in2 } => {
                walk(graph, *in1, weight, fields, out, as3, times);
                walk(graph, *in2, weight, fields, out, as3, times);
            }
            ClosureNode::Multiply { in1, in2 } => {
                walk(graph, *in1, times(weight, as3(&fields[in2])), fields, out, as3, times);
            }
        }
    }
    let mut out = Vec::new();
    for root in [graph.bsdf, graph.edf].into_iter().flatten() {
        walk(graph, root, [1.0; 3], &fields, &mut out, &as3, &times);
    }
    out
}
