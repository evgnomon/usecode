// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Render CSV from stdin as an A4-landscape PDF table on stdout.
//! Pure Rust: lays the table out itself and writes the PDF with pdf-writer,
//! embedding the subset of DejaVu Sans the table uses, so any text the font
//! covers renders without a browser engine or system fonts.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::process::exit;

use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str};
use subsetter::GlyphRemapper;
use ttf_parser::{Face, GlyphId};

const FONT: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");

// A4 landscape with a 1cm margin, in points.
const PAGE_W: f32 = 841.89;
const PAGE_H: f32 = 595.28;
const MARGIN: f32 = 28.35;
const SIZE: f32 = 9.0;
const LEADING: f32 = SIZE * 1.2;
const PAD_X: f32 = 6.0;
const PAD_Y: f32 = 4.0;
const BORDER: f32 = 0.75;

const HEADER_BG: [f32; 3] = [0.933, 0.933, 0.933];
const STRIPE_BG: [f32; 3] = [0.969, 0.969, 0.969];
const BORDER_RGB: [f32; 3] = [0.6, 0.6, 0.6];

struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

/// Tabs become spaces and carriage returns go; newlines stay as line breaks.
fn clean(s: &str) -> String {
    s.chars()
        .filter(|&c| c != '\r')
        .map(|c| if c == '\t' { ' ' } else { c })
        .collect()
}

/// Missing/empty cells render as "NaN", like pandas' default `na_rep`.
fn cell(s: Option<&str>) -> String {
    match s {
        Some(v) if !v.is_empty() => clean(v),
        _ => "NaN".into(),
    }
}

fn read_table(input: impl io::Read) -> Result<Table, String> {
    let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(input);
    let headers = rdr.headers().map_err(|e| e.to_string())?.clone();
    if headers.is_empty() {
        return Err("No columns to parse from file".into());
    }
    let mut rows = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| e.to_string())?;
        if rec.len() > headers.len() {
            return Err(format!(
                "Error tokenizing data. Expected {} fields, saw {}",
                headers.len(),
                rec.len()
            ));
        }
        rows.push((0..headers.len()).map(|i| cell(rec.get(i))).collect());
    }
    Ok(Table {
        headers: headers.iter().map(clean).collect(),
        rows,
    })
}

/// The embedded font: measures text and records the glyphs it is asked to
/// draw, so only those go into the PDF.
struct Font {
    face: Face<'static>,
    scale: f32,
    remap: GlyphRemapper,
    /// Original glyph id and first character of each subset glyph id.
    used: Vec<(u16, char)>,
}

impl Font {
    fn new() -> Self {
        let face = Face::parse(FONT, 0).expect("embedded font parses");
        Font {
            scale: SIZE / f32::from(face.units_per_em()),
            face,
            remap: GlyphRemapper::new(),
            used: vec![(0, '\u{FFFD}')],
        }
    }

    fn glyph(&self, c: char) -> u16 {
        self.face
            .glyph_index(c)
            .or_else(|| self.face.glyph_index('?'))
            .map_or(0, |g| g.0)
    }

    fn advance(&self, g: u16) -> f32 {
        f32::from(self.face.glyph_hor_advance(GlyphId(g)).unwrap_or(0)) * self.scale
    }

    fn width(&self, s: &str) -> f32 {
        s.chars().map(|c| self.advance(self.glyph(c))).sum()
    }

    /// Two-byte subset glyph ids for an Identity-H encoded string.
    fn encode(&mut self, s: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(s.len() * 2);
        for c in s.chars() {
            let g = self.glyph(c);
            let id = self.remap.remap(g);
            if usize::from(id) == self.used.len() {
                self.used.push((g, c));
            }
            out.extend(id.to_be_bytes());
        }
        out
    }

    /// Font-unit metric in the PDF's 1000-unit glyph space.
    fn pdf_units(&self, v: i16) -> f32 {
        f32::from(v) * 1000.0 / f32::from(self.face.units_per_em())
    }
}

/// Splits a cell into lines no wider than `max`: on newlines, then between
/// words, and inside a word only when it is too wide by itself.
fn wrap(font: &Font, text: &str, max: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut cur = String::new();
        for word in para.split(' ') {
            let candidate = if cur.is_empty() {
                word.to_string()
            } else {
                format!("{cur} {word}")
            };
            if font.width(&candidate) <= max {
                cur = candidate;
                continue;
            }
            if !cur.is_empty() {
                lines.push(cur);
            }
            cur = word.to_string();
            while font.width(&cur) > max && cur.chars().count() > 1 {
                let mut cut = 0;
                let mut w = 0.0;
                for (i, c) in cur.char_indices() {
                    w += font.width(&c.to_string());
                    if w > max && i > 0 {
                        break;
                    }
                    cut = i + c.len_utf8();
                }
                lines.push(cur[..cut].to_string());
                cur = cur[cut..].to_string();
            }
        }
        lines.push(cur);
    }
    lines
}

/// Each column gets its natural width when the table fits the page; when it
/// does not, narrow columns keep theirs and the wide ones share the rest.
fn column_widths(natural: &[f32], avail: f32) -> Vec<f32> {
    let mut widths = natural.to_vec();
    if natural.iter().sum::<f32>() <= avail {
        return widths;
    }
    let mut order: Vec<usize> = (0..natural.len()).collect();
    order.sort_by(|&a, &b| natural[a].total_cmp(&natural[b]));
    let mut left = avail;
    for (n, &i) in order.iter().enumerate() {
        let share = left / (order.len() - n) as f32;
        widths[i] = natural[i].min(share).max(2.0 * PAD_X + SIZE);
        left -= widths[i];
    }
    widths
}

fn row_height(lines: &[Vec<String>]) -> f32 {
    lines.iter().map(Vec::len).max().unwrap_or(1) as f32 * LEADING + 2.0 * PAD_Y
}

/// Draws one row with its top edge at `top` and returns its bottom edge.
fn draw_row(
    c: &mut Content,
    font: &mut Font,
    widths: &[f32],
    lines: &[Vec<String>],
    top: f32,
    bg: Option<[f32; 3]>,
) -> f32 {
    let h = row_height(lines);
    let bottom = top - h;
    let total: f32 = widths.iter().sum();
    if let Some([r, g, b]) = bg {
        c.set_fill_rgb(r, g, b);
        c.rect(MARGIN, bottom, total, h);
        c.fill_nonzero();
    }
    let [r, g, b] = BORDER_RGB;
    c.set_stroke_rgb(r, g, b);
    c.set_line_width(BORDER);
    let mut x = MARGIN;
    for w in widths {
        c.rect(x, bottom, *w, h);
        x += w;
    }
    c.stroke();

    let ascent = f32::from(font.face.ascender()) * font.scale;
    let descent = -f32::from(font.face.descender()) * font.scale;
    let first_baseline = top - PAD_Y - (LEADING - ascent - descent) / 2.0 - ascent;
    c.set_fill_rgb(0.0, 0.0, 0.0);
    c.begin_text();
    c.set_font(Name(b"F1"), SIZE);
    let mut x = MARGIN;
    for (w, cell) in widths.iter().zip(lines) {
        for (k, line) in cell.iter().enumerate() {
            if line.is_empty() {
                continue;
            }
            c.set_text_matrix([
                1.0,
                0.0,
                0.0,
                1.0,
                x + PAD_X,
                first_baseline - k as f32 * LEADING,
            ]);
            c.show(Str(&font.encode(line)));
        }
        x += w;
    }
    c.end_text();
    bottom
}

fn deflate(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(data, 6)
}

/// Lays the table out over as many pages as it needs, repeating the header
/// on each, and returns the PDF.
fn render(table: &Table) -> Result<Vec<u8>, String> {
    let mut font = Font::new();
    let natural: Vec<f32> = (0..table.headers.len())
        .map(|i| {
            std::iter::once(&table.headers[i])
                .chain(table.rows.iter().map(|r| &r[i]))
                .flat_map(|s| s.split('\n'))
                .map(|l| font.width(l))
                .fold(0.0, f32::max)
                + 2.0 * PAD_X
        })
        .collect();
    let widths = column_widths(&natural, PAGE_W - 2.0 * MARGIN);
    let layout = |row: &[String], font: &Font| -> Vec<Vec<String>> {
        row.iter()
            .zip(&widths)
            .map(|(s, w)| wrap(font, s, w - 2.0 * PAD_X))
            .collect()
    };
    let header = layout(&table.headers, &font);
    let rows: Vec<_> = table.rows.iter().map(|r| layout(r, &font)).collect();

    let mut pages = Vec::new();
    let mut c = Content::new();
    let mut y = draw_row(
        &mut c,
        &mut font,
        &widths,
        &header,
        PAGE_H - MARGIN,
        Some(HEADER_BG),
    );
    let mut on_page = 0;
    for (n, row) in rows.iter().enumerate() {
        if on_page > 0 && y - row_height(row) < MARGIN {
            pages.push(std::mem::replace(&mut c, Content::new()).finish());
            y = draw_row(
                &mut c,
                &mut font,
                &widths,
                &header,
                PAGE_H - MARGIN,
                Some(HEADER_BG),
            );
            on_page = 0;
        }
        let bg = (n % 2 == 1).then_some(STRIPE_BG);
        y = draw_row(&mut c, &mut font, &widths, row, y, bg);
        on_page += 1;
    }
    pages.push(c.finish());

    let catalog_id = Ref::new(1);
    let tree_id = Ref::new(2);
    let font_id = Ref::new(3);
    let cid_id = Ref::new(4);
    let descriptor_id = Ref::new(5);
    let file_id = Ref::new(6);
    let cmap_id = Ref::new(7);
    let page_ids: Vec<Ref> = (0..pages.len())
        .map(|i| Ref::new(8 + 2 * i as i32))
        .collect();

    let mut pdf = Pdf::new();
    pdf.catalog(catalog_id).pages(tree_id);
    pdf.pages(tree_id)
        .kids(page_ids.iter().copied())
        .count(pages.len() as i32);
    for (id, content) in page_ids.iter().zip(&pages) {
        let content_id = Ref::new(id.get() + 1);
        let mut page = pdf.page(*id);
        page.media_box(Rect::new(0.0, 0.0, PAGE_W, PAGE_H));
        page.parent(tree_id);
        page.contents(content_id);
        page.resources().fonts().pair(Name(b"F1"), font_id);
        page.finish();
        pdf.stream(content_id, &deflate(content))
            .filter(Filter::FlateDecode);
    }

    // Subset fonts are named with a tag derived from the glyphs they hold.
    let mut hasher = DefaultHasher::new();
    font.used.hash(&mut hasher);
    let mut tag = Hasher::finish(&hasher);
    let mut base = String::new();
    for _ in 0..6 {
        base.push((b'A' + (tag % 26) as u8) as char);
        tag /= 26;
    }
    base.push_str("+DejaVuSans");
    let base = Name(base.as_bytes());
    let info = SystemInfo {
        registry: Str(b"Adobe"),
        ordering: Str(b"Identity"),
        supplement: 0,
    };

    pdf.type0_font(font_id)
        .base_font(base)
        .encoding_predefined(Name(b"Identity-H"))
        .descendant_font(cid_id)
        .to_unicode(cmap_id);

    let em = 1000.0 / SIZE;
    let mut cid = pdf.cid_font(cid_id);
    cid.subtype(CidFontType::Type2)
        .base_font(base)
        .system_info(info)
        .font_descriptor(descriptor_id)
        .default_width(0.0)
        .cid_to_gid_map_predefined(Name(b"Identity"));
    cid.widths()
        .consecutive(0, font.used.iter().map(|&(g, _)| font.advance(g) * em));
    cid.finish();

    let bbox = font.face.global_bounding_box();
    pdf.font_descriptor(descriptor_id)
        .name(base)
        .flags(FontFlags::NON_SYMBOLIC)
        .bbox(Rect::new(
            font.pdf_units(bbox.x_min),
            font.pdf_units(bbox.y_min),
            font.pdf_units(bbox.x_max),
            font.pdf_units(bbox.y_max),
        ))
        .italic_angle(0.0)
        .ascent(font.pdf_units(font.face.ascender()))
        .descent(font.pdf_units(font.face.descender()))
        .cap_height(font.pdf_units(font.face.capital_height().unwrap_or(font.face.ascender())))
        .stem_v(80.0)
        .font_file2(file_id);

    let subset =
        subsetter::subset(FONT, 0, &font.remap).map_err(|e| format!("font subset: {e}"))?;
    pdf.stream(file_id, &deflate(&subset))
        .filter(Filter::FlateDecode)
        .pair(Name(b"Length1"), subset.len() as i32);

    let mut cmap = UnicodeCmap::new(Name(b"Custom"), info);
    for (id, &(_, c)) in font.used.iter().enumerate().skip(1) {
        cmap.pair(id as u16, c);
    }
    pdf.cmap(cmap_id, &cmap.finish());

    Ok(pdf.finish())
}

fn main() {
    let pdf = match read_table(io::stdin().lock()).and_then(|t| render(&t)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("uc-data-pdf: {e}");
            exit(1);
        }
    };
    let mut out = io::stdout().lock();
    if let Err(e) = out.write_all(&pdf).and_then(|()| out.flush()) {
        eprintln!("uc-data-pdf: {e}");
        exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pandas_like_cells() {
        let t = read_table("a,b\n1,<x>\n2\n".as_bytes()).unwrap();
        assert_eq!(t.headers, ["a", "b"]);
        assert_eq!(t.rows, [["1", "<x>"], ["2", "NaN"]]);
    }

    #[test]
    fn rejects_empty_and_ragged() {
        assert!(read_table("".as_bytes()).is_err());
        assert!(read_table("a\n1,2\n".as_bytes()).is_err());
    }

    #[test]
    fn wraps_words_then_characters() {
        let font = Font::new();
        let max = font.width("hello world") - 1.0;
        assert_eq!(wrap(&font, "hello world", max), ["hello", "world"]);
        assert_eq!(wrap(&font, "a\nb", 100.0), ["a", "b"]);
        let long = "x".repeat(50);
        let lines = wrap(&font, &long, 30.0);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| font.width(l) <= 30.0));
        assert_eq!(lines.concat(), long);
    }

    #[test]
    fn narrow_columns_keep_their_width() {
        assert_eq!(column_widths(&[10.0, 20.0], 100.0), [10.0, 20.0]);
        let w = column_widths(&[30.0, 500.0, 500.0], 300.0);
        assert_eq!(w[0], 30.0);
        assert_eq!(w[1], 135.0);
        assert_eq!(w[2], 135.0);
    }

    #[test]
    fn renders_multi_page_pdf() {
        let mut csv = String::from("name,note\n");
        for i in 0..100 {
            csv.push_str(&format!("row {i},Grüße καλημέρα\n"));
        }
        let pdf = render(&read_table(csv.as_bytes()).unwrap()).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        // 100 one-line rows of ~20pt plus a repeated header take four pages.
        assert!(String::from_utf8_lossy(&pdf).contains("/Count 4\n"));
    }
}
