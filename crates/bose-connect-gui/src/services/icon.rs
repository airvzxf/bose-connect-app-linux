//! The application icon, rendered from the GResource bundle.
//!
//! The binary carries its own icon, so the tray and the desktop
//! notifications show it without anything installed under
//! `/usr/share/icons` or `~/.local/share/icons`. Installing the
//! icon into the icon theme is the package's job.

/// Application id: the name of the `.desktop` file and of the
/// icon that the packages install.
pub const APP_ID: &str = "com.airvzxf.bose-connect-gui";

/// GResource path of the full-colour application icon.
pub const APP_ICON_RESOURCE: &str =
    "/com/airvzxf/bose-connect-gui/icons/hicolor/scalable/apps/bose-connect-gui.svg";

/// Render the application icon as a `side` × `side` RGBA buffer,
/// tightly packed (no row padding). Returns `(width, height, bytes)`.
pub fn render_rgba(side: i32) -> anyhow::Result<(i32, i32, Vec<u8>)> {
    // Tests do not go through `main`, so register the bundle here;
    // a second registration is a cheap refcount bump.
    crate::register_resources();
    let side = side.max(1);
    let pixbuf = gdk_pixbuf::Pixbuf::from_resource_at_scale(APP_ICON_RESOURCE, side, side, true)
        .map_err(|e| anyhow::anyhow!("load SVG icon: {e}"))?;
    let w = pixbuf.width();
    let h = pixbuf.height();
    anyhow::ensure!(
        pixbuf.n_channels() == 4 && pixbuf.has_alpha(),
        "expected an RGBA pixbuf"
    );
    let stride = pixbuf.rowstride() as usize;
    let row_len = w as usize * 4;
    let bytes = pixbuf.read_pixel_bytes();
    // The last row may be shorter than the stride.
    anyhow::ensure!(
        h > 0 && bytes.len() >= stride * (h as usize - 1) + row_len,
        "gdk-pixbuf returned a pixel buffer of unexpected size"
    );
    let mut data = Vec::with_capacity(row_len * h as usize);
    for row in 0..h as usize {
        let start = row * stride;
        data.extend_from_slice(&bytes[start..start + row_len]);
    }
    Ok((w, h, data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_square_rgba_icon() {
        let (w, h, data) = render_rgba(64).expect("SVG must render");
        assert_eq!((w, h), (64, 64));
        assert_eq!(data.len(), 64 * 64 * 4);
        let opaque = data.chunks_exact(4).filter(|p| p[3] == 0xFF).count();
        assert!(opaque > 64 * 64 / 8, "icon must not be mostly transparent");
    }
}
