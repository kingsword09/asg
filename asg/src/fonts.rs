//! Font file parsing, subsetting, and WOFF2 embedding.
//!
//! Only compiled with the `embed-fonts` feature. TrueType faces are
//! subsetted to the codepoints actually used by the recording; CFF faces
//! remain complete. Each face is emitted as a WOFF2 or OpenType
//! data-URI `@font-face` rule so the SVG renders
//! with the intended fonts on machines that do not have them installed.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use skrifa::raw::collections::int_set::IntSet;
use skrifa::raw::types::{NameId, Tag};
use skrifa::{FontRef, MetadataProvider};

/// One embedded `@font-face` rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedFace {
    pub family: String,
    pub italic: bool,
    pub weight: u16,
    pub format: &'static str,
    pub data_uri: String,
}

/// Parse a font file and extract the CSS-relevant face metadata.
pub fn parse_face(data: &[u8]) -> Result<EmbeddedFace> {
    let font = FontRef::new(data)?;
    let family = [NameId::TYPOGRAPHIC_FAMILY_NAME, NameId::FAMILY_NAME]
        .into_iter()
        .find_map(|id| {
            font.localized_strings(id)
                .english_or_first()
                .map(|name| name.to_string().trim().to_owned())
                .filter(|name| !name.is_empty())
        })
        .context("font has no usable family name (name IDs 16 or 1)")?;

    let attributes = font.attributes();
    let weight = attributes.weight.value().round().clamp(1.0, 1000.0) as u16;
    let italic = matches!(attributes.style, skrifa::attribute::Style::Italic);

    Ok(EmbeddedFace {
        family,
        italic,
        weight,
        format: "woff2",
        data_uri: String::new(),
    })
}

/// Detect bitmap-only fonts (Apple SBIX, Microsoft CBDT/CBLC) that must not
/// be subsetted or embedded: their outlines are absent and the subset would
/// be enormous or empty.
fn is_bitmap_font(font: &FontRef<'_>) -> bool {
    font.table_data(Tag::new(b"sbix")).is_some() || font.table_data(Tag::new(b"CBLC")).is_some()
}

/// Subset TrueType `data` to `codepoints` and encode as a WOFF2 data URI.
/// CFF fonts use complete OpenType data because the subsetter and WOFF2
/// encoder only support TrueType outlines.
pub fn embed(data: &[u8], codepoints: &BTreeSet<char>) -> Result<EmbeddedFace> {
    let font = FontRef::new(data)?;
    if is_bitmap_font(&font) {
        bail!("bitmap fonts (SBIX/CBDT) cannot be embedded");
    }

    let mut face = parse_face(data)?;
    // skera 0.6 drops CFF/CFF2 tables, and ttf2woff2 only accepts TrueType.
    if font.table_data(Tag::new(b"CFF ")).is_some() || font.table_data(Tag::new(b"CFF2")).is_some()
    {
        log::warn!(
            "embedding CFF font '{}' as complete OpenType data; use TrueType for glyph subsetting and WOFF2 compression",
            face.family
        );
        face.format = "opentype";
        face.data_uri = format!(
            "data:font/otf;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(data)
        );
        return Ok(face);
    }

    let subset = subset_font(&font, codepoints)?;
    let woff2 = ttf2woff2::encode(&subset, ttf2woff2::BrotliQuality::default())
        .map_err(|error| anyhow::anyhow!("WOFF2 encoding failed: {error}"))?;

    face.data_uri = format!(
        "data:font/woff2;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(woff2)
    );
    Ok(face)
}

fn subset_font(font: &FontRef<'_>, codepoints: &BTreeSet<char>) -> Result<Vec<u8>> {
    let mut unicodes = IntSet::empty();
    unicodes.insert(0x20);
    unicodes.insert(0xFE0E);
    for ch in codepoints {
        unicodes.insert(*ch as u32);
    }

    let mut drop_tables = IntSet::empty();
    for tag in skera::DEFAULT_DROP_TABLES {
        drop_tables.insert(*tag);
    }
    let layout_features = skera::DEFAULT_LAYOUT_FEATURES.iter().copied().collect();

    // Keep shaping for all scripts and the glyphs it references, along with
    // names needed by font loaders and the font's license metadata.
    let plan = skera::Plan::new(
        &IntSet::empty(),
        &unicodes,
        font,
        skera::SubsetFlags::SUBSET_FLAGS_NO_HINTING,
        &drop_tables,
        &IntSet::all(),
        &layout_features,
        &IntSet::all(),
        &IntSet::all(),
    );
    Ok(skera::subset_font(font, &plan)?)
}

/// Load and embed each font, subsetting TrueType faces against `codepoints`.
/// Returns faces in the order the files were supplied.
pub fn load_faces(
    paths: &[std::path::PathBuf],
    codepoints: &BTreeSet<char>,
) -> Result<Vec<EmbeddedFace>> {
    let mut faces = Vec::with_capacity(paths.len());
    for path in paths {
        let data = read_font_file(path)?;
        faces.push(
            embed(&data, codepoints)
                .with_context(|| format!("failed to embed font {}", path.display()))?,
        );
    }
    Ok(faces)
}

fn read_font_file(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("cannot read font file {}", path.display()))
}

/// Write `@font-face` rules for all embedded faces.
pub fn write_font_faces(output: &mut String, faces: &[EmbeddedFace]) -> std::fmt::Result {
    for face in faces {
        write!(
            output,
            "@font-face{{font-family:'{}';font-style:{};font-weight:{};src:url({}) format('{}')}}",
            escape_css_family(&face.family),
            if face.italic { "italic" } else { "normal" },
            face.weight,
            face.data_uri,
            face.format
        )?;
    }
    Ok(())
}

fn escape_css_family(family: &str) -> String {
    // The CSS string is also XML text inside the SVG's <style> element.
    family
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\a ")
        .replace('\r', "\\d ")
        .replace('\u{c}', "\\c ")
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use skrifa::raw::TableProvider;

    const SANS_REGULAR: &[u8] = include_bytes!("../tests/fonts/AsgTestSans-Regular.ttf");
    const SANS_BOLD: &[u8] = include_bytes!("../tests/fonts/AsgTestSans-Bold.ttf");
    const SERIF_REGULAR: &[u8] = include_bytes!("../tests/fonts/AsgTestSerif-Regular.otf");
    const LAYOUT_REGULAR: &[u8] = include_bytes!("../tests/fonts/AsgTestLayout-Regular.ttf");

    #[test]
    fn reads_family_style_and_weight_from_ttf() {
        let face = parse_face(SANS_REGULAR).unwrap();
        assert_eq!(face.family, "Asg Test Sans");
        assert_eq!(face.weight, 400);
        assert!(!face.italic);

        let bold = parse_face(SANS_BOLD).unwrap();
        assert_eq!(bold.weight, 700);
        assert_eq!(bold.family, "Asg Test Sans");
    }

    #[test]
    fn rejects_non_font_data() {
        assert!(parse_face(b"not a font").is_err());
    }

    #[test]
    fn prefers_english_typographic_family_over_localized_legacy_names() {
        let face = parse_face(LAYOUT_REGULAR).unwrap();
        assert_eq!(face.family, "Asg Test Layout");
    }

    #[test]
    fn preserves_shaping_glyphs_and_names_when_subsetting() {
        let source = FontRef::new(LAYOUT_REGULAR).unwrap();
        let data = subset_font(&source, &BTreeSet::from(['A', 'B'])).unwrap();
        let font = FontRef::new(&data).unwrap();

        assert_eq!(parse_face(&data).unwrap().family, "Asg Test Layout");
        assert!(font.charmap().map('A').is_some());
        assert!(font.charmap().map('B').is_some());
        assert!(font.charmap().map('C').is_none());
        // .notdef, space, A, B, and C retained only as a ligature output.
        assert_eq!(font.maxp().unwrap().num_glyphs(), 5);

        let gsub = font.gsub().unwrap();
        assert!(
            gsub.script_list()
                .unwrap()
                .script_records()
                .iter()
                .any(|record| record.script_tag() == Tag::new(b"arab"))
        );
        assert_eq!(gsub.lookup_list().unwrap().lookup_count(), 1);
        assert_eq!(
            font.gpos().unwrap().lookup_list().unwrap().lookup_count(),
            1
        );
    }

    #[test]
    fn subsets_ttf_and_produces_woff2_data_uri() {
        let mut codepoints = BTreeSet::new();
        codepoints.insert('A');
        codepoints.insert('B');
        codepoints.insert('界');
        let face = embed(SANS_REGULAR, &codepoints).unwrap();

        assert_eq!(face.family, "Asg Test Sans");
        let uri = face
            .data_uri
            .strip_prefix("data:font/woff2;base64,")
            .unwrap();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(uri)
            .unwrap();
        assert_eq!(&decoded[..4], b"wOF2");
    }

    #[test]
    fn preserves_cff_outlines_when_embedding_otf() {
        let mut codepoints = BTreeSet::new();
        codepoints.insert('A');
        let face = embed(SERIF_REGULAR, &codepoints).unwrap();

        assert_eq!(face.family, "Asg Test Serif");
        assert_eq!(face.format, "opentype");
        let uri = face.data_uri.strip_prefix("data:font/otf;base64,").unwrap();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(uri)
            .unwrap();
        let font = FontRef::new(&decoded).unwrap();
        let glyph = font.charmap().map('A').unwrap();
        assert!(font.outline_glyphs().get(glyph).is_some());
        assert_eq!(decoded, SERIF_REGULAR);

        let mut output = String::new();
        write_font_faces(&mut output, &[face]).unwrap();
        assert!(output.contains("format('opentype')}"));
    }

    #[test]
    fn empty_codepoints_still_embed_base_glyphs() {
        let face = embed(SANS_REGULAR, &BTreeSet::new()).unwrap();
        let uri = face
            .data_uri
            .strip_prefix("data:font/woff2;base64,")
            .unwrap();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(uri)
            .unwrap();
        assert_eq!(&decoded[..4], b"wOF2");
    }

    #[test]
    fn writes_font_face_rules() {
        let mut codepoints = BTreeSet::new();
        codepoints.insert('A');
        let face = embed(SANS_REGULAR, &codepoints).unwrap();
        let mut output = String::new();
        write_font_faces(&mut output, &[face]).unwrap();

        assert!(output.starts_with(
            "@font-face{font-family:'Asg Test Sans';font-style:normal;font-weight:400;"
        ));
        assert!(output.contains("format('woff2')}"));
    }

    #[test]
    fn escapes_single_quotes_in_family() {
        let mut output = String::new();
        write_font_faces(
            &mut output,
            &[EmbeddedFace {
                family: "Evil 'Font'".to_owned(),
                italic: true,
                weight: 700,
                format: "woff2",
                data_uri: "data:x".to_owned(),
            }],
        )
        .unwrap();

        assert!(output.contains("font-family:'Evil \\'Font\\''"));
    }

    #[test]
    fn font_family_remains_css_text_in_valid_svg() {
        let mut output = "<svg xmlns=\"http://www.w3.org/2000/svg\"><style>".to_owned();
        write_font_faces(
            &mut output,
            &[EmbeddedFace {
                family: "Asg & </style> 'Sans' \\ Font\nLine\rBreak\u{c}".to_owned(),
                italic: false,
                weight: 400,
                format: "woff2",
                data_uri: "data:font/woff2;base64,AA==".to_owned(),
            }],
        )
        .unwrap();
        output.push_str("</style></svg>");

        let document = roxmltree::Document::parse(&output).unwrap();
        let style = document.root_element().first_element_child().unwrap();
        assert!(style.next_sibling_element().is_none());
        assert!(
            style.text().unwrap().contains(
                "font-family:'Asg & </style> \\'Sans\\' \\\\ Font\\a Line\\d Break\\c ';"
            )
        );
    }
}
