//! KWin backdrop blur.
//!
//! KWin blurs whatever is behind a window when the window asks it to, by
//! setting `_KDE_NET_WM_BLUR_BEHIND_REGION` on itself. The property holds a
//! list of `x, y, width, height` rectangles, in physical pixels relative to
//! the window; an empty list means "the whole window".
//!
//! ACLI has rounded corners over a transparent background, so it sends a
//! region that follows those corners — otherwise the blur would show through
//! as a hard square behind them.
//!
//! This is X11 only. On a Plasma Wayland session the equivalent is a Wayland
//! protocol that winit does not expose, so the window falls back to the dense
//! glass palette; running under XWayland (`WINIT_UNIX_BACKEND=x11`) or using
//! the Force Blur KWin effect both get the blur back.

use raw_window_handle::RawWindowHandle;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, AtomEnum, ConnectionExt, PropMode};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

const BLUR_ATOM: &str = "_KDE_NET_WM_BLUR_BEHIND_REGION";

/// Holds the X11 connection used to keep the blur region in step with the
/// window size.
pub struct Blur {
    conn: RustConnection,
    atom: xproto::Atom,
    window: u32,
    /// Whether a compositor that understands the property is running.
    announced: bool,
    last: Option<(u32, u32, u8)>,
}

impl Blur {
    /// Connects to the X server and prepares to blur `handle`.
    ///
    /// Returns `None` when the window is not an X11 window (a native Wayland
    /// surface, say) or the display cannot be reached — neither is an error,
    /// it just means there is no blur to be had.
    pub fn new(handle: &RawWindowHandle) -> Option<Self> {
        let window = match handle {
            RawWindowHandle::Xlib(h) => h.window as u32,
            RawWindowHandle::Xcb(h) => h.window.get(),
            _ => return None,
        };

        let (conn, screen_num) = x11rb::connect(None).ok()?;
        let atom = conn.intern_atom(false, BLUR_ATOM.as_bytes()).ok()?.reply().ok()?.atom;

        // KWin advertises the effects it has loaded by setting the matching
        // property on the root window, so this tells us whether blurring is
        // actually switched on rather than just asked for.
        let root = conn.setup().roots.get(screen_num)?.root;
        let announced = conn
            .get_property(false, root, atom, AtomEnum::ANY, 0, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|reply| reply.type_ != x11rb::NONE)
            .unwrap_or(false);

        Some(Self {
            conn,
            atom,
            window,
            announced,
            last: None,
        })
    }

    /// True when a compositor announced support for the blur property.
    pub fn announced(&self) -> bool {
        self.announced
    }

    /// Sets the blur region to a rounded rectangle of `width` x `height`
    /// physical pixels. Cheap to call every frame: it only talks to the X
    /// server when the geometry actually changed.
    pub fn apply(&mut self, width: u32, height: u32, radius: u8) {
        if width == 0 || height == 0 {
            return;
        }
        if self.last == Some((width, height, radius)) {
            return;
        }
        self.last = Some((width, height, radius));

        let region = rounded_region(width, height, radius as u32);
        let _ = self.conn.change_property32(
            PropMode::REPLACE,
            self.window,
            self.atom,
            AtomEnum::CARDINAL,
            &region,
        );
        let _ = self.conn.flush();
    }
}

/// Builds the `x, y, w, h` list approximating a rounded rectangle: one row per
/// scanline through each corner, and a single rectangle for everything between.
fn rounded_region(width: u32, height: u32, radius: u32) -> Vec<u32> {
    let r = radius.min(width / 2).min(height / 2);
    if r == 0 {
        return vec![0, 0, width, height];
    }

    let mut out = Vec::with_capacity((r as usize * 2 + 1) * 4);
    let rf = r as f32;

    // Horizontal inset of the rounded edge, `row` scanlines into the corner.
    let inset = |row: u32| -> u32 {
        let dy = rf - (row as f32 + 0.5);
        let dx = (rf * rf - dy * dy).max(0.0).sqrt();
        (rf - dx).round() as u32
    };

    let row_rect = |out: &mut Vec<u32>, y: u32, row: u32| {
        let i = inset(row).min(width / 2);
        let w = width - 2 * i;
        if w > 0 {
            out.extend_from_slice(&[i, y, w, 1]);
        }
    };

    for row in 0..r {
        row_rect(&mut out, row, row);
    }
    // A window exactly twice the corner radius tall has no straight middle.
    if height > 2 * r {
        out.extend_from_slice(&[0, r, width, height - 2 * r]);
    }
    for row in 0..r {
        row_rect(&mut out, height - 1 - row, row);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each rectangle is `x, y, w, h`.
    fn rects(region: &[u32]) -> Vec<(u32, u32, u32, u32)> {
        region.chunks_exact(4).map(|c| (c[0], c[1], c[2], c[3])).collect()
    }

    #[test]
    fn a_zero_radius_is_just_the_window() {
        assert_eq!(rounded_region(800, 600, 0), vec![0, 0, 800, 600]);
    }

    #[test]
    fn rounded_corners_are_inset_and_symmetric() {
        let (w, h, r) = (400u32, 300u32, 12u32);
        let rects = rects(&rounded_region(w, h, r));

        // One scanline per corner row, plus the straight middle.
        let scanlines: Vec<_> = rects.iter().filter(|c| c.3 == 1).copied().collect();
        assert_eq!(scanlines.len(), (r * 2) as usize);

        let middle: Vec<_> = rects.iter().filter(|c| c.3 > 1).copied().collect();
        assert_eq!(middle, vec![(0, r, w, h - 2 * r)]);

        // The outermost scanline is inset the most; the innermost not at all.
        let top: Vec<_> = scanlines.iter().filter(|c| c.1 < r).copied().collect();
        assert_eq!(top.len(), r as usize);
        assert!(top[0].0 > top[top.len() - 1].0, "{top:?}");
        assert_eq!(top[top.len() - 1].0, 0);

        // Every top scanline has a mirror image at the bottom.
        for (x, y, cw, _) in top {
            let mirror = scanlines
                .iter()
                .find(|c| c.1 == h - 1 - y)
                .unwrap_or_else(|| panic!("no mirror for row {y}"));
            assert_eq!((mirror.0, mirror.2), (x, cw), "row {y}");
        }
    }

    #[test]
    fn every_rectangle_stays_inside_the_window() {
        for (w, h, r) in [
            (400u32, 300u32, 12u32),
            (60, 40, 12),
            (24, 24, 40), // radius larger than the window
            (1000, 700, 1),
            (1, 1, 12),
        ] {
            let region = rounded_region(w, h, r);
            assert!(!region.is_empty(), "{w}x{h} r{r}: no region at all");
            for (x, y, rw, rh) in rects(&region) {
                assert!(x + rw <= w, "{w}x{h} r{r}: {x}+{rw} > {w}");
                assert!(y + rh <= h, "{w}x{h} r{r}: {y}+{rh} > {h}");
                assert!(rw > 0 && rh > 0, "{w}x{h} r{r}: empty rect");
            }
        }
    }

    #[test]
    fn covers_every_scanline_exactly_once() {
        let (w, h, r) = (200u32, 120u32, 10u32);
        let mut covered = vec![0u32; h as usize];
        for (_, y, _, rh) in rects(&rounded_region(w, h, r)) {
            for row in y..y + rh {
                covered[row as usize] += 1;
            }
        }
        assert!(
            covered.iter().all(|&n| n == 1),
            "each scanline should appear once: {covered:?}"
        );
    }
}
