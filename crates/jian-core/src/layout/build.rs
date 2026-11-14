//! Shared layout construction over owned and borrowed schema views.
use super::*;
use crate::document::{layout_tree::LayoutTreeView, BorrowedNodeTree};

impl LayoutEngine {
    /// Build the identical layout pass while borrowing immutable source schemas.
    pub fn build_borrowed(&mut self, tree: &BorrowedNodeTree<'_>) -> CoreResult<Vec<NodeId>> {
        self.build_with_mode(tree, false)
    }
    pub(super) fn build_with_mode<T: LayoutTreeView>(
        &mut self,
        doc_tree: &T,
        responsive: bool,
    ) -> CoreResult<Vec<NodeId>> {
        // A real taffy compute supersedes any preload snapshot —
        // post-compute `node_rect` reads must come from taffy, not
        // stale AOT geometry. Plan 19 D1.
        self.preload = SecondaryMap::new();
        self.tree = TaffyTree::new();
        self.map = SecondaryMap::new();
        self.parent = SecondaryMap::new();
        self.reference = None;
        self.constraint_lints.clear();
        self.node_order.clear();
        self.overrides = SecondaryMap::new();
        self.compute_count = 0;
        self.bound_hit = false;
        self.root_owner = SecondaryMap::new();
        self.root_origins = SecondaryMap::new();
        self.base_styles = SecondaryMap::new();
        self.origin_normalized.clear();
        self.node_order = doc_tree.keys_top_down();
        for &root in doc_tree.roots() {
            if let Some(origin) = doc_tree
                .node(root)
                .and_then(|node| resolve::explicit_position(node.schema))
            {
                self.root_origins.insert(root, origin);
            }
            let mut stack = vec![root];
            while let Some(key) = stack.pop() {
                // First owner wins; a duplicate cross-root child or cycle
                // is malformed input — skip instead of looping/overwriting.
                if self.root_owner.insert(key, root).is_some() {
                    debug_assert!(false, "node reachable from multiple roots or cycle");
                    continue;
                }
                stack.extend(
                    doc_tree
                        .node(key)
                        .expect("layout tree key")
                        .children
                        .iter()
                        .rev()
                        .copied(),
                );
            }
        }

        // Pass 1: create a taffy node for each doc node. `node_to_style`
        // handles both containers (Frame/Group/Rectangle) and leaves
        // (Text / IconFont / Image / …) so leaf sizes propagate into
        // flex measurements.
        for (key, data) in doc_tree.iter_nodes() {
            let mut style = if responsive {
                resolve::node_to_style_responsive(data.schema, &mut self.constraint_lints)
            } else {
                resolve::node_to_style(data.schema)
            };
            // Direction-aware flex_shrink: a child whose MAIN-AXIS size is a
            // fixed Number must not be shrunk below it. `node_to_style` only
            // pins fully-fixed squares (both axes Number); here, with the parent
            // in hand, we also pin a `width:200, height:fit_content` card in a
            // horizontal scroll row so it keeps its width — its title doesn't
            // wrap and sibling cards stay equal-height (otherwise an overflowing
            // row squeezes the cards into ragged, uneven heights).
            if let Some(p) = data.parent {
                let parent_schema = doc_tree.node(p).expect("layout parent key").schema;
                if resolve::child_joins_computed_flow(parent_schema, data.schema) {
                    style.position = Position::Relative;
                    style.inset.left = LengthPercentageAuto::Auto;
                    style.inset.right = LengthPercentageAuto::Auto;
                    style.inset.top = LengthPercentageAuto::Auto;
                    style.inset.bottom = LengthPercentageAuto::Auto;
                }
                if resolve::node_is_stack_container(
                    doc_tree.node(p).expect("layout parent key").schema,
                ) {
                    // `layout: "none"` parent: children share one grid cell
                    // and overlap. The flex hints below all assume a flow
                    // line and would fight the stack (`main_axis` has no
                    // meaning when every child starts at the same origin).
                    resolve::apply_stack_child(&mut style);
                } else {
                    let parent_horizontal = resolve::node_is_horizontal(
                        doc_tree.node(p).expect("layout parent key").schema,
                    );
                    if resolve::main_axis_is_fixed_number(data.schema, parent_horizontal) {
                        style.flex_shrink = 0.0;
                    }
                    // A fill-height child in a row stretches to the row's height
                    // (so a space_between sidebar's footer reaches the bottom)
                    // instead of collapsing on an indefinite-height parent.
                    resolve::apply_fill_container_axes(&mut style, data.schema, parent_horizontal);
                }
            }
            self.base_styles.insert(key, style.clone());
            let ctx = text_measure_for(data.schema);
            let id = self
                .tree
                .new_leaf_with_context(style, ctx)
                .map_err(|e| CoreError::Layout(e.to_string()))?;
            self.map.insert(key, id);
            if let Some(p) = data.parent {
                self.parent.insert(key, p);
            }
        }

        // Pass 2: wire parent/child relationships.
        for (key, data) in doc_tree.iter_nodes() {
            if !data.children.is_empty() {
                let parent = self.map[key];
                let child_ids: Vec<NodeId> = data.children.iter().map(|k| self.map[*k]).collect();
                self.tree
                    .set_children(parent, &child_ids)
                    .map_err(|e| CoreError::Layout(e.to_string()))?;
            }
        }

        Ok(doc_tree.roots().iter().map(|k| self.map[*k]).collect())
    }
}
