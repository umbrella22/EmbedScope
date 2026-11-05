//! Presentation order and search context for provider-owned object identities.
use escope_host::UiNode;
use std::collections::{HashMap, HashSet};

/// Group descendants with their parents, preserving provider sibling order.
/// Session::ui_tree validates parent paths before a snapshot reaches the UI.
pub(super) fn preorder(nodes: &[UiNode]) -> Vec<usize> {
    let mut children = HashMap::<u32, Vec<usize>>::new();
    for (index, node) in nodes.iter().enumerate() {
        children.entry(node.parent).or_default().push(index);
    }
    let mut order = Vec::with_capacity(nodes.len());
    let mut stack: Vec<_> = children
        .get(&0)
        .into_iter()
        .flatten()
        .rev()
        .copied()
        .collect();
    while let Some(index) = stack.pop() {
        order.push(index);
        if let Some(children) = children.get(&nodes[index].id) {
            stack.extend(children.iter().rev().copied());
        }
    }
    order
}

/// Search results retain ancestor paths, including when the matching child was
/// enumerated before its parent. Node metadata is displayed without rewriting.
pub(super) fn matching_paths(nodes: &[UiNode], query: &str) -> HashSet<u32> {
    let parents: HashMap<_, _> = nodes.iter().map(|n| (n.id, n.parent)).collect();
    let mut included = HashSet::new();
    for node in nodes {
        if format!("{} {} #{}", node.kind, node.name, node.id)
            .to_lowercase()
            .contains(query)
        {
            let mut id = node.id;
            while id != 0 && included.insert(id) {
                id = parents.get(&id).copied().unwrap_or(0);
            }
        }
    }
    included
}
