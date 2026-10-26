//! The widget the machine's picture is drawn in.
//!
//! It shows the latest frame at the machine's pixel aspect, centred on black,
//! at the largest whole-number scale that fits, and smaller than scale 1 only
//! when the window is. The picture arrives already composed at the widget's
//! size in device pixels, so it is drawn one to one.

use std::cell::{Cell, RefCell};

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{gdk, glib, graphene};

mod imp {
    use super::*;

    pub struct Screen {
        pub texture: RefCell<Option<gdk::Texture>>,
        /// Width of one source pixel relative to its height.
        pub pixel_aspect: Cell<f32>,
        /// The picture size the window should open at, in logical pixels.
        pub natural: Cell<(i32, i32)>,
        /// The picture size at scale 1, the smallest the window allows.
        pub minimum: Cell<(i32, i32)>,
        /// The machine frame's size, for finding the picture in a composed
        /// texture.
        pub frame_size: Cell<(i32, i32)>,
    }

    impl Default for Screen {
        fn default() -> Self {
            Self {
                texture: RefCell::new(None),
                pixel_aspect: Cell::new(1.0),
                natural: Cell::new((0, 0)),
                minimum: Cell::new((0, 0)),
                frame_size: Cell::new((0, 0)),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Screen {
        const NAME: &'static str = "Emu198xScreen";
        type Type = super::Screen;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Screen {}

    impl WidgetImpl for Screen {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let (min_w, min_h) = self.minimum.get();
            let (nat_w, nat_h) = self.natural.get();
            match orientation {
                gtk4::Orientation::Horizontal => (min_w, nat_w.max(min_w), -1, -1),
                _ => (min_h, nat_h.max(min_h), -1, -1),
            }
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let widget = self.obj();
            let width = widget.width() as f32;
            let height = widget.height() as f32;
            snapshot.append_color(
                &gdk::RGBA::BLACK,
                &graphene::Rect::new(0.0, 0.0, width, height),
            );
            let Some(texture) = self.texture.borrow().clone() else {
                return;
            };
            // The texture is the whole widget at one texture pixel per
            // device pixel, composed by the presenter or the fallback.
            snapshot.append_texture(&texture, &graphene::Rect::new(0.0, 0.0, width, height));
        }
    }
}

glib::wrapper! {
    pub struct Screen(ObjectSubclass<imp::Screen>)
        @extends gtk4::Widget,
        @implements gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Screen {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Screen {
    /// Show a picture the video presenter has already composed for the
    /// whole widget, at its size in device pixels, from a machine frame of
    /// `frame_size`.
    pub fn show_composed(&self, texture: gdk::Texture, frame_size: (u32, u32), pixel_aspect: f32) {
        let imp = self.imp();
        imp.frame_size.set((
            i32::try_from(frame_size.0).unwrap_or(0),
            i32::try_from(frame_size.1).unwrap_or(0),
        ));
        imp.texture.replace(Some(texture));
        imp.pixel_aspect.set(pixel_aspect);
        self.queue_draw();
    }

    /// The widget's size in device pixels.
    pub fn device_size(&self) -> (u32, u32) {
        let scale = self.scale_factor().max(1);
        (
            u32::try_from(self.width().max(1) * scale).unwrap_or(1),
            u32::try_from(self.height().max(1) * scale).unwrap_or(1),
        )
    }

    /// Set the picture's size at scale 1 (the minimum) and at the chosen
    /// scale (the size asked for).
    pub fn set_sizes(&self, minimum: (f64, f64), natural: (f64, f64)) {
        let imp = self.imp();
        imp.minimum.set(round_size(minimum));
        imp.natural.set(round_size(natural));
        self.queue_resize();
    }

    /// The rectangle the picture currently fills inside the widget.
    pub fn picture_rect(&self) -> Option<graphene::Rect> {
        self.imp().texture.borrow().as_ref()?;
        let (frame_w, frame_h) = self.imp().frame_size.get();
        Some(fit(
            frame_w,
            frame_h,
            self.imp().pixel_aspect.get(),
            self.width() as f32,
            self.height() as f32,
        ))
    }
}

fn round_size((width, height): (f64, f64)) -> (i32, i32) {
    (width.round() as i32, height.round() as i32)
}

/// Where a `tex_w` × `tex_h` picture with this pixel aspect goes in a
/// `width` × `height` area: centred, at the largest whole-number scale that
/// fits, or shrunk to fit when even scale 1 does not.
pub(super) fn fit(
    tex_w: i32,
    tex_h: i32,
    pixel_aspect: f32,
    width: f32,
    height: f32,
) -> graphene::Rect {
    let display_w = tex_w as f32 * pixel_aspect.max(f32::MIN_POSITIVE);
    let display_h = tex_h as f32;
    if display_w <= 0.0 || display_h <= 0.0 {
        return graphene::Rect::new(0.0, 0.0, 0.0, 0.0);
    }
    let exact = (width / display_w).min(height / display_h);
    let scale = if exact >= 1.0 { exact.floor() } else { exact };
    let w = display_w * scale;
    let h = display_h * scale;
    graphene::Rect::new(
        ((width - w) / 2.0).round(),
        ((height - h) / 2.0).round(),
        w,
        h,
    )
}

#[cfg(test)]
mod tests {
    use super::fit;

    #[test]
    fn fit_picks_the_largest_whole_scale_and_centres() {
        // 320 × 200 square pixels in 1000 × 700: scale 3 fits (960 × 600).
        let rect = fit(320, 200, 1.0, 1000.0, 700.0);
        assert_eq!((rect.width(), rect.height()), (960.0, 600.0));
        assert_eq!((rect.x(), rect.y()), (20.0, 50.0));
    }

    #[test]
    fn fit_stretches_by_the_pixel_aspect() {
        // Twice-as-wide pixels: 100 × 100 shows as 200 × 100 at scale 1.
        let rect = fit(100, 100, 2.0, 450.0, 450.0);
        assert_eq!((rect.width(), rect.height()), (400.0, 200.0));
    }

    #[test]
    fn fit_shrinks_below_scale_one_only_when_it_must() {
        let rect = fit(320, 200, 1.0, 160.0, 400.0);
        assert_eq!((rect.width(), rect.height()), (160.0, 100.0));
    }
}
