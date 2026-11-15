//! Generic Jian text paint paths; actual shaping/draw stays under font_lock.
use super::*;

pub(super) fn draw_text(canvas: &skia_safe::Canvas, run: &jian_core::render::TextRun) {
    // Both shaping paths below build a `FontMgr` (DirectWrite on Windows) and
    // match system typefaces; serialize the whole draw with every other
    // DirectWrite user so a concurrent measure/paint on another thread can't
    // segfault. Reentrant, so nesting under a locked caller is a no-op.
    crate::font_lock::with_font_lock(|| {
        #[cfg(feature = "textlayout")]
        {
            if draw_text_paragraph(canvas, run) {
                return;
            }
        }
        draw_text_singleline(canvas, run);
    });
}

/// Single-line shaping path (no `textlayout`). Splits on `\n`, runs
/// `font.measure_str` per word, lines wrapped greedily.
fn draw_text_singleline(canvas: &skia_safe::Canvas, run: &jian_core::render::TextRun) {
    use jian_core::render::TextAlign;
    use skia_safe::font_style::Weight;
    use skia_safe::FontStyle;

    let mut p = SkPaint::new(to_sk_color(run.color), None);
    crate::raster_sampling::configure_geometry(&mut p);

    let weight = Weight::from(run.font_weight as i32);
    let style = FontStyle::new(
        weight,
        skia_safe::font_style::Width::NORMAL,
        skia_safe::font_style::Slant::Upright,
    );
    let mgr = skia_safe::FontMgr::new();
    let typeface = if !run.font_family.is_empty() {
        mgr.match_family_style(&run.font_family, style)
            .or_else(|| mgr.legacy_make_typeface(None, style))
    } else {
        mgr.legacy_make_typeface(None, style)
    }
    .expect("default typeface");
    let font = skia_safe::Font::new(typeface, run.font_size);

    // Line-height in pixels (defaults to 1.3× font_size if unset).
    let line_mult = if run.line_height > 0.0 {
        run.line_height
    } else {
        1.3
    };
    let line_h = run.font_size * line_mult;

    // Simple word-wrap: split by spaces and accumulate runs that fit
    // within `max_width`. When `max_width == 0` draw on a single line.
    let lines = if run.max_width > 0.0 {
        wrap_to_lines(&run.content, &font, &p, run.max_width)
    } else {
        vec![std::borrow::Cow::Borrowed(run.content.as_str())]
    };

    for (i, line) in lines.iter().enumerate() {
        let (line_w, _) = font.measure_str(line.as_ref(), Some(&p));
        let x = match run.align {
            TextAlign::Start => run.origin.x,
            TextAlign::Center => run.origin.x + (run.max_width - line_w) * 0.5,
            TextAlign::End => run.origin.x + (run.max_width - line_w),
        };
        // Baseline-y: font_size baseline is roughly 80% of font size.
        // Using `origin.y + font_size` matches CSS-ish placement.
        let y = run.origin.y + run.font_size + (i as f32) * line_h;
        canvas.draw_str(line.as_ref(), SkPoint::new(x, y), &font, &p);
    }
}

fn wrap_to_lines<'a>(
    content: &'a str,
    font: &skia_safe::Font,
    paint: &SkPaint,
    max_width: f32,
) -> Vec<std::borrow::Cow<'a, str>> {
    use std::borrow::Cow;
    let mut lines = Vec::new();
    for paragraph in content.split('\n') {
        if paragraph.is_empty() {
            lines.push(Cow::Borrowed(paragraph));
            continue;
        }
        // Measure the whole paragraph first — if it fits, no wrapping.
        let (w_all, _) = font.measure_str(paragraph, Some(paint));
        if w_all <= max_width {
            lines.push(Cow::Borrowed(paragraph));
            continue;
        }
        let mut cur = String::new();
        for word in paragraph.split_whitespace() {
            let previous_len = cur.len();
            if previous_len != 0 {
                cur.push(' ');
            }
            cur.push_str(word);
            let (tw, _) = font.measure_str(&cur, Some(paint));
            if tw <= max_width || previous_len == 0 {
                continue;
            }
            cur.truncate(previous_len);
            lines.push(Cow::Owned(std::mem::take(&mut cur)));
            cur.push_str(word);
        }
        if !cur.is_empty() {
            lines.push(Cow::Owned(cur));
        }
    }
    lines
}

/// `textlayout` feature path — uses Skia ParagraphBuilder for proper
/// CJK / emoji shaping + bidi + per-segment styles. Returns `true`
/// when shaping ran successfully; the single-line fallback runs
/// when this path returns `false` (e.g. font manager unavailable).
#[cfg(feature = "textlayout")]
fn draw_text_paragraph(canvas: &skia_safe::Canvas, run: &jian_core::render::TextRun) -> bool {
    use jian_core::render::TextAlign;
    use skia_safe::textlayout::{
        ParagraphBuilder, ParagraphStyle, TextAlign as SkTextAlign, TextStyle,
    };

    let collection = paragraph_collection();

    let mut style = ParagraphStyle::new();
    style.set_text_align(match run.align {
        TextAlign::Start => SkTextAlign::Start,
        TextAlign::Center => SkTextAlign::Center,
        TextAlign::End => SkTextAlign::End,
    });
    let mut text_style = TextStyle::new();
    text_style.set_font_size(run.font_size);
    if run.line_height > 0.0 {
        // Apply line-height on the TEXT style (matching `SkiaMeasure`), not
        // the paragraph style, so paint and measure use the identical box;
        // `half_leading` splits the extra leading half above + half below
        // the glyphs the way Pencil/CSS do (skia's proportional default
        // drifts each baseline and ghosts multi-line text vs Pencil).
        text_style.set_height(run.line_height);
        text_style.set_height_override(true);
        text_style.set_half_leading(true);
    }
    let c4 = to_sk_color(run.color);
    text_style.set_color(skia_safe::Color::from_argb(
        (c4.a * 255.0) as u8,
        (c4.r * 255.0) as u8,
        (c4.g * 255.0) as u8,
        (c4.b * 255.0) as u8,
    ));
    if !run.font_family.is_empty() {
        text_style.set_font_families(&[run.font_family.as_str()]);
    }
    // Match the single-line fallback's font-weight handling so
    // enabling the `textlayout` feature doesn't change visual
    // output for ASCII text.
    let fs = skia_safe::FontStyle::new(
        skia_safe::font_style::Weight::from(run.font_weight as i32),
        skia_safe::font_style::Width::NORMAL,
        skia_safe::font_style::Slant::Upright,
    );
    text_style.set_font_style(fs);
    crate::font_variations::set_paragraph_weight(&mut text_style, run.font_weight);
    style.set_text_style(&text_style);

    let mut builder = ParagraphBuilder::new(&style, collection);
    builder.add_text(&run.content);
    let mut paragraph = builder.build();
    let max_w = if run.max_width > 0.0 {
        run.max_width
    } else {
        f32::MAX
    };
    paragraph.layout(max_w);
    paragraph.paint(canvas, (run.origin.x, run.origin.y));
    true
}

#[cfg(feature = "textlayout")]
fn paragraph_collection() -> skia_safe::textlayout::FontCollection {
    use skia_safe::{textlayout::FontCollection, FontMgr};
    use std::cell::RefCell;
    struct Cached {
        generation: u64,
        collection: Option<FontCollection>,
    }
    impl Drop for Cached {
        fn drop(&mut self) {
            // The TLS slot is first touched under font_lock, so its destructor
            // precedes that lock's reentrancy flag at thread shutdown.
            crate::font_lock::with_font_lock(|| drop(self.collection.take()));
        }
    }
    thread_local! {
        static COLLECTION: RefCell<Option<Cached>> = const { RefCell::new(None) };
    }
    // Read before construction: an import racing the native build cannot be
    // recorded as already incorporated by a stale collection.
    let generation = crate::bundled_fonts::generation();
    if let Some(collection) = COLLECTION.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|entry| entry.generation == generation)
            .and_then(|entry| entry.collection.clone())
    }) {
        return collection;
    }
    // Identical manager/provider ordering to the original draw path; this is
    // separate from measurement's stronger ordered-font-manager contract.
    let provider = crate::bundled_fonts::asset_provider().unwrap_or_default();
    let mut collection = FontCollection::new();
    collection.set_default_font_manager(FontMgr::new(), None);
    collection.set_asset_font_manager(Some(provider.into()));
    COLLECTION.with(|slot| {
        *slot.borrow_mut() = Some(Cached {
            generation,
            collection: Some(collection.clone()),
        });
    });
    collection
}
