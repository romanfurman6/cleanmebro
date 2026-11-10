//! Our own icons for what SF Symbols lacks (a broom, for one). Each is an
//! SVG under `assets/icons`, drawn as a template: only its alpha counts and
//! the caller tints it, the same way SF Symbols are drawn.

/// The bundled icons by name.
const ICONS: &[(&str, &str)] = &[("broom", include_str!("../../assets/icons/broom.svg"))];

/// Render icon `name` into a `px`-high square: white RGBA (not premultiplied)
/// whose alpha is the icon's coverage. None if there is no such icon.
pub fn render(name: &str, px: u32) -> Option<(usize, usize, Vec<u8>)> {
    let src = ICONS.iter().find(|(n, _)| *n == name)?.1;
    let tree = resvg::usvg::Tree::from_str(src, &resvg::usvg::Options::default()).ok()?;
    let px = px.max(8);
    let mut pm = resvg::tiny_skia::Pixmap::new(px, px)?;
    let s = tree.size();
    let k = px as f32 / s.width().max(s.height());
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(k, k), &mut pm.as_mut());
    let rgba = pm.pixels().iter().flat_map(|c| [255, 255, 255, c.alpha()]).collect();
    Some((px as usize, px as usize, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_renders() {
        for (name, _) in ICONS {
            let (w, h, rgba) = render(name, 48).expect(name);
            assert_eq!((w, h), (48, 48));
            assert!(rgba.chunks(4).any(|p| p[3] > 200), "{name} drew nothing");
        }
        assert!(render("nope", 48).is_none());
    }
}
