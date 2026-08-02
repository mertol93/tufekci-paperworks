use super::{PdfBookmarkEntry, PrintedContentsOptions, MAX_BOOKMARK_DEPTH};
use crate::job_control::PdfJobExecutionControl;
use lopdf::content::{Content, Operation};
use lopdf::{
    decode_text_string, dictionary, text_string, Dictionary, Document, Object, ObjectId, Stream,
    StringFormat,
};
use sha2::{Digest, Sha256};
use skrifa::{
    charmap::Charmap,
    instance::{LocationRef, Size},
    metrics::{GlyphMetrics, Metrics},
    FontRef, GlyphId, MetadataProvider,
};
use std::collections::{BTreeMap, HashMap};

const CONTENTS_FONT_BYTES: &[u8] = include_bytes!("../../assets/fonts/LiberationSans-Regular.ttf");
const CONTENTS_FONT_RESOURCE: &[u8] = b"PWContentsFont";
const CONTENTS_PAGE_WIDTH: f32 = 595.0;
const CONTENTS_PAGE_HEIGHT: f32 = 842.0;
const CONTENTS_MARGIN: f32 = 54.0;
const CONTENTS_ENTRY_START_Y: f32 = 711.0;
const CONTENTS_ENTRY_LINE_HEIGHT: f32 = 16.5;
const CONTENTS_ENTRIES_PER_PAGE: usize = 38;
const MAX_PRINTED_CONTENTS_PAGES: usize = 64;
const MAX_PRINTED_TITLE_CHARACTERS: usize = 128;
const MAX_PRINTED_TITLE_BYTES: usize = 512;

pub(super) struct PrintedContentsOutcome {
    pub(super) output_bookmarks: Vec<PdfBookmarkEntry>,
    pub(super) verification: PrintedContentsVerification,
    pub(super) warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub(super) struct PrintedContentsVerification {
    pub(super) page_count: usize,
    pub(super) entry_count: usize,
    title: String,
    destinations: Vec<u32>,
    content_sha256: Vec<[u8; 32]>,
}

struct PrintedEntry<'a> {
    bookmark: &'a PdfBookmarkEntry,
    destination_id: ObjectId,
    output_page_number: u32,
}

struct LinkDraft {
    destination_id: ObjectId,
    rectangle: [f32; 4],
}

struct ContentsPageDraft {
    content: Vec<u8>,
    entry_count: usize,
    links: Vec<LinkDraft>,
}

struct ContentsFont<'a> {
    charmap: Charmap<'a>,
    glyph_metrics: GlyphMetrics<'a>,
    metrics: Metrics,
    glyph_to_unicode: BTreeMap<u16, char>,
    substituted_characters: usize,
}

pub(super) fn validate_printed_contents_options(
    options: Option<&PrintedContentsOptions>,
    bookmarks: &[PdfBookmarkEntry],
) -> Result<(), String> {
    let Some(options) = options else {
        return Ok(());
    };
    let title = options.title.trim();
    if title.is_empty() {
        return Err("Printed contents pages need a title.".to_string());
    }
    if title.chars().count() > MAX_PRINTED_TITLE_CHARACTERS || title.len() > MAX_PRINTED_TITLE_BYTES
    {
        return Err(format!(
            "The printed contents title must contain at most {MAX_PRINTED_TITLE_CHARACTERS} characters."
        ));
    }
    if title.chars().any(char::is_control) {
        return Err("The printed contents title cannot contain control characters.".to_string());
    }
    if options.maximum_level > MAX_BOOKMARK_DEPTH {
        return Err(format!(
            "Printed contents can include at most level {} bookmarks.",
            MAX_BOOKMARK_DEPTH + 1
        ));
    }
    let entry_count = bookmarks
        .iter()
        .filter(|bookmark| bookmark.level <= options.maximum_level)
        .count();
    if entry_count == 0 {
        return Err(
            "Printed contents pages need at least one bookmark at the selected levels.".to_string(),
        );
    }
    let page_count = entry_count.div_ceil(CONTENTS_ENTRIES_PER_PAGE);
    if page_count > MAX_PRINTED_CONTENTS_PAGES {
        return Err(format!(
            "Printed contents can contain at most {MAX_PRINTED_CONTENTS_PAGES} pages. Include fewer bookmark levels."
        ));
    }
    Ok(())
}

pub(super) fn add_printed_contents_pages(
    document: &mut Document,
    source_page_count: usize,
    bookmarks: &[PdfBookmarkEntry],
    options: &PrintedContentsOptions,
    control: &PdfJobExecutionControl,
) -> Result<PrintedContentsOutcome, String> {
    validate_printed_contents_options(Some(options), bookmarks)?;
    control.checkpoint(32, "Preparing printed contents pages")?;
    let title = options.title.trim().to_string();
    let source_pages = document.get_pages();
    if source_pages.len() != source_page_count {
        return Err(
            "The source page tree changed before printed contents were generated.".to_string(),
        );
    }
    let selected = bookmarks
        .iter()
        .filter(|bookmark| bookmark.level <= options.maximum_level)
        .map(|bookmark| {
            let source_page_number = bookmark
                .page_number
                .ok_or_else(|| "A printed contents entry has no page destination.".to_string())?;
            let destination_id =
                source_pages
                    .get(&source_page_number)
                    .copied()
                    .ok_or_else(|| {
                        "A printed contents entry points outside the source PDF.".to_string()
                    })?;
            Ok((bookmark, source_page_number, destination_id))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let page_count = selected.len().div_ceil(CONTENTS_ENTRIES_PER_PAGE);
    let entries = selected
        .into_iter()
        .map(
            |(bookmark, source_page_number, destination_id)| PrintedEntry {
                bookmark,
                destination_id,
                output_page_number: source_page_number + page_count as u32,
            },
        )
        .collect::<Vec<_>>();

    let mut font = ContentsFont::new()?;
    let mut page_drafts = Vec::with_capacity(page_count);
    for (page_index, page_entries) in entries.chunks(CONTENTS_ENTRIES_PER_PAGE).enumerate() {
        let progress = 34 + (((page_index + 1) * 13) / page_count.max(1)) as u8;
        control.checkpoint(
            progress,
            format!(
                "Building printed contents page {} of {page_count}",
                page_index + 1
            ),
        )?;
        page_drafts.push(render_contents_page(
            &mut font,
            &title,
            page_index,
            page_count,
            page_entries,
        )?);
    }
    let content_sha256 = page_drafts
        .iter()
        .map(|draft| Sha256::digest(&draft.content).into())
        .collect::<Vec<[u8; 32]>>();
    let font_id = font.install(document)?;
    let pages_root_id = document
        .catalog()
        .and_then(|catalog| catalog.get(b"Pages"))
        .and_then(Object::as_reference)
        .map_err(|_| "The PDF catalogue has no valid page-tree root.".to_string())?;
    let mut generated_page_ids = Vec::with_capacity(page_count);
    for (page_index, draft) in page_drafts.into_iter().enumerate() {
        let annotation_ids = draft
            .links
            .into_iter()
            .map(|link| {
                document.add_object(dictionary! {
                    "Type" => "Annot",
                    "Subtype" => "Link",
                    "Rect" => link.rectangle.into_iter().map(pdf_real).collect::<Vec<_>>(),
                    "Border" => vec![0.into(), 0.into(), 0.into()],
                    "Dest" => vec![Object::Reference(link.destination_id), Object::Name(b"Fit".to_vec())],
                    "F" => 4,
                })
            })
            .collect::<Vec<_>>();
        let content_id = document.add_object(Stream::new(dictionary! {}, draft.content));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_root_id,
            "MediaBox" => vec![0.into(), 0.into(), pdf_real(CONTENTS_PAGE_WIDTH), pdf_real(CONTENTS_PAGE_HEIGHT)],
            "Resources" => dictionary! {
                "Font" => dictionary! { CONTENTS_FONT_RESOURCE => font_id },
                "ProcSet" => vec![Object::Name(b"PDF".to_vec()), Object::Name(b"Text".to_vec())],
            },
            "Contents" => content_id,
            "Annots" => annotation_ids.into_iter().map(Object::Reference).collect::<Vec<_>>(),
            "Tabs" => "S",
            "PaperworksPrintedContents" => true,
            "PaperworksContentsIndex" => (page_index + 1) as i64,
            "PaperworksContentsCount" => page_count as i64,
            "PaperworksContentsEntryCount" => draft.entry_count as i64,
            "PaperworksContentsTitle" => text_string(&title),
        });
        generated_page_ids.push(page_id);
    }
    prepend_pages_to_tree(
        document,
        pages_root_id,
        &generated_page_ids,
        source_page_count,
    )?;

    let mut output_bookmarks = bookmarks
        .iter()
        .cloned()
        .map(|mut bookmark| {
            bookmark.page_number = bookmark
                .page_number
                .map(|page_number| page_number + page_count as u32);
            bookmark
        })
        .collect::<Vec<_>>();
    if options.add_bookmark {
        output_bookmarks.insert(
            0,
            PdfBookmarkEntry {
                title: title.clone(),
                page_number: Some(1),
                level: 0,
                bold: true,
                italic: false,
                open: true,
                colour: [0.09, 0.31, 0.65],
            },
        );
    }

    let mut warnings = vec![format!(
        "Added {page_count} printed contents page{} with {} linked entr{}; source pages moved forward by {page_count}.",
        if page_count == 1 { "" } else { "s" },
        entries.len(),
        if entries.len() == 1 { "y" } else { "ies" }
    )];
    warnings.push(
        "Printed contents pages are not tagged. Run PDF/UA checks before claiming an accessible or standards-conforming copy."
            .to_string(),
    );
    if document
        .catalog()
        .is_ok_and(|catalog| catalog.has(b"PageLabels"))
    {
        warnings.push(
            "The contents entries use physical output page numbers; existing custom page labels are preserved but not printed."
                .to_string(),
        );
    }
    if font.substituted_characters > 0 {
        warnings.push(format!(
            "{} unsupported contents character{} were replaced with question marks in the printed pages; the bookmark titles remain unchanged.",
            font.substituted_characters,
            if font.substituted_characters == 1 { "" } else { "s" }
        ));
    }

    Ok(PrintedContentsOutcome {
        output_bookmarks,
        verification: PrintedContentsVerification {
            page_count,
            entry_count: entries.len(),
            title,
            destinations: entries
                .iter()
                .map(|entry| entry.output_page_number)
                .collect(),
            content_sha256,
        },
        warnings,
    })
}

pub(super) fn verify_printed_contents(
    document: &Document,
    expected: &PrintedContentsVerification,
) -> Result<(), String> {
    let pages = document.get_pages();
    let pages_by_id = pages
        .iter()
        .map(|(number, id)| (*id, *number))
        .collect::<HashMap<_, _>>();
    for page_index in 0..expected.page_count {
        let page_number = (page_index + 1) as u32;
        let page_id = pages.get(&page_number).copied().ok_or_else(|| {
            "A generated printed contents page disappeared during verification.".to_string()
        })?;
        let page = document
            .get_object(page_id)
            .and_then(Object::as_dict)
            .map_err(|_| "A generated printed contents page is malformed.".to_string())?;
        if !matches!(
            page.get(b"PaperworksPrintedContents"),
            Ok(Object::Boolean(true))
        ) {
            return Err(
                "A generated printed contents page lost its verification marker.".to_string(),
            );
        }
        require_integer(page, b"PaperworksContentsIndex", page_index + 1)?;
        require_integer(page, b"PaperworksContentsCount", expected.page_count)?;
        let expected_destinations = expected
            .destinations
            .chunks(CONTENTS_ENTRIES_PER_PAGE)
            .nth(page_index)
            .unwrap_or_default();
        require_integer(
            page,
            b"PaperworksContentsEntryCount",
            expected_destinations.len(),
        )?;
        let title = page
            .get(b"PaperworksContentsTitle")
            .ok()
            .and_then(|value| decode_text_string(value).ok())
            .ok_or_else(|| "A generated contents title could not be decoded.".to_string())?;
        if title != expected.title {
            return Err("The generated contents title changed during verification.".to_string());
        }
        verify_page_font(document, page)?;
        let rendered_content = page
            .get(b"Contents")
            .ok()
            .and_then(|contents| decoded_stream_content(document, contents))
            .ok_or_else(|| {
                "A generated contents page has no decodable rendered text stream.".to_string()
            })?;
        let expected_content_sha256 = expected.content_sha256.get(page_index).ok_or_else(|| {
            "A generated contents text-stream expectation is missing.".to_string()
        })?;
        if <[u8; 32]>::from(Sha256::digest(&rendered_content)) != *expected_content_sha256 {
            return Err(
                "A generated contents text stream changed during verification.".to_string(),
            );
        }
        let annotations = page
            .get(b"Annots")
            .ok()
            .and_then(|value| resolved_array(document, value).ok())
            .ok_or_else(|| "A generated contents page has no link list.".to_string())?;
        if annotations.len() != expected_destinations.len() {
            return Err("A generated contents page changed its link count.".to_string());
        }
        for (annotation, expected_destination) in
            annotations.iter().zip(expected_destinations.iter())
        {
            let annotation = resolved_dictionary(document, annotation)
                .map_err(|_| "A generated contents link is malformed.".to_string())?;
            if !matches!(
                annotation.get(b"Subtype").and_then(Object::as_name),
                Ok(name) if name == b"Link"
            ) {
                return Err("A generated contents entry is not a PDF link.".to_string());
            }
            let destination = annotation
                .get(b"Dest")
                .and_then(Object::as_array)
                .map_err(|_| "A generated contents link has no destination.".to_string())?;
            let destination_id = destination
                .first()
                .and_then(|value| value.as_reference().ok())
                .ok_or_else(|| "A generated contents link target is malformed.".to_string())?;
            if !matches!(
                destination.get(1).and_then(|value| value.as_name().ok()),
                Some(name) if name == b"Fit"
            ) {
                return Err("A generated contents link lost its whole-page Fit target.".to_string());
            }
            if pages_by_id.get(&destination_id) != Some(expected_destination) {
                return Err("A generated contents link points to the wrong page.".to_string());
            }
        }
    }
    if pages
        .get(&((expected.page_count + 1) as u32))
        .and_then(|page_id| document.get_object(*page_id).ok())
        .and_then(|page| page.as_dict().ok())
        .is_some_and(|page| page.has(b"PaperworksPrintedContents"))
    {
        return Err("A source page was incorrectly marked as printed contents.".to_string());
    }
    Ok(())
}

fn render_contents_page(
    font: &mut ContentsFont<'_>,
    title: &str,
    page_index: usize,
    page_count: usize,
    entries: &[PrintedEntry<'_>],
) -> Result<ContentsPageDraft, String> {
    let mut operations = Vec::new();
    let heading = if page_index == 0 {
        title.to_string()
    } else {
        format!("{title} (continued)")
    };
    let heading = font.fit_text(&heading, 22.0, CONTENTS_PAGE_WIDTH - 2.0 * CONTENTS_MARGIN);
    push_text(
        &mut operations,
        font,
        &heading,
        22.0,
        CONTENTS_MARGIN,
        775.0,
        [0.09, 0.31, 0.65],
    );
    operations.extend([
        Operation::new("RG", vec![pdf_real(0.73), pdf_real(0.79), pdf_real(0.88)]),
        Operation::new("w", vec![pdf_real(0.8)]),
        Operation::new("m", vec![pdf_real(CONTENTS_MARGIN), pdf_real(748.0)]),
        Operation::new(
            "l",
            vec![
                pdf_real(CONTENTS_PAGE_WIDTH - CONTENTS_MARGIN),
                pdf_real(748.0),
            ],
        ),
        Operation::new("S", vec![]),
    ]);

    let mut links = Vec::with_capacity(entries.len());
    for (entry_index, entry) in entries.iter().enumerate() {
        let baseline = CONTENTS_ENTRY_START_Y - entry_index as f32 * CONTENTS_ENTRY_LINE_HEIGHT;
        let indent = entry.bookmark.level as f32 * 14.0;
        let title_x = CONTENTS_MARGIN + indent;
        let page_label = entry.output_page_number.to_string();
        let page_width = font.text_width(&page_label, 10.5);
        let page_x = CONTENTS_PAGE_WIDTH - CONTENTS_MARGIN - page_width;
        let maximum_title_width = (page_x - title_x - 18.0).max(40.0);
        let printable_title = normalise_printed_text(&entry.bookmark.title);
        let visible_title = font.fit_text(&printable_title, 10.5, maximum_title_width);
        let title_width = font.text_width(&visible_title, 10.5);
        let colour = if entry.bookmark.level == 0 {
            [0.09, 0.31, 0.65]
        } else {
            [0.12, 0.14, 0.18]
        };
        push_text(
            &mut operations,
            font,
            &visible_title,
            10.5,
            title_x,
            baseline,
            colour,
        );
        push_text(
            &mut operations,
            font,
            &page_label,
            10.5,
            page_x,
            baseline,
            [0.12, 0.14, 0.18],
        );
        let leader_start = title_x + title_width + 5.0;
        let leader_end = page_x - 5.0;
        if leader_end > leader_start {
            operations.extend([
                Operation::new("RG", vec![pdf_real(0.72), pdf_real(0.74), pdf_real(0.78)]),
                Operation::new("w", vec![pdf_real(0.35)]),
                Operation::new("m", vec![pdf_real(leader_start), pdf_real(baseline + 2.0)]),
                Operation::new("l", vec![pdf_real(leader_end), pdf_real(baseline + 2.0)]),
                Operation::new("S", vec![]),
            ]);
        }
        links.push(LinkDraft {
            destination_id: entry.destination_id,
            rectangle: [
                CONTENTS_MARGIN - 3.0,
                baseline - 3.0,
                CONTENTS_PAGE_WIDTH - CONTENTS_MARGIN + 3.0,
                baseline + 11.0,
            ],
        });
    }
    let footer = format!("Contents page {} of {page_count}", page_index + 1);
    let footer_width = font.text_width(&footer, 8.5);
    push_text(
        &mut operations,
        font,
        &footer,
        8.5,
        CONTENTS_PAGE_WIDTH - CONTENTS_MARGIN - footer_width,
        34.0,
        [0.36, 0.39, 0.44],
    );
    let content = Content { operations }
        .encode()
        .map_err(|error| format!("Printed contents text could not be encoded: {error}"))?;
    Ok(ContentsPageDraft {
        content,
        entry_count: entries.len(),
        links,
    })
}

fn prepend_pages_to_tree(
    document: &mut Document,
    pages_root_id: ObjectId,
    generated_page_ids: &[ObjectId],
    source_page_count: usize,
) -> Result<(), String> {
    let existing_kids = document
        .get_object(pages_root_id)
        .and_then(Object::as_dict)
        .and_then(|pages| pages.get(b"Kids"))
        .and_then(Object::as_array)
        .cloned()
        .map_err(|_| "The PDF page-tree root has no valid child list.".to_string())?;
    let mut kids = generated_page_ids
        .iter()
        .copied()
        .map(Object::Reference)
        .collect::<Vec<_>>();
    kids.extend(existing_kids);
    let pages = document
        .get_object_mut(pages_root_id)
        .and_then(Object::as_dict_mut)
        .map_err(|_| "The PDF page-tree root could not be updated.".to_string())?;
    pages.set("Kids", kids);
    pages.set(
        "Count",
        (source_page_count + generated_page_ids.len()) as i64,
    );
    Ok(())
}

impl<'a> ContentsFont<'a> {
    fn new() -> Result<Self, String> {
        let font = FontRef::new(CONTENTS_FONT_BYTES)
            .map_err(|_| "The bundled printed-contents font is invalid.".to_string())?;
        let metrics = font.metrics(Size::unscaled(), LocationRef::default());
        if metrics.units_per_em == 0 || metrics.bounds.is_none() {
            return Err("The bundled printed-contents font has no usable metrics.".to_string());
        }
        Ok(Self {
            charmap: font.charmap(),
            glyph_metrics: font.glyph_metrics(Size::unscaled(), LocationRef::default()),
            metrics,
            glyph_to_unicode: BTreeMap::new(),
            substituted_characters: 0,
        })
    }

    fn fit_text(&self, text: &str, size: f32, maximum_width: f32) -> String {
        if self.text_width(text, size) <= maximum_width {
            return text.to_string();
        }
        let ellipsis = "…";
        let mut characters = text.chars().collect::<Vec<_>>();
        while !characters.is_empty() {
            characters.pop();
            let mut candidate = characters.iter().collect::<String>();
            candidate.push_str(ellipsis);
            if self.text_width(&candidate, size) <= maximum_width {
                return candidate;
            }
        }
        ellipsis.to_string()
    }

    fn text_width(&self, text: &str, size: f32) -> f32 {
        let units = text
            .chars()
            .map(|character| {
                let (glyph, _, _) = self.glyph_for_character(character);
                self.glyph_metrics.advance_width(glyph).unwrap_or(0.0)
            })
            .sum::<f32>();
        units * size / self.metrics.units_per_em as f32
    }

    fn encode_text(&mut self, text: &str) -> Vec<u8> {
        let mut encoded = Vec::with_capacity(text.len() * 2);
        for character in text.chars() {
            let (glyph, mapped_character, substituted) = self.glyph_for_character(character);
            if substituted {
                self.substituted_characters += 1;
            }
            self.glyph_to_unicode
                .entry(glyph.to_u32() as u16)
                .or_insert(mapped_character);
            encoded.extend_from_slice(&(glyph.to_u32() as u16).to_be_bytes());
        }
        encoded
    }

    fn glyph_for_character(&self, character: char) -> (GlyphId, char, bool) {
        if let Some(glyph) = self
            .charmap
            .map(character)
            .filter(|glyph| glyph.to_u32() <= u16::MAX as u32)
        {
            return (glyph, character, false);
        }
        let fallback = self
            .charmap
            .map('?')
            .filter(|glyph| glyph.to_u32() <= u16::MAX as u32)
            .unwrap_or_else(|| GlyphId::new(0));
        (fallback, '?', true)
    }

    fn install(&self, document: &mut Document) -> Result<ObjectId, String> {
        if self.glyph_to_unicode.is_empty() {
            return Err("The printed contents font was not used.".to_string());
        }
        let font_file_id = document.add_object(Stream::new(
            dictionary! { "Length1" => CONTENTS_FONT_BYTES.len() as i64 },
            CONTENTS_FONT_BYTES.to_vec(),
        ));
        let units_per_em = self.metrics.units_per_em as f32;
        let scale = |value: f32| (value * 1000.0 / units_per_em).round() as i64;
        let bounds = self
            .metrics
            .bounds
            .ok_or_else(|| "The bundled printed-contents font has no bounds.".to_string())?;
        let descriptor_id = document.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "LiberationSans",
            "Flags" => 32,
            "FontBBox" => vec![
                Object::Integer(scale(bounds.x_min)),
                Object::Integer(scale(bounds.y_min)),
                Object::Integer(scale(bounds.x_max)),
                Object::Integer(scale(bounds.y_max)),
            ],
            "ItalicAngle" => 0,
            "Ascent" => scale(self.metrics.ascent),
            "Descent" => scale(self.metrics.descent),
            "CapHeight" => scale(self.metrics.cap_height.unwrap_or(self.metrics.ascent)),
            "StemV" => 80,
            "MissingWidth" => 600,
            "FontFile2" => font_file_id,
        });
        let widths = self
            .glyph_to_unicode
            .keys()
            .flat_map(|glyph_id| {
                let advance = self
                    .glyph_metrics
                    .advance_width(GlyphId::new(*glyph_id as u32))
                    .unwrap_or(units_per_em);
                let width = (advance * 1000.0 / units_per_em).round() as i64;
                [
                    Object::Integer(*glyph_id as i64),
                    Object::Array(vec![Object::Integer(width)]),
                ]
            })
            .collect::<Vec<_>>();
        let descendant_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "CIDFontType2",
            "BaseFont" => "LiberationSans",
            "CIDSystemInfo" => dictionary! {
                "Registry" => Object::string_literal("Adobe"),
                "Ordering" => Object::string_literal("Identity"),
                "Supplement" => 0,
            },
            "FontDescriptor" => descriptor_id,
            "DW" => 1000,
            "W" => widths,
            "CIDToGIDMap" => "Identity",
        });
        let to_unicode_id = document.add_object(Stream::new(
            dictionary! {},
            build_to_unicode_map(&self.glyph_to_unicode).into_bytes(),
        ));
        Ok(document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => "LiberationSans",
            "Encoding" => "Identity-H",
            "DescendantFonts" => vec![Object::Reference(descendant_id)],
            "ToUnicode" => to_unicode_id,
        }))
    }
}

fn push_text(
    operations: &mut Vec<Operation>,
    font: &mut ContentsFont<'_>,
    text: &str,
    size: f32,
    x: f32,
    y: f32,
    colour: [f32; 3],
) {
    operations.push(Operation::new(
        "rg",
        colour.into_iter().map(pdf_real).collect(),
    ));
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![
            Object::Name(CONTENTS_FONT_RESOURCE.to_vec()),
            pdf_real(size),
        ],
    ));
    operations.push(Operation::new(
        "Tm",
        vec![
            1.into(),
            0.into(),
            0.into(),
            1.into(),
            pdf_real(x),
            pdf_real(y),
        ],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::String(
            font.encode_text(text),
            StringFormat::Hexadecimal,
        )],
    ));
    operations.push(Operation::new("ET", vec![]));
}

fn build_to_unicode_map(glyphs: &BTreeMap<u16, char>) -> String {
    let mut map = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /PaperworksContents-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries = glyphs.iter().collect::<Vec<_>>();
    for chunk in entries.chunks(100) {
        map.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (glyph, character) in chunk {
            map.push_str(&format!("<{glyph:04X}> <{}>\n", unicode_hex(**character)));
        }
        map.push_str("endbfchar\n");
    }
    map.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    map
}

fn unicode_hex(character: char) -> String {
    let mut buffer = [0_u16; 2];
    character
        .encode_utf16(&mut buffer)
        .iter()
        .map(|unit| format!("{unit:04X}"))
        .collect()
}

fn normalise_printed_text(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut previous_space = false;
    for character in value.chars() {
        let character = if character.is_control() {
            ' '
        } else {
            character
        };
        if character.is_whitespace() {
            if !previous_space {
                result.push(' ');
            }
            previous_space = true;
        } else {
            result.push(character);
            previous_space = false;
        }
    }
    result.trim().to_string()
}

fn verify_page_font(document: &Document, page: &Dictionary) -> Result<(), String> {
    let resources = page
        .get(b"Resources")
        .map_err(|_| "A generated contents page has no resources.".to_string())?;
    let resources = resolved_dictionary(document, resources)
        .map_err(|_| "A generated contents resource dictionary is malformed.".to_string())?;
    let fonts = resources
        .get(b"Font")
        .map_err(|_| "A generated contents page has no font dictionary.".to_string())?;
    let fonts = resolved_dictionary(document, fonts)
        .map_err(|_| "A generated contents font dictionary is malformed.".to_string())?;
    let font = fonts
        .get(CONTENTS_FONT_RESOURCE)
        .map_err(|_| "The embedded contents font is missing.".to_string())?;
    let font = resolved_dictionary(document, font)
        .map_err(|_| "The embedded contents font is malformed.".to_string())?;
    if !matches!(
        font.get(b"Subtype").and_then(Object::as_name),
        Ok(name) if name == b"Type0"
    ) {
        return Err(
            "The generated contents font lost its Unicode or embedded-font structure.".to_string(),
        );
    }
    if !font
        .get(b"ToUnicode")
        .is_ok_and(|value| object_has_non_empty_stream(document, value))
    {
        return Err("The generated contents font has no usable Unicode map.".to_string());
    }
    let descendants = font
        .get(b"DescendantFonts")
        .ok()
        .and_then(|value| resolved_array(document, value).ok())
        .ok_or_else(|| "The generated contents font has no descendant font.".to_string())?;
    let descendant = descendants
        .first()
        .ok_or_else(|| "The generated contents descendant font is missing.".to_string())?;
    let descendant = resolved_dictionary(document, descendant)
        .map_err(|_| "The generated contents descendant font is malformed.".to_string())?;
    if !matches!(
        descendant.get(b"Subtype").and_then(Object::as_name),
        Ok(name) if name == b"CIDFontType2"
    ) {
        return Err("The generated contents font is not embedded TrueType text.".to_string());
    }
    let descriptor = descendant
        .get(b"FontDescriptor")
        .map_err(|_| "The generated contents font descriptor is missing.".to_string())?;
    let descriptor = resolved_dictionary(document, descriptor)
        .map_err(|_| "The generated contents font descriptor is malformed.".to_string())?;
    if !descriptor
        .get(b"FontFile2")
        .is_ok_and(|value| object_has_non_empty_stream(document, value))
    {
        return Err("The generated contents TrueType font file is missing.".to_string());
    }
    Ok(())
}

fn resolved_dictionary<'a>(
    document: &'a Document,
    object: &'a Object,
) -> Result<&'a Dictionary, lopdf::Error> {
    match object {
        Object::Reference(id) => document.get_object(*id)?.as_dict(),
        _ => object.as_dict(),
    }
}

fn resolved_array<'a>(
    document: &'a Document,
    object: &'a Object,
) -> Result<&'a Vec<Object>, lopdf::Error> {
    match object {
        Object::Reference(id) => document.get_object(*id)?.as_array(),
        _ => object.as_array(),
    }
}

fn object_has_non_empty_stream(document: &Document, object: &Object) -> bool {
    match object {
        Object::Reference(id) => document
            .get_object(*id)
            .is_ok_and(|value| object_has_non_empty_stream(document, value)),
        Object::Stream(stream) => !stream.content.is_empty(),
        Object::Array(values) => values
            .iter()
            .any(|value| object_has_non_empty_stream(document, value)),
        _ => false,
    }
}

fn decoded_stream_content(document: &Document, object: &Object) -> Option<Vec<u8>> {
    match object {
        Object::Reference(id) => document
            .get_object(*id)
            .ok()
            .and_then(|value| decoded_stream_content(document, value)),
        Object::Stream(stream) => stream.decompressed_content().ok(),
        _ => None,
    }
}

fn require_integer(dictionary: &Dictionary, key: &[u8], expected: usize) -> Result<(), String> {
    let value = dictionary
        .get(key)
        .and_then(Object::as_i64)
        .map_err(|_| "A generated contents verification count is malformed.".to_string())?;
    if value != expected as i64 {
        return Err("A generated contents verification count changed.".to_string());
    }
    Ok(())
}

fn pdf_real(value: f32) -> Object {
    Object::Real(value)
}
