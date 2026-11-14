//! Read-only layout topology without duplicating canonical schema records.
use super::{tree::node_schema_id, NodeKey, NodeTree};
use jian_ops_schema::node::PenNode;
use slotmap::{SecondaryMap, SlotMap};
use std::collections::BTreeMap;

struct BorrowedNode<'a> {
    schema: &'a PenNode,
    parent: Option<NodeKey>,
    children: Vec<NodeKey>,
}

/// Temporary layout index borrowing every schema record from its source tree.
/// Text/style contexts copied into LayoutEngine remain owned after build.
pub struct BorrowedNodeTree<'a> {
    nodes: SlotMap<NodeKey, BorrowedNode<'a>>,
    pub by_id: BTreeMap<&'a str, NodeKey>,
    roots: Vec<NodeKey>,
}

fn schema_children(node: &PenNode) -> &[PenNode] {
    match node {
        PenNode::Frame(n) => n.children.as_deref(),
        PenNode::Group(n) => n.children.as_deref(),
        PenNode::Rectangle(n) => n.children.as_deref(),
        PenNode::Tabs(n) => n.children.as_deref(),
        PenNode::Ref(n) => n.children.as_deref(),
        _ => None,
    }
    .unwrap_or(&[])
}

impl<'a> BorrowedNodeTree<'a> {
    pub fn from_root(root: &'a PenNode) -> Self {
        Self::from_roots(std::slice::from_ref(root))
    }

    pub fn from_roots(roots: &'a [PenNode]) -> Self {
        fn count(node: &PenNode) -> usize {
            schema_children(node).iter().fold(1usize, |n, child| {
                n.checked_add(count(child))
                    .expect("allocated canonical node count fits usize")
            })
        }
        let count = roots.iter().fold(0usize, |n, root| {
            n.checked_add(count(root))
                .expect("allocated canonical node count fits usize")
        });
        let mut tree = Self {
            nodes: SlotMap::with_capacity_and_key(count),
            by_id: BTreeMap::new(),
            roots: Vec::with_capacity(roots.len()),
        };
        for root in roots {
            tree.insert(root, None);
        }
        tree
    }

    fn insert(&mut self, schema: &'a PenNode, parent: Option<NodeKey>) -> NodeKey {
        let children = schema_children(schema);
        let key = self.nodes.insert(BorrowedNode {
            schema,
            parent,
            children: Vec::with_capacity(children.len()),
        });
        self.by_id.insert(node_schema_id(schema), key);
        if let Some(parent) = parent {
            self.nodes[parent].children.push(key);
        } else {
            self.roots.push(key);
        }
        for child in children {
            self.insert(child, Some(key));
        }
        key
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    pub fn capacity(&self) -> usize {
        self.nodes.capacity()
    }
    pub fn schema(&self, key: NodeKey) -> Option<&'a PenNode> {
        self.nodes.get(key).map(|node| node.schema)
    }
}

pub(crate) struct LayoutNode<'a> {
    pub schema: &'a PenNode,
    pub parent: Option<NodeKey>,
    pub children: &'a [NodeKey],
}

/// Both owned runtime trees and borrowed loader trees use one layout algorithm.
pub(crate) trait LayoutTreeView {
    fn roots(&self) -> &[NodeKey];
    fn node(&self, key: NodeKey) -> Option<LayoutNode<'_>>;
    fn iter_nodes(&self) -> impl Iterator<Item = (NodeKey, LayoutNode<'_>)>;

    fn keys_top_down(&self) -> Vec<NodeKey> {
        let mut order = Vec::new();
        let mut visited = SecondaryMap::new();
        let mut stack: Vec<_> = self.roots().iter().rev().copied().collect();
        while let Some(key) = stack.pop() {
            if visited.insert(key, ()).is_some() {
                debug_assert!(false, "layout tree child links form cycle/duplicate");
                continue;
            }
            order.push(key);
            stack.extend(
                self.node(key)
                    .expect("layout tree key")
                    .children
                    .iter()
                    .rev()
                    .copied(),
            );
        }
        order
    }
}

impl LayoutTreeView for NodeTree {
    fn roots(&self) -> &[NodeKey] {
        &self.roots
    }
    fn node(&self, key: NodeKey) -> Option<LayoutNode<'_>> {
        self.nodes.get(key).map(|n| LayoutNode {
            schema: &n.schema,
            parent: n.parent,
            children: &n.children,
        })
    }
    fn iter_nodes(&self) -> impl Iterator<Item = (NodeKey, LayoutNode<'_>)> {
        self.nodes.iter().map(|(key, n)| {
            (
                key,
                LayoutNode {
                    schema: &n.schema,
                    parent: n.parent,
                    children: &n.children,
                },
            )
        })
    }
    fn keys_top_down(&self) -> Vec<NodeKey> {
        NodeTree::keys_top_down(self)
    }
}
impl LayoutTreeView for BorrowedNodeTree<'_> {
    fn roots(&self) -> &[NodeKey] {
        &self.roots
    }
    fn node(&self, key: NodeKey) -> Option<LayoutNode<'_>> {
        self.nodes.get(key).map(|n| LayoutNode {
            schema: n.schema,
            parent: n.parent,
            children: &n.children,
        })
    }
    fn iter_nodes(&self) -> impl Iterator<Item = (NodeKey, LayoutNode<'_>)> {
        self.nodes.iter().map(|(key, n)| {
            (
                key,
                LayoutNode {
                    schema: n.schema,
                    parent: n.parent,
                    children: &n.children,
                },
            )
        })
    }
}
