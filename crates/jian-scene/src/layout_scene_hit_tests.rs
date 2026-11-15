use super::*;
use crate::layout_scene::{LayoutScene, SceneNode, ScenePage};

fn leaf(id: &str, kind: NodeKind, bounds: Rect) -> SceneNode {
    let mut n = SceneNode::leaf(id, kind);
    n.bounds = bounds;
    n
}

/// A Frame that paints a body, so it is selectable across its whole
/// rect rather than opting out through `paints_body`.
fn filled(id: &str, bounds: Rect) -> SceneNode {
    let mut n = leaf(id, NodeKind::Frame, bounds);
    n.fill = Some(jian_widgets::Color::rgb_u8(0x11, 0x22, 0x33));
    n
}

fn one_page(children: Vec<SceneNode>) -> LayoutScene {
    LayoutScene {
        pages: vec![ScenePage {
            id: "p".into(),
            name: "P".into(),
            children,
        }],
        active_page_index: 0,
    }
}

fn path(ids: &[&str]) -> Option<Vec<String>> {
    Some(ids.iter().map(|id| (*id).to_owned()).collect())
}

#[test]
fn node_at_doc_point_treats_first_child_as_frontmost() {
    let scene = one_page(vec![
        leaf("over", NodeKind::Rect, Rect::xywh(0.0, 0.0, 50.0, 50.0)),
        leaf("under", NodeKind::Rect, Rect::xywh(10.0, 10.0, 50.0, 50.0)),
    ]);
    // Overlap region -> children[0] is the top layer.
    assert_eq!(
        scene
            .node_at_doc_point(Point2D::new(20.0, 20.0), 1.0)
            .as_deref(),
        Some("over")
    );
    // Only "under" covers (55, 55).
    assert_eq!(
        scene
            .node_at_doc_point(Point2D::new(55.0, 55.0), 1.0)
            .as_deref(),
        Some("under")
    );
    // Dead space.
    assert!(scene
        .node_at_doc_point(Point2D::new(200.0, 200.0), 1.0)
        .is_none());
}

#[test]
fn node_path_returns_frontmost_root_to_deepest_hit() {
    let mut front = leaf(
        "front-root",
        NodeKind::Frame,
        Rect::xywh(0.0, 0.0, 100.0, 100.0),
    );
    front.children = vec![leaf(
        "front-leaf",
        NodeKind::Rect,
        Rect::xywh(10.0, 10.0, 30.0, 30.0),
    )];
    let mut back = leaf(
        "back-root",
        NodeKind::Frame,
        Rect::xywh(0.0, 0.0, 100.0, 100.0),
    );
    back.children = vec![leaf(
        "back-leaf",
        NodeKind::Rect,
        Rect::xywh(10.0, 10.0, 30.0, 30.0),
    )];
    let scene = one_page(vec![front, back]);
    let point = Point2D::new(20.0, 20.0);

    let hit_path = scene.node_path_at_doc_point(point, 1.0);
    assert_eq!(hit_path, path(&["front-root", "front-leaf"]));
    assert_eq!(
        hit_path.as_ref().and_then(|ids| ids.last()).cloned(),
        scene.node_at_doc_point(point, 1.0),
        "the terminal path id must preserve the existing hit-test result"
    );
    assert_eq!(
        scene.node_path_at_doc_point(Point2D::new(200.0, 200.0), 1.0),
        None
    );
}

#[test]
fn node_path_skips_hidden_front_subtree() {
    let mut hidden = leaf(
        "hidden-root",
        NodeKind::Frame,
        Rect::xywh(0.0, 0.0, 100.0, 100.0),
    );
    hidden.hidden = true;
    hidden.children = vec![leaf(
        "hidden-leaf",
        NodeKind::Rect,
        Rect::xywh(10.0, 10.0, 30.0, 30.0),
    )];
    let mut visible = leaf(
        "visible-root",
        NodeKind::Frame,
        Rect::xywh(0.0, 0.0, 100.0, 100.0),
    );
    visible.children = vec![leaf(
        "visible-leaf",
        NodeKind::Rect,
        Rect::xywh(10.0, 10.0, 30.0, 30.0),
    )];
    let scene = one_page(vec![hidden, visible]);

    assert_eq!(
        scene.node_path_at_doc_point(Point2D::new(20.0, 20.0), 1.0),
        path(&["visible-root", "visible-leaf"])
    );
}

/// A tabs frame with two fully overlapping panels — the shape jian
/// compiles a `layout: none` / single-cell grid into. Only the panel the
/// painter draws may be hittable.
fn tabs_scene(active: Option<&str>) -> LayoutScene {
    let mut tabs = leaf("tabs", NodeKind::Frame, Rect::xywh(0.0, 0.0, 200.0, 200.0));
    tabs.widget = Some(crate::layout_scene::SceneWidget {
        kind: "tabs".into(),
        value_str: active.map(str::to_owned),
        options: vec![
            crate::layout_scene::SceneWidgetOption {
                value: "overview".into(),
                label: "Overview".into(),
            },
            crate::layout_scene::SceneWidgetOption {
                value: "details".into(),
                label: "Details".into(),
            },
        ],
        ..Default::default()
    });
    // The panels carry a fill because this test is about which panel
    // the walk routes to, not about whether an empty container claims
    // its own body — an unfilled panel would opt out via `paints_body`
    // and mask the routing this asserts.
    tabs.children = vec![
        filled("overview-panel", Rect::xywh(0.0, 40.0, 200.0, 160.0)),
        filled("details-panel", Rect::xywh(0.0, 40.0, 200.0, 160.0)),
    ];
    one_page(vec![tabs])
}

/// The `layout: none` decoration idiom: a full-bleed wrapper holding
/// a few small marks, stacked in front of the content it decorates.
/// It paints no body, so its empty area must fall through to the
/// content behind rather than swallowing every click in the frame.
#[test]
fn a_full_bleed_transparent_decoration_layer_does_not_swallow_clicks() {
    let mut decoration = leaf(
        "decoration",
        NodeKind::Group,
        Rect::xywh(0.0, 0.0, 200.0, 200.0),
    );
    decoration.children = vec![leaf(
        "tape",
        NodeKind::Rect,
        Rect::xywh(0.0, 0.0, 20.0, 20.0),
    )];
    let mut content = leaf(
        "content",
        NodeKind::Frame,
        Rect::xywh(0.0, 0.0, 200.0, 200.0),
    );
    content.children = vec![leaf(
        "headline",
        NodeKind::Text,
        Rect::xywh(50.0, 100.0, 100.0, 40.0),
    )];
    let mut root = filled("board", Rect::xywh(0.0, 0.0, 200.0, 200.0));
    root.children = vec![decoration, content];
    let scene = one_page(vec![root]);

    // Over the text: the decoration is frontmost and its rect covers
    // this point, but only the text is painted here.
    assert_eq!(
        scene
            .node_at_doc_point(Point2D::new(100.0, 120.0), 1.0)
            .as_deref(),
        Some("headline"),
        "a transparent decoration layer must not intercept the content below it"
    );
    // The path walk agrees, so canvas drill-down and text editing —
    // which both read the hit path — can reach the text node.
    assert_eq!(
        scene.node_path_at_doc_point(Point2D::new(100.0, 120.0), 1.0),
        path(&["board", "content", "headline"]),
    );
    // The decoration's own marks stay selectable.
    assert_eq!(
        scene
            .node_at_doc_point(Point2D::new(10.0, 10.0), 1.0)
            .as_deref(),
        Some("tape"),
    );
    // Empty space in both wrappers falls through to the painted root.
    assert_eq!(
        scene
            .node_at_doc_point(Point2D::new(180.0, 180.0), 1.0)
            .as_deref(),
        Some("board"),
    );
}

#[test]
fn a_painted_container_still_claims_its_whole_body() {
    let mut card = filled("card", Rect::xywh(0.0, 0.0, 100.0, 100.0));
    card.children = vec![leaf(
        "label",
        NodeKind::Text,
        Rect::xywh(10.0, 10.0, 20.0, 20.0),
    )];
    let scene = one_page(vec![card]);
    assert_eq!(
        scene
            .node_at_doc_point(Point2D::new(80.0, 80.0), 1.0)
            .as_deref(),
        Some("card"),
        "a filled card is selectable across its padding, not just on its children"
    );
}

/// A stroke-only outline box paints ink without a fill, so it keeps
/// its body — `paints_body` is about ink, not about `fill` alone.
#[test]
fn a_stroke_only_container_keeps_its_body() {
    let mut outlined = leaf(
        "outlined",
        NodeKind::Frame,
        Rect::xywh(0.0, 0.0, 100.0, 100.0),
    );
    outlined.stroke = Some(crate::layout_scene::SceneStroke {
        color: jian_widgets::Color::rgb_u8(0, 0, 0),
        width: 2.0,
        sides: None,
        align: crate::layout_scene::SceneStrokeAlign::Center,
    });
    let scene = one_page(vec![outlined]);
    assert_eq!(
        scene
            .node_at_doc_point(Point2D::new(50.0, 50.0), 1.0)
            .as_deref(),
        Some("outlined"),
    );
}

#[test]
fn tabs_hit_test_only_reaches_the_active_panel() {
    let point = Point2D::new(100.0, 120.0);

    // Missing / stale values deterministically select the first panel.
    for active in [None, Some("nope")] {
        let scene = tabs_scene(active);
        assert_eq!(
            scene.node_at_doc_point(point, 1.0).as_deref(),
            Some("overview-panel"),
            "unresolved active value must fall back to the first panel"
        );
    }

    let scene = tabs_scene(Some("details"));
    assert_eq!(
        scene.node_at_doc_point(point, 1.0).as_deref(),
        Some("details-panel"),
        "the second tab's panel is on top of the first — the hit must \
         follow the active value, not document order"
    );
    assert_eq!(
        scene.node_path_at_doc_point(point, 1.0),
        path(&["tabs", "details-panel"]),
        "the path walk must apply the same active-panel rule"
    );
}

#[test]
fn node_path_tracks_ancestor_rotation_and_flip() {
    let mut rotated = leaf(
        "rotated-root",
        NodeKind::Frame,
        Rect::xywh(0.0, 0.0, 100.0, 100.0),
    );
    rotated.rotation = std::f32::consts::FRAC_PI_2;
    rotated.children = vec![leaf(
        "rotated-leaf",
        NodeKind::Rect,
        Rect::xywh(10.0, 40.0, 20.0, 20.0),
    )];
    let rotated_scene = one_page(vec![rotated]);
    assert_eq!(
        rotated_scene.node_path_at_doc_point(Point2D::new(50.0, 20.0), 1.0),
        path(&["rotated-root", "rotated-leaf"])
    );

    let mut flipped = leaf(
        "flipped-root",
        NodeKind::Frame,
        Rect::xywh(0.0, 0.0, 100.0, 100.0),
    );
    flipped.flip_x = true;
    flipped.children = vec![leaf(
        "flipped-leaf",
        NodeKind::Rect,
        Rect::xywh(10.0, 10.0, 20.0, 20.0),
    )];
    let flipped_scene = one_page(vec![flipped]);
    assert_eq!(
        flipped_scene.node_path_at_doc_point(Point2D::new(80.0, 20.0), 1.0),
        path(&["flipped-root", "flipped-leaf"])
    );
}

#[test]
fn node_path_keeps_locked_ancestors_but_not_locked_terminal_nodes() {
    let child = leaf("child", NodeKind::Rect, Rect::xywh(10.0, 10.0, 20.0, 20.0));
    let mut locked_root = leaf(
        "locked-root",
        NodeKind::Frame,
        Rect::xywh(0.0, 0.0, 100.0, 100.0),
    );
    locked_root.locked = true;
    locked_root.children = vec![child];
    let scene = one_page(vec![locked_root]);
    assert_eq!(
        scene.node_path_at_doc_point(Point2D::new(20.0, 20.0), 1.0),
        path(&["locked-root", "child"])
    );
    assert_eq!(
        scene.node_path_at_doc_point(Point2D::new(80.0, 80.0), 1.0),
        None,
        "a locked ancestor's bare body must not become the path endpoint"
    );

    let mut locked_front = leaf(
        "locked-front",
        NodeKind::Rect,
        Rect::xywh(10.0, 10.0, 20.0, 20.0),
    );
    locked_front.locked = true;
    let back = leaf(
        "unlocked-back",
        NodeKind::Rect,
        Rect::xywh(10.0, 10.0, 20.0, 20.0),
    );
    let mut root = leaf("root", NodeKind::Frame, Rect::xywh(0.0, 0.0, 100.0, 100.0));
    root.children = vec![locked_front, back];
    let scene = one_page(vec![root]);
    assert_eq!(
        scene.node_path_at_doc_point(Point2D::new(20.0, 20.0), 1.0),
        path(&["root", "unlocked-back"])
    );
}

#[test]
fn hidden_node_is_not_hit() {
    let mut n = leaf("h", NodeKind::Rect, Rect::xywh(0.0, 0.0, 50.0, 50.0));
    n.hidden = true;
    let scene = one_page(vec![n]);
    assert!(scene
        .node_at_doc_point(Point2D::new(25.0, 25.0), 1.0)
        .is_none());
}

#[test]
fn locked_node_body_opts_out_but_child_stays_hittable() {
    let child = leaf("c", NodeKind::Rect, Rect::xywh(10.0, 10.0, 10.0, 10.0));
    let mut frame = leaf("f", NodeKind::Frame, Rect::xywh(0.0, 0.0, 100.0, 100.0));
    frame.locked = true;
    frame.children = vec![child];
    let scene = one_page(vec![frame]);
    // Click on the child → child id even though parent is locked.
    assert_eq!(
        scene
            .node_at_doc_point(Point2D::new(15.0, 15.0), 1.0)
            .as_deref(),
        Some("c")
    );
    // Click on the locked frame's bare body → no hit.
    assert!(scene
        .node_at_doc_point(Point2D::new(80.0, 80.0), 1.0)
        .is_none());
}

#[test]
fn ellipse_uses_tight_oval_geometry() {
    let scene = one_page(vec![leaf(
        "e",
        NodeKind::Ellipse,
        Rect::xywh(0.0, 0.0, 100.0, 100.0),
    )]);
    // Center → inside the oval.
    assert_eq!(
        scene
            .node_at_doc_point(Point2D::new(50.0, 50.0), 1.0)
            .as_deref(),
        Some("e")
    );
    // Corner of the bounds → outside the oval.
    assert!(scene
        .node_at_doc_point(Point2D::new(2.0, 2.0), 1.0)
        .is_none());
}

#[test]
fn line_hit_uses_segment_distance() {
    let mut line = leaf("l", NodeKind::Line, Rect::xywh(0.0, 0.0, 100.0, 0.0));
    line.stroke = Some(crate::layout_scene::SceneStroke {
        color: jian_widgets::geometry::Color::WHITE,
        width: 1.0,
        sides: None,
        align: crate::layout_scene::SceneStrokeAlign::Center,
    });
    let scene = one_page(vec![line]);
    // Right on the horizontal segment.
    assert_eq!(
        scene
            .node_at_doc_point(Point2D::new(50.0, 0.0), 1.0)
            .as_deref(),
        Some("l")
    );
    // Far from the segment → miss.
    assert!(scene
        .node_at_doc_point(Point2D::new(50.0, 40.0), 1.0)
        .is_none());
}

#[test]
fn rotated_node_hit_tracks_rendered_geometry() {
    let mut n = leaf("r", NodeKind::Rect, Rect::xywh(0.0, 0.0, 100.0, 20.0));
    n.rotation = std::f32::consts::FRAC_PI_2; // 90°
    let scene = one_page(vec![n]);
    // After a 90° rotation about (50, 10), the painted rect spans
    // roughly x 40..60, y -40..60 — a point at (50, 50) lands
    // inside the rotated body though it's outside authored bounds.
    assert_eq!(
        scene
            .node_at_doc_point(Point2D::new(50.0, 50.0), 1.0)
            .as_deref(),
        Some("r")
    );
}

#[test]
fn path_hit_points_borrows_handle_free_open_path() {
    let mut path = leaf("p", NodeKind::Path, Rect::xywh(0.0, 0.0, 100.0, 0.0));
    path.points = vec![Point2D::new(0.0, 0.0), Point2D::new(100.0, 0.0)];

    let points = path_hit_points(&path);

    match points {
        crate::path_geometry::PathPoints::Borrowed(slice) => {
            assert!(std::ptr::eq(slice.as_ptr(), path.points.as_ptr()));
        }
        crate::path_geometry::PathPoints::Owned(_) => {
            panic!("handle-free open path hit-test should borrow points")
        }
    }
}

#[test]
fn nodes_intersecting_doc_rect_returns_overlapping_top_level_ids() {
    let scene = one_page(vec![
        leaf("a", NodeKind::Rect, Rect::xywh(0.0, 0.0, 30.0, 30.0)),
        leaf("b", NodeKind::Rect, Rect::xywh(100.0, 100.0, 30.0, 30.0)),
    ]);
    let hits = scene.nodes_intersecting_doc_rect(Rect::xywh(10.0, 10.0, 80.0, 80.0));
    assert_eq!(hits, vec!["a".to_string()]);
}

#[test]
fn nodes_intersecting_doc_rect_handles_negative_size() {
    let scene = one_page(vec![leaf(
        "a",
        NodeKind::Rect,
        Rect::xywh(0.0, 0.0, 30.0, 30.0),
    )]);
    // Negative-size marquee still intersects.
    let hits = scene.nodes_intersecting_doc_rect(Rect::xywh(40.0, 40.0, -30.0, -30.0));
    assert_eq!(hits, vec!["a".to_string()]);
}
