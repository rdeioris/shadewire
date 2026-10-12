use crate::Dag;

/// Which nodes the surface fields can reach, so un-optimized DAGs do not report dead reads.
pub fn live_nodes(dag: &Dag) -> Vec<bool> {
    let mut live = vec![false; dag.nodes.len()];
    let mut stack: Vec<_> = dag.surface.fields.iter().map(|f| f.node).collect();
    while let Some(id) = stack.pop() {
        if live[id.index()] {
            continue;
        }
        live[id.index()] = true;
        stack.extend(dag.args(id));
    }
    live
}
