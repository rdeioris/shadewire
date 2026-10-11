//! Runs WGSL probe functions on the GPU: one compute invocation writes `case_i()` to slot `i`.

pub mod cases;

use std::sync::mpsc;

use shadewire_ir::{Dag, NodeId, NodeKind};

/// The compute module around `wgsl_functions` (library files + `case_0 .. case_{count-1}`).
/// `sw_zero` is a runtime 0.0 that `opaque_constants` adds to every constant.
pub fn shader(wgsl_functions: &str, count: usize) -> String {
    let mut text = String::from("@group(0) @binding(0) var<storage, read_write> sw_out: array<vec4<f32>>;\n");
    text.push_str("@group(0) @binding(1) var<storage, read> sw_zero: f32;\n\n");
    text.push_str(wgsl_functions);
    text.push_str("\n@compute @workgroup_size(1)\nfn main() {\n");
    for i in 0..count {
        text.push_str(&format!("    sw_out[{i}] = case_{i}();\n"));
    }
    text.push_str("}\n");
    text
}

/// Rewrites a probe function so each `Const` node reads `value + sw_zero`: with literal
/// arguments naga (and the driver) would fold the op at compile time on the CPU and the
/// GPU would never compute it. `node` is the probed node: every `Const` it reads must have
/// been rewritten, or the probe format changed and this panics naming the probe.
pub fn opaque_constants(dag: &Dag, node: NodeId, probe: &str) -> String {
    let mut out = String::new();
    let mut rewritten = 0;
    for line in probe.lines() {
        match opaque_line(dag, line) {
            Some(opaque) => {
                rewritten += 1;
                out.push_str(&opaque);
            }
            None => out.push_str(line),
        }
        out.push('\n');
    }
    let wanted = live_constants(dag, node);
    let name = probe.lines().next().unwrap_or("").trim_start_matches("fn ").split('(').next().unwrap_or("");
    assert_eq!(rewritten, wanted, "probe {name}: made {rewritten} constants opaque, it reads {wanted}");
    out
}

/// How many `Const` nodes `node` reads, itself included.
fn live_constants(dag: &Dag, node: NodeId) -> usize {
    let mut seen = vec![false; dag.nodes.len()];
    let mut stack = vec![node];
    while let Some(id) = stack.pop() {
        if !seen[id.index()] {
            seen[id.index()] = true;
            stack.extend(dag.args(id));
        }
    }
    dag.nodes.iter().zip(&seen).filter(|(n, live)| **live && matches!(n.kind, NodeKind::Const(_))).count()
}

/// `    let nI: T = expr;` for a `Const` node I becomes `    let nI: T = <expr made opaque>;`.
/// `None` for any line not of exactly that shape.
fn opaque_line(dag: &Dag, line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix("let n")?;
    let (index, rest) = rest.split_once(": ")?;
    if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let index: usize = index.parse().ok()?;
    let (ty, expr) = rest.split_once(" = ")?;
    let expr = expr.strip_suffix(';')?;
    if !matches!(dag.nodes.get(index).map(|n| &n.kind), Some(NodeKind::Const(_))) {
        return None;
    }
    let opaque = match ty {
        "f32" => format!("({expr} + sw_zero)"),
        "i32" => format!("({expr} + i32(sw_zero))"),
        "bool" => format!("(({expr}) == (sw_zero == 0.0))"),
        vector => format!("({expr} + {vector}(sw_zero))"),
    };
    Some(format!("    let n{index}: {ty} = {opaque};"))
}

/// Runs `case_0 .. case_{count-1}` once and reads back their vec4 results.
/// Prints the adapter used; `None` (after printing the skip line) when no adapter exists.
pub fn run_probes(wgsl_functions: &str, count: usize) -> Option<Vec<[f32; 4]>> {
    pollster::block_on(run(&shader(wgsl_functions, count), count))
}

async fn run(source: &str, count: usize) -> Option<Vec<[f32; 4]>> {
    let instance = wgpu::Instance::default();
    let adapter = match instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await {
        Ok(adapter) => adapter,
        Err(e) => {
            if std::env::var("SHADEWIRE_REQUIRE_GPU").as_deref() == Ok("1") {
                panic!("gpu parity: no adapter and SHADEWIRE_REQUIRE_GPU=1 ({e})");
            }
            println!("gpu parity skipped: no adapter");
            return None;
        }
    };
    let info = adapter.get_info();
    println!("gpu parity adapter: {} ({:?})", info.name, info.backend);
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("device");
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("gpu parity"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let size = (count * 16) as u64;
    // Zero-initialised by wgpu.
    let zero = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sw_zero"),
        size: 16,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let out = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sw_out"),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("staging"),
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("gpu parity"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: out.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: zero.as_entire_binding() },
        ],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&out, 0, &staging, 0, size);
    queue.submit(Some(encoder.finish()));

    let (sender, receiver) = mpsc::channel();
    staging.map_async(wgpu::MapMode::Read, .., move |result| sender.send(result).expect("send"));
    device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
    receiver.recv().expect("map callback").expect("map");
    let values: Vec<[f32; 4]> = {
        let view = staging.get_mapped_range(..).expect("mapped range");
        view.chunks_exact(16)
            .map(|c| {
                let f = |i: usize| f32::from_le_bytes(c[i * 4..i * 4 + 4].try_into().expect("4 bytes"));
                [f(0), f(1), f(2), f(3)]
            })
            .collect()
    };
    staging.unmap();
    out.destroy();
    zero.destroy();
    staging.destroy();
    device.destroy();
    Some(values)
}

#[cfg(test)]
mod tests {
    use super::opaque_constants;
    use shadewire_ir::{Dag, Node, NodeId, NodeKind, Op, Type, Value};

    fn two_constants() -> (Dag, NodeId) {
        let mut dag = Dag::new("m");
        let mut add = |kind, ty| dag.add(Node { kind, ty, origin: String::new() });
        let a = add(NodeKind::Const(Value::Float(2.0)), Type::Float);
        let b = add(NodeKind::Const(Value::Color3([0.1, 0.2, 0.3])), Type::Color3);
        let m = add(NodeKind::Op { op: Op::Multiply, args: vec![b, a] }, Type::Color3);
        (dag, m)
    }

    #[test]
    fn both_constants_are_made_opaque() {
        let (dag, m) = two_constants();
        let probe = "fn case_7() -> vec4<f32> {
    let n0: f32 = 2.0;
    let n1: vec3<f32> = vec3<f32>(0.1, 0.2, 0.3);
    let n2: vec3<f32> = (n1 * vec3<f32>(n0));
    return vec4<f32>(n2, 0.0);
}
";
        let out = opaque_constants(&dag, m, probe);
        assert!(out.contains("    let n0: f32 = (2.0 + sw_zero);\n"), "{out}");
        assert!(out.contains("    let n1: vec3<f32> = (vec3<f32>(0.1, 0.2, 0.3) + vec3<f32>(sw_zero));\n"), "{out}");
        assert!(out.contains("    let n2: vec3<f32> = (n1 * vec3<f32>(n0));\n"), "{out}");
    }

    #[test]
    #[should_panic(expected = "probe case_7: made 1 constants opaque, it reads 2")]
    fn a_malformed_constant_line_fails_the_guard() {
        let (dag, m) = two_constants();
        let probe = "fn case_7() -> vec4<f32> {
    let n0: f32 = 2.0;
    let n1 : vec3<f32> = vec3<f32>(0.1, 0.2, 0.3);
    let n2: vec3<f32> = (n1 * vec3<f32>(n0));
    return vec4<f32>(n2, 0.0);
}
";
        opaque_constants(&dag, m, probe);
    }
}
