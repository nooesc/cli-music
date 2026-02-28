use image::{DynamicImage, imageops::FilterType};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

const UPPER_HALF: char = '\u{2580}'; // ▀

/// Convert a DynamicImage to ratatui Lines using half-block characters.
/// Each terminal row represents 2 pixel rows.
pub fn image_to_halfblocks(img: &DynamicImage, width: u16, height: u16) -> Vec<Line<'static>> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let resized = img.resize_exact(width as u32, (height * 2) as u32, FilterType::Triangle);
    let rgb = resized.to_rgb8();
    let mut lines = Vec::with_capacity(height as usize);

    for row in 0..height {
        let mut spans = Vec::with_capacity(width as usize);
        let upper_y = (row * 2) as u32;
        let lower_y = upper_y + 1;

        for col in 0..width {
            let up = rgb.get_pixel(col as u32, upper_y);
            let lo = rgb.get_pixel(col as u32, lower_y);

            let fg = Color::Rgb(up[0], up[1], up[2]);
            let bg = Color::Rgb(lo[0], lo[1], lo[2]);

            spans.push(Span::styled(
                UPPER_HALF.to_string(),
                Style::default().fg(fg).bg(bg),
            ));
        }

        lines.push(Line::from(spans));
    }

    lines
}

/// Fetch artwork URL for a track from iTunes Search API.
pub fn fetch_artwork_url(track_name: &str, artist: &str) -> Option<String> {
    let query = format!("{} {}", track_name, artist);
    let encoded = urlencoding::encode(&query);
    let url = format!(
        "https://itunes.apple.com/search?term={}&entity=song&limit=10",
        encoded
    );

    let resp = reqwest::blocking::get(&url).ok()?;
    let json: serde_json::Value = resp.json().ok()?;

    let results = json["results"].as_array()?;
    for result in results {
        if let Some(art_url) = result["artworkUrl100"].as_str() {
            // Upgrade to 300x300 for better quality
            let high_res = art_url.replace("100x100bb", "300x300bb");
            return Some(high_res);
        }
    }
    None
}

/// Download image from URL and decode it.
pub fn download_image(url: &str) -> Option<DynamicImage> {
    let bytes = reqwest::blocking::get(url).ok()?.bytes().ok()?;
    image::load_from_memory(&bytes).ok()
}
