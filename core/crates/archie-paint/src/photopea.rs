//! Photopea, the image editor that is a web page, as Archie draws in it.
//!
//! What was learned on the stage (docs/STAGE.md, "What Photopea taught the stage") is kept
//! here as arithmetic rather than as a list of coordinates to type in:
//!
//! - The document is made by the page itself: Photopea runs a script given in its URL, so a
//!   white sheet of the picture's shape, a first ink and a second one exist before anything
//!   is touched. Its `x` key swaps the two inks, which is how a drawing changes colour
//!   without hunting through panels.
//! - Photopea declares nothing to the accessibility bus; it draws its whole interface in one
//!   canvas. Where its tools are is read off a capture: the page starts where its own grey
//!   (71, 71, 71) does, and the tool column below it is a fixed ladder.
//! - The brush is clicked where the ladder puts it, and that click is what gives the page the
//!   keyboard; `b` then takes the brush even where the column is drawn at another scale (under
//!   WSLg the click lands on the blur tool), and `[` thins it, from 15 pixels to 8 in three
//!   presses and by one after that.
//! - A stroke the compositor executed is not a mark on the paper. After the first lines the
//!   paper is looked at again, and a white sheet means the brush was not in hand.

use crate::{Canvas, Picture};

/// Photopea's own grey (71, 71, 71): its menu bar, option bar and tool column.
pub const UI_GREY: u8 = 71;
/// From the top of the page to the brush in the tool column, and the column's middle.
pub const BRUSH_BELOW_PAGE_TOP: i32 = 302;
pub const TOOL_COLUMN_X: i32 = 19;
/// The brush Photopea starts with, in pixels.
pub const STARTING_BRUSH: u32 = 15;

fn hex([r, g, b]: [u8; 3]) -> String {
    format!("{r:02x}{g:02x}{b:02x}")
}

/// The address that opens Photopea with a white `width`×`height` document called `name`,
/// `first` in hand and `second` behind it.
pub fn url(width: u32, height: u32, name: &str, first: [u8; 3], second: [u8; 3]) -> String {
    let name: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .take(40)
        .collect();
    let script = format!(
        "app.documents.add({width},{height},72,'{name}',NewDocumentMode.RGB,DocumentFill.WHITE);\
         var a=new SolidColor();a.rgb.hexValue='{}';app.foregroundColor=a;\
         var b=new SolidColor();b.rgb.hexValue='{}';app.backgroundColor=b;",
        hex(first),
        hex(second)
    );
    let hash = serde_json::json!({ "script": script }).to_string();
    format!("https://www.photopea.com/#{}", percent_encode(&hash))
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn is_grey(picture: &Picture, x: u32, y: u32) -> bool {
    picture.grey(x, y).is_some_and(|g| g.abs_diff(UI_GREY) <= 3)
}

/// Where Photopea's page starts in a capture of the browser window: the first row of its
/// grey at the left edge that the next twenty rows continue. None when Photopea is not on
/// screen (still loading, or another page).
pub fn page_top(picture: &Picture) -> Option<i32> {
    let x = 3;
    let height = picture.height();
    (0..height.saturating_sub(20))
        .find(|&y| (y..y + 20).all(|row| is_grey(picture, x, row)))
        .map(|y| y as i32)
}

/// Where to click for the brush, given where the page starts.
pub fn brush_at(page_top: i32) -> (i32, i32) {
    (TOOL_COLUMN_X, page_top + BRUSH_BELOW_PAGE_TOP)
}

/// How many presses of `[` take the starting brush down to `size` pixels.
pub fn thinner_presses(size: u32) -> usize {
    match size {
        s if s >= STARTING_BRUSH => 0,
        s if s >= 8 => 3,
        s => 3 + (8 - s.max(1)) as usize,
    }
}

/// How much of `canvas` is no longer white, from nothing to one: whether a brush is
/// putting ink down at all.
pub fn inked(picture: &Picture, canvas: Canvas) -> f32 {
    let (mut marked, mut seen) = (0_u32, 0_u32);
    let step = 3;
    let mut y = canvas.y.max(0) as u32;
    while y < (canvas.y.max(0) as u32 + canvas.height).min(picture.height()) {
        let mut x = canvas.x.max(0) as u32;
        while x < (canvas.x.max(0) as u32 + canvas.width).min(picture.width()) {
            if let Some(grey) = picture.grey(x, y) {
                seen += 1;
                if grey < 230 {
                    marked += 1;
                }
            }
            x += step;
        }
        y += step;
    }
    if seen == 0 {
        0.0
    } else {
        marked as f32 / seen as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(top: u32) -> Picture {
        // A light browser frame, then Photopea's grey from `top` down, and a white sheet.
        let (w, h) = (400u32, 600u32);
        let mut grey = vec![250u8; (w * h) as usize];
        for y in top..h {
            for x in 0..w {
                let white = (100..300).contains(&x) && (200..550).contains(&y);
                grey[(y * w + x) as usize] = if white { 255 } else { UI_GREY };
            }
        }
        Picture::from_grey(grey, w, h).unwrap()
    }

    #[test]
    fn the_page_starts_where_photopeas_grey_does() {
        assert_eq!(page_top(&window(85)), Some(85));
        assert_eq!(
            brush_at(85),
            (19, 387),
            "the brush where it was clicked on the stage"
        );
        let blank = Picture::from_grey(vec![250; 500], 10, 50).unwrap();
        assert_eq!(page_top(&blank), None);
    }

    #[test]
    fn the_brush_thins_as_photopea_steps_it() {
        assert_eq!(thinner_presses(15), 0);
        assert_eq!(thinner_presses(8), 3);
        assert_eq!(
            thinner_presses(5),
            6,
            "measured: six presses showed 5 in the option bar"
        );
        assert_eq!(thinner_presses(3), 8);
        assert_eq!(thinner_presses(0), 10);
    }

    #[test]
    fn ink_is_what_is_no_longer_white() {
        let page = window(85);
        let sheet = Canvas::new(100, 200, 200, 350);
        assert!(inked(&page, sheet) < 0.01);
        let around = Canvas::new(0, 150, 400, 450);
        assert!(
            inked(&page, around) > 0.2,
            "the grey around the sheet counts"
        );
    }

    #[test]
    fn the_address_carries_the_document_and_both_inks() {
        let address = url(
            480,
            716,
            "Mona Lisa",
            [0x5a, 0x3a, 0x22],
            [0xa8, 0x4a, 0x2a],
        );
        assert!(address.starts_with("https://www.photopea.com/#%7B%22script%22"));
        assert!(address.contains("480%2C716"));
        assert!(address.contains("5a3a22") && address.contains("a84a2a"));
        assert!(!address.contains(' '));
        // Nothing of a name can break out of the script's string.
        assert!(!url(10, 10, "a');alert(1);('", [0; 3], [0; 3]).contains("alert%281%29%3B%28%27"));
    }
}
