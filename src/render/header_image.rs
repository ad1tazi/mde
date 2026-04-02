use std::collections::HashMap;

use cosmic_text::{
    Attrs, Buffer as CosmicBuffer, Color as CosmicColor, Family, FontSystem, Metrics, Shaping,
    SwashCache, Weight,
};
use image::{DynamicImage, ImageBuffer, Rgba};
use ratatui_image::{
    picker::{Picker, ProtocolType},
    protocol::StatefulProtocol,
};

/// Display rows per heading tier (H1=3, H2=2, H3-H6=1).
fn display_rows_for_tier(tier: u8) -> u16 {
    match tier {
        1 => 3,
        2 => 2,
        _ => 1,
    }
}

/// Font size scale factor per heading tier, following mdfried's formula.
/// (12 - tier) / 12.0, but with a minimum base so H6 isn't too small.
fn scale_for_tier(tier: u8) -> f32 {
    match tier {
        1 => 11.0 / 12.0,
        2 => 10.0 / 12.0,
        3 => 9.0 / 12.0,
        4 => 8.0 / 12.0,
        5 => 7.0 / 12.0,
        6 => 6.0 / 12.0,
        _ => 1.0,
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct CacheKey {
    text: String,
    tier: u8,
    width_cells: u16,
}

struct CachedImage {
    protocol: StatefulProtocol,
}

pub struct HeaderImageSupport {
    picker: Picker,
    font_system: FontSystem,
    swash_cache: SwashCache,
    image_cache: HashMap<CacheKey, CachedImage>,
    /// Pixel dimensions of a terminal cell (width, height).
    cell_size: (u16, u16),
    /// Last viewport width in cells, for cache invalidation on resize.
    last_width_cells: u16,
}

impl HeaderImageSupport {
    /// Try to initialize image support. Returns None if the terminal
    /// doesn't support any graphics protocol (falls back to Halfblocks only).
    pub fn new() -> Option<Self> {
        let picker = Picker::from_query_stdio().ok()?;

        // Halfblocks is the fallback — we only want real graphics protocols
        if picker.protocol_type() == ProtocolType::Halfblocks {
            return None;
        }

        let cell_size = picker.font_size();
        if cell_size.0 == 0 || cell_size.1 == 0 {
            return None;
        }

        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();

        Some(Self {
            picker,
            font_system,
            swash_cache,
            image_cache: HashMap::new(),
            cell_size,
            last_width_cells: 0,
        })
    }

    /// Get the display rows for a heading tier.
    pub fn display_rows(&self, tier: u8) -> u16 {
        display_rows_for_tier(tier)
    }

    /// Check if image support is available.
    pub fn is_available(&self) -> bool {
        true // If we were constructed, we have support
    }

    /// Clear the image cache (e.g. on terminal resize).
    pub fn clear_cache(&mut self) {
        self.image_cache.clear();
    }

    /// Render a header image or return a cached one.
    /// Returns a mutable reference to the cached StatefulProtocol,
    /// which persists across frames to avoid re-encoding.
    pub fn get_or_render(
        &mut self,
        text: &str,
        tier: u8,
        width_cells: u16,
    ) -> &mut StatefulProtocol {
        // Invalidate cache on viewport width change
        if width_cells != self.last_width_cells {
            self.image_cache.clear();
            self.last_width_cells = width_cells;
        }

        let key = CacheKey {
            text: text.to_string(),
            tier,
            width_cells,
        };

        if !self.image_cache.contains_key(&key) {
            let cached = self.render_header_image(text, tier, width_cells);
            self.image_cache.insert(key.clone(), cached);
        }

        &mut self.image_cache.get_mut(&key).unwrap().protocol
    }

    fn render_header_image(&mut self, text: &str, tier: u8, width_cells: u16) -> CachedImage {
        let cell_w = self.cell_size.0 as f32;
        let cell_h = self.cell_size.1 as f32;
        let d_rows = display_rows_for_tier(tier);
        let scale = scale_for_tier(tier);

        // Target image dimensions in pixels
        let img_width = (width_cells as f32 * cell_w) as u32;
        let img_height = (d_rows as f32 * cell_h) as u32;

        // Font size: scale relative to the image height so text fills the rows
        // Use most of the available vertical space
        let font_size = cell_h * scale * d_rows as f32 * 0.75;
        let line_height = img_height as f32;

        let metrics = Metrics::new(font_size, line_height);
        let mut buffer = CosmicBuffer::new(&mut self.font_system, metrics);

        {
            let mut buffer = buffer.borrow_with(&mut self.font_system);
            buffer.set_size(Some(img_width as f32), Some(img_height as f32));

            let attrs = Attrs::new()
                .family(Family::SansSerif)
                .weight(Weight::BOLD)
                .color(CosmicColor::rgb(0xFF, 0xFF, 0xFF));

            buffer.set_text(text, attrs, Shaping::Advanced);
            buffer.shape_until_scroll(true);
        }

        // Render to RGBA image
        let mut img_buffer: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::new(img_width.max(1), img_height.max(1));

        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            CosmicColor::rgb(0xFF, 0xFF, 0xFF),
            |x, y, _w, _h, color| {
                if x >= 0 && y >= 0 {
                    let px = x as u32;
                    let py = y as u32;
                    if px < img_width && py < img_height {
                        let (r, g, b, a) = color.as_rgba_tuple();
                        img_buffer.put_pixel(px, py, Rgba([r, g, b, a]));
                    }
                }
            },
        );

        let dyn_image = DynamicImage::ImageRgba8(img_buffer);
        let protocol = self.picker.new_resize_protocol(dyn_image);
        CachedImage { protocol }
    }
}
