use crate::health::inspect_pdf_print_resources;
use crate::job_control::PdfJobExecutionControl;
use lopdf::{decode_text_string, Dictionary, Document, LoadOptions, Object, ObjectId};
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use std::collections::HashSet;
use std::io::Cursor;
use std::path::Path;

const MAX_OBJECT_STREAM_DECOMPRESSION: usize = 64 * 1024 * 1024;
const MAX_XMP_BYTES: usize = 16 * 1024 * 1024;
const MAX_FEATURE_NODES: usize = 2_000_000;
const MAX_EXAMPLES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PdfXProfile {
    PdfX1a2001,
    PdfX3_2002,
    PdfX4,
}

impl PdfXProfile {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::PdfX1a2001 => "PDF/X-1a:2001",
            Self::PdfX3_2002 => "PDF/X-3:2002",
            Self::PdfX4 => "PDF/X-4",
        }
    }

    fn declaration_matches(self, versions: &[String], conformance: &[String]) -> bool {
        let versions = versions
            .iter()
            .map(|value| compact(value))
            .collect::<Vec<_>>();
        let conformance = conformance
            .iter()
            .map(|value| compact(value))
            .collect::<Vec<_>>();
        match self {
            Self::PdfX1a2001 => {
                versions.iter().any(|value| value == "PDFX12001")
                    && conformance.iter().any(|value| value == "PDFX1A2001")
            }
            Self::PdfX3_2002 => versions.iter().any(|value| value == "PDFX32002"),
            Self::PdfX4 => versions.iter().any(|value| value == "PDFX4"),
        }
    }
}

pub(crate) struct PdfXPreflightFailure {
    pub(crate) code: &'static str,
    pub(crate) title: &'static str,
    pub(crate) description: String,
}

pub(crate) struct PdfXPreflightResult {
    pub(crate) failed_checks: u64,
    pub(crate) failures: Vec<PdfXPreflightFailure>,
    pub(crate) passed_checks: u64,
}

struct PreflightCheck {
    code: &'static str,
    title: &'static str,
    passed: bool,
    failure: String,
}

#[derive(Default)]
struct DeclarationAudit {
    conformance: Vec<String>,
    trapped: Vec<String>,
    versions: Vec<String>,
    xmp_error: Option<String>,
}

#[derive(Default)]
struct OutputIntentAudit {
    matching_intents: usize,
    issues: Vec<String>,
}

#[derive(Default)]
struct FeatureAudit {
    alternate_images: usize,
    attachments: usize,
    external_content: usize,
    forms: usize,
    incomplete: bool,
    javascript: usize,
    lzw_filters: usize,
    non_printing_media: usize,
    transfer_curves: usize,
    xfa: usize,
}

#[derive(Clone, Copy)]
struct PageBox {
    left: f64,
    bottom: f64,
    right: f64,
    top: f64,
}

pub(crate) fn run_pdfx_preflight(
    candidate: &Path,
    profile: PdfXProfile,
    source_was_encrypted: bool,
    control: &PdfJobExecutionControl,
) -> Result<PdfXPreflightResult, String> {
    control.checkpoint(2, "Opening the PDF/X preflight candidate")?;
    let document = Document::load_with_options(
        candidate,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|_| "The PDF/X preflight candidate is not a readable PDF.".to_string())?;
    if document.is_encrypted() {
        return Err("The private PDF/X preflight candidate is unexpectedly encrypted.".to_string());
    }
    let pages = document.get_pages();
    if pages.is_empty() {
        return Err("The PDF/X preflight candidate contains no readable pages.".to_string());
    }

    control.checkpoint(9, "Checking PDF/X declarations")?;
    let declarations = inspect_declarations(&document);
    let declaration_ok = profile
        .declaration_matches(&declarations.versions, &declarations.conformance)
        && declarations.xmp_error.is_none();
    let declared_values = declarations
        .versions
        .iter()
        .chain(declarations.conformance.iter())
        .cloned()
        .collect::<Vec<_>>();
    let mut checks = vec![PreflightCheck {
        code: "declaration",
        title: "Conformance declaration",
        passed: declaration_ok,
        failure: if let Some(error) = declarations.xmp_error {
            format!("The XMP identification metadata could not be parsed safely: {error}")
        } else if declared_values.is_empty() {
            format!(
                "The document does not declare the selected {} profile.",
                profile.label()
            )
        } else {
            format!(
                "The document declares {} instead of the selected {} profile.",
                joined_examples(&declared_values),
                profile.label()
            )
        },
    }];

    let trapped_ok = declarations
        .trapped
        .iter()
        .any(|value| matches!(compact(value).as_str(), "TRUE" | "FALSE"));
    checks.push(PreflightCheck {
        code: "trapping",
        title: "Trapping declaration",
        passed: trapped_ok,
        failure: "The document metadata must state whether the file has been trapped using True or False."
            .to_string(),
    });
    checks.push(PreflightCheck {
        code: "encryption",
        title: "Password protection",
        passed: !source_was_encrypted,
        failure: "The original source uses password protection or encryption, which PDF/X does not permit."
            .to_string(),
    });

    control.checkpoint(18, "Checking PDF/X output intent")?;
    let output_intent = inspect_output_intents(&document);
    checks.push(PreflightCheck {
        code: "output-intent",
        title: "Print output intent",
        passed: output_intent.matching_intents > 0 && output_intent.issues.is_empty(),
        failure: if output_intent.matching_intents == 0 {
            "No complete embedded GTS_PDFX output intent was found.".to_string()
        } else {
            format!(
                "The PDF/X output intent needs review: {}.",
                joined_examples(&output_intent.issues)
            )
        },
    });

    control.checkpoint(27, "Checking fonts, ICC profiles and object references")?;
    let resources = inspect_pdf_print_resources(
        &document,
        &pages,
        &control.subrange(27, 64, "Print resources".to_string()),
    )?;
    checks.push(PreflightCheck {
        code: "fonts",
        title: "Embedded fonts",
        passed: resources.unembedded_font_count == 0 && !resources.incomplete,
        failure: format!(
            "{} of {} inspected font resources are not embedded, or the bounded font inspection was incomplete.{}",
            resources.unembedded_font_count,
            resources.font_count,
            appended_examples(&resources.examples)
        ),
    });
    checks.push(PreflightCheck {
        code: "icc-profiles",
        title: "ICC profile integrity",
        passed: resources.output_intent_count > 0
            && resources.invalid_icc_profile_count == 0
            && resources.colour_issue_count == 0,
        failure: format!(
            "The colour audit found {} invalid ICC profiles and {} bounded colour-structure issues.{}",
            resources.invalid_icc_profile_count,
            resources.colour_issue_count,
            appended_examples(&resources.examples)
        ),
    });
    checks.push(PreflightCheck {
        code: "object-integrity",
        title: "Object and resource integrity",
        passed: resources.broken_reference_count == 0
            && resources.resource_issue_count == 0
            && !resources.incomplete,
        failure: format!(
            "The structural audit found {} broken references and {} page or Form resource issues, or reached an inspection limit.{}",
            resources.broken_reference_count,
            resources.resource_issue_count,
            appended_examples(&resources.examples)
        ),
    });

    control.checkpoint(67, "Checking PDF/X page boxes")?;
    let geometry_issues = inspect_page_boxes(&document, &pages, control)?;
    checks.push(PreflightCheck {
        code: "page-boxes",
        title: "Printable page geometry",
        passed: geometry_issues.is_empty(),
        failure: format!(
            "One or more pages have incomplete or inconsistent MediaBox, TrimBox, ArtBox, CropBox, or BleedBox geometry: {}.",
            joined_examples(&geometry_issues)
        ),
    });

    control.checkpoint(76, "Checking non-printing PDF features")?;
    let features = inspect_features(&document, control)?;
    checks.push(PreflightCheck {
        code: "javascript",
        title: "Executable actions",
        passed: features.javascript == 0 && !features.incomplete,
        failure: format!(
            "The bounded object scan found {} JavaScript structures or could not inspect every object.",
            features.javascript
        ),
    });
    checks.push(PreflightCheck {
        code: "forms",
        title: "Interactive forms",
        passed: features.forms == 0 && features.xfa == 0 && !features.incomplete,
        failure: format!(
            "The bounded object scan found {} AcroForm and {} XFA structures, or could not inspect every object.",
            features.forms, features.xfa
        ),
    });
    checks.push(PreflightCheck {
        code: "self-contained",
        title: "Self-contained print content",
        passed: features.attachments == 0
            && features.external_content == 0
            && features.alternate_images == 0
            && features.lzw_filters == 0
            && !features.incomplete,
        failure: format!(
            "The bounded object scan found {} attachment, {} external-content, {} alternate-image and {} LZW-filter structures, or could not inspect every object.",
            features.attachments,
            features.external_content,
            features.alternate_images,
            features.lzw_filters
        ),
    });
    checks.push(PreflightCheck {
        code: "printable-content",
        title: "Printable-only content",
        passed: features.non_printing_media == 0
            && features.transfer_curves == 0
            && !features.incomplete,
        failure: format!(
            "The bounded object scan found {} audio, video, 3D or rich-media structures and {} transfer-curve structures, or could not inspect every object.",
            features.non_printing_media, features.transfer_curves
        ),
    });

    control.checkpoint(96, "Finalising PDF/X structural preflight")?;
    let failed_checks = checks.iter().filter(|check| !check.passed).count() as u64;
    let passed_checks = checks.len() as u64 - failed_checks;
    let failures = checks
        .into_iter()
        .filter(|check| !check.passed)
        .map(|check| PdfXPreflightFailure {
            code: check.code,
            title: check.title,
            description: check.failure,
        })
        .collect();
    Ok(PdfXPreflightResult {
        failed_checks,
        failures,
        passed_checks,
    })
}

fn inspect_declarations(document: &Document) -> DeclarationAudit {
    let mut audit = DeclarationAudit::default();
    if let Some(info) = document
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|value| resolve_dictionary(document, value).ok())
    {
        collect_dictionary_text(info, b"GTS_PDFXVersion", &mut audit.versions);
        collect_dictionary_text(info, b"GTS_PDFXConformance", &mut audit.conformance);
        collect_dictionary_text(info, b"Trapped", &mut audit.trapped);
    }

    let metadata = document
        .catalog()
        .ok()
        .and_then(|catalogue| catalogue.get(b"Metadata").ok())
        .and_then(|value| resolve_object(document, value).ok())
        .and_then(|value| value.as_stream().ok());
    if let Some(stream) = metadata {
        match stream.decompressed_content_with_limit(MAX_XMP_BYTES) {
            Ok(bytes) => match parse_xmp_fields(&bytes) {
                Ok(xmp) => {
                    audit.versions.extend(xmp.versions);
                    audit.conformance.extend(xmp.conformance);
                    audit.trapped.extend(xmp.trapped);
                }
                Err(error) => audit.xmp_error = Some(error),
            },
            Err(_) => {
                audit.xmp_error = Some("metadata exceeds the 16 MiB decoding boundary".to_string())
            }
        }
    }
    deduplicate(&mut audit.versions);
    deduplicate(&mut audit.conformance);
    deduplicate(&mut audit.trapped);
    audit
}

fn collect_dictionary_text(dictionary: &Dictionary, key: &[u8], output: &mut Vec<String>) {
    if let Ok(value) = dictionary.get(key) {
        if let Some(value) = object_text(value) {
            push_bounded(output, value);
        }
    }
}

fn parse_xmp_fields(bytes: &[u8]) -> Result<DeclarationAudit, String> {
    let mut reader = Reader::from_reader(Cursor::new(bytes));
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut fields = DeclarationAudit::default();
    let mut element_stack: Vec<Option<XmpField>> = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element)) => {
                let field = xmp_field(element.name().as_ref());
                for attribute in element.attributes().with_checks(true) {
                    let attribute =
                        attribute.map_err(|_| "an XMP attribute is malformed".to_string())?;
                    if let Some(field) = xmp_field(attribute.key.as_ref()) {
                        let value = attribute
                            .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
                            .map_err(|_| "an XMP attribute value is malformed".to_string())?;
                        collect_xmp_value(&mut fields, field, value.into_owned());
                    }
                }
                element_stack.push(field);
            }
            Ok(Event::Empty(element)) => {
                for attribute in element.attributes().with_checks(true) {
                    let attribute =
                        attribute.map_err(|_| "an XMP attribute is malformed".to_string())?;
                    if let Some(field) = xmp_field(attribute.key.as_ref()) {
                        let value = attribute
                            .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
                            .map_err(|_| "an XMP attribute value is malformed".to_string())?;
                        collect_xmp_value(&mut fields, field, value.into_owned());
                    }
                }
            }
            Ok(Event::Text(text)) => {
                if let Some(Some(field)) = element_stack.last().copied() {
                    let value = text
                        .decode()
                        .map_err(|_| "XMP text uses an unsupported encoding".to_string())?;
                    collect_xmp_value(&mut fields, field, value.into_owned());
                }
            }
            Ok(Event::End(_)) => {
                element_stack.pop();
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => return Err("the XMP packet is not well-formed XML".to_string()),
        }
        buffer.clear();
    }
    Ok(fields)
}

#[derive(Clone, Copy)]
enum XmpField {
    Version,
    Conformance,
    Trapped,
}

fn xmp_field(name: &[u8]) -> Option<XmpField> {
    let local = name.rsplit(|byte| *byte == b':').next().unwrap_or(name);
    if local.eq_ignore_ascii_case(b"GTS_PDFXVersion") {
        Some(XmpField::Version)
    } else if local.eq_ignore_ascii_case(b"GTS_PDFXConformance") {
        Some(XmpField::Conformance)
    } else if local.eq_ignore_ascii_case(b"Trapped") {
        Some(XmpField::Trapped)
    } else {
        None
    }
}

fn collect_xmp_value(audit: &mut DeclarationAudit, field: XmpField, value: String) {
    match field {
        XmpField::Version => push_bounded(&mut audit.versions, value),
        XmpField::Conformance => push_bounded(&mut audit.conformance, value),
        XmpField::Trapped => push_bounded(&mut audit.trapped, value),
    }
}

fn inspect_output_intents(document: &Document) -> OutputIntentAudit {
    let mut audit = OutputIntentAudit::default();
    let Some(intents) = document
        .catalog()
        .ok()
        .and_then(|catalogue| catalogue.get(b"OutputIntents").ok())
        .and_then(|value| resolve_object(document, value).ok())
        .and_then(|value| value.as_array().ok())
    else {
        return audit;
    };
    for (index, intent) in intents.iter().enumerate() {
        let Some(dictionary) = resolve_dictionary(document, intent).ok() else {
            push_example(
                &mut audit.issues,
                format!("output intent {} is not a dictionary", index + 1),
            );
            continue;
        };
        let pdfx_subtype = dictionary
            .get(b"S")
            .and_then(Object::as_name)
            .is_ok_and(|name| name == b"GTS_PDFX");
        if !pdfx_subtype {
            continue;
        }
        let output_identifier = dictionary
            .get(b"OutputConditionIdentifier")
            .ok()
            .and_then(object_text)
            .is_some_and(|value| !value.trim().is_empty());
        let embedded_profile = dictionary
            .get(b"DestOutputProfile")
            .ok()
            .and_then(|value| resolve_object(document, value).ok())
            .and_then(|value| value.as_stream().ok())
            .is_some();
        if output_identifier && embedded_profile {
            audit.matching_intents += 1;
        } else {
            push_example(
                &mut audit.issues,
                format!(
                    "output intent {} lacks {}",
                    index + 1,
                    if !output_identifier {
                        "an output-condition identifier"
                    } else {
                        "an embedded destination ICC profile"
                    }
                ),
            );
        }
    }
    audit
}

fn inspect_page_boxes(
    document: &Document,
    pages: &std::collections::BTreeMap<u32, ObjectId>,
    control: &PdfJobExecutionControl,
) -> Result<Vec<String>, String> {
    let mut issues = Vec::new();
    for (index, (page_number, page_id)) in pages.iter().enumerate() {
        if index.is_multiple_of(64) {
            control.ensure_not_cancelled()?;
        }
        let media = inherited_box(document, *page_id, b"MediaBox");
        let crop = inherited_box(document, *page_id, b"CropBox").or(media);
        let trim = inherited_box(document, *page_id, b"TrimBox");
        let art = inherited_box(document, *page_id, b"ArtBox");
        let bleed = inherited_box(document, *page_id, b"BleedBox").or(crop);
        let Some(media) = media else {
            push_example(
                &mut issues,
                format!("page {page_number} has no valid MediaBox"),
            );
            continue;
        };
        if trim.is_some() == art.is_some() {
            push_example(
                &mut issues,
                format!("page {page_number} must define exactly one of TrimBox or ArtBox"),
            );
            continue;
        }
        let print_box = trim.or(art).expect("exactly one print box was checked");
        if !contains(media, print_box)
            || crop.is_none_or(|value| !contains(value, print_box) || !contains(media, value))
            || bleed.is_none_or(|value| !contains(value, print_box) || !contains(media, value))
        {
            push_example(
                &mut issues,
                format!("page {page_number} has page boxes outside their enclosing page area"),
            );
        }
    }
    Ok(issues)
}

fn inherited_box(document: &Document, page_id: ObjectId, key: &[u8]) -> Option<PageBox> {
    let mut current = page_id;
    let mut visited = HashSet::new();
    for _ in 0..256 {
        if !visited.insert(current) {
            return None;
        }
        let dictionary = document.get_dictionary(current).ok()?;
        if let Ok(value) = dictionary.get(key) {
            let value = resolve_object(document, value).ok()?;
            let coordinates = value.as_array().ok()?;
            if coordinates.len() != 4 {
                return None;
            }
            let left = pdf_number(&coordinates[0])?;
            let bottom = pdf_number(&coordinates[1])?;
            let right = pdf_number(&coordinates[2])?;
            let top = pdf_number(&coordinates[3])?;
            if [left, bottom, right, top]
                .iter()
                .all(|value| value.is_finite())
                && right > left
                && top > bottom
            {
                return Some(PageBox {
                    left,
                    bottom,
                    right,
                    top,
                });
            }
            return None;
        }
        current = dictionary
            .get(b"Parent")
            .and_then(Object::as_reference)
            .ok()?;
    }
    None
}

fn contains(outer: PageBox, inner: PageBox) -> bool {
    outer.left <= inner.left
        && outer.bottom <= inner.bottom
        && outer.right >= inner.right
        && outer.top >= inner.top
}

fn pdf_number(value: &Object) -> Option<f64> {
    match value {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(f64::from(*value)),
        _ => None,
    }
}

fn inspect_features(
    document: &Document,
    control: &PdfJobExecutionControl,
) -> Result<FeatureAudit, String> {
    let mut audit = FeatureAudit::default();
    if document
        .catalog()
        .is_ok_and(|catalogue| catalogue.has(b"AcroForm"))
    {
        audit.forms += 1;
    }
    let mut visited = 0_usize;
    inspect_feature_object(
        &Object::Dictionary(document.trailer.clone()),
        &mut audit,
        &mut visited,
    );
    for (index, object) in document.objects.values().enumerate() {
        if index.is_multiple_of(256) {
            control.ensure_not_cancelled()?;
        }
        inspect_feature_object(object, &mut audit, &mut visited);
        if audit.incomplete {
            break;
        }
    }
    Ok(audit)
}

fn inspect_feature_object(object: &Object, audit: &mut FeatureAudit, visited: &mut usize) {
    if *visited >= MAX_FEATURE_NODES {
        audit.incomplete = true;
        return;
    }
    *visited += 1;
    match object {
        Object::Array(values) => {
            for value in values {
                inspect_feature_object(value, audit, visited);
                if audit.incomplete {
                    break;
                }
            }
        }
        Object::Dictionary(dictionary) => inspect_feature_dictionary(dictionary, audit, visited),
        Object::Stream(stream) => {
            if stream.dict.has(b"F") {
                audit.external_content += 1;
            }
            inspect_feature_dictionary(&stream.dict, audit, visited);
        }
        _ => {}
    }
}

fn inspect_feature_dictionary(
    dictionary: &Dictionary,
    audit: &mut FeatureAudit,
    visited: &mut usize,
) {
    if dictionary.has(b"JS")
        || dictionary
            .get(b"S")
            .and_then(Object::as_name)
            .is_ok_and(|name| name == b"JavaScript")
    {
        audit.javascript += 1;
    }
    if dictionary.has(b"XFA") {
        audit.xfa += 1;
    }
    if dictionary.has(b"EF") || dictionary.has(b"EmbeddedFiles") {
        audit.attachments += 1;
    }
    if dictionary.has(b"OPI") {
        audit.external_content += 1;
    }
    if dictionary.has(b"Alternates") {
        audit.alternate_images += 1;
    }
    if dictionary
        .get(b"Subtype")
        .and_then(Object::as_name)
        .is_ok_and(|name| matches!(name, b"Sound" | b"Movie" | b"Screen" | b"3D" | b"RichMedia"))
    {
        audit.non_printing_media += 1;
    }
    for key in [b"TR".as_slice(), b"TR2".as_slice()] {
        if dictionary
            .get(key)
            .is_ok_and(|value| !matches!(value, Object::Name(name) if name == b"Default"))
        {
            audit.transfer_curves += 1;
        }
    }
    if dictionary
        .get(b"Filter")
        .is_ok_and(object_contains_lzw_filter)
    {
        audit.lzw_filters += 1;
    }
    for (_, value) in dictionary.iter() {
        inspect_feature_object(value, audit, visited);
        if audit.incomplete {
            break;
        }
    }
}

fn object_contains_lzw_filter(value: &Object) -> bool {
    match value {
        Object::Name(name) => name == b"LZWDecode" || name == b"LZW",
        Object::Array(values) => values.iter().any(object_contains_lzw_filter),
        _ => false,
    }
}

fn resolve_object<'a>(document: &'a Document, value: &'a Object) -> Result<&'a Object, String> {
    let mut current = value;
    let mut visited = HashSet::new();
    for _ in 0..32 {
        let Object::Reference(id) = current else {
            return Ok(current);
        };
        if !visited.insert(*id) {
            return Err("a cyclic object reference was found".to_string());
        }
        current = document
            .get_object(*id)
            .map_err(|_| "an object reference could not be resolved".to_string())?;
    }
    Err("an object reference exceeds the 32-level resolution limit".to_string())
}

fn resolve_dictionary<'a>(
    document: &'a Document,
    value: &'a Object,
) -> Result<&'a Dictionary, String> {
    match resolve_object(document, value)? {
        Object::Dictionary(dictionary) => Ok(dictionary),
        Object::Stream(stream) => Ok(&stream.dict),
        _ => Err("the object is not a dictionary".to_string()),
    }
}

fn object_text(value: &Object) -> Option<String> {
    match value {
        Object::Name(value) => Some(String::from_utf8_lossy(value).into_owned()),
        Object::String(_, _) => decode_text_string(value).ok(),
        _ => None,
    }
}

fn compact(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase()
}

fn push_bounded(values: &mut Vec<String>, value: String) {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if !value.is_empty() && values.len() < 32 {
        values.push(value.chars().take(160).collect());
    }
}

fn push_example(examples: &mut Vec<String>, value: String) {
    if examples.len() < MAX_EXAMPLES {
        examples.push(value);
    }
}

fn deduplicate(values: &mut Vec<String>) {
    let mut seen = HashSet::new();
    values.retain(|value| seen.insert(compact(value)));
}

fn joined_examples(values: &[String]) -> String {
    if values.is_empty() {
        return "no usable evidence".to_string();
    }
    values
        .iter()
        .take(MAX_EXAMPLES)
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn appended_examples(values: &[String]) -> String {
    if values.is_empty() {
        String::new()
    } else {
        format!(" Examples: {}.", joined_examples(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Object, Stream};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn matches_only_the_selected_pdfx_declaration() {
        assert!(PdfXProfile::PdfX1a2001.declaration_matches(
            &["PDF/X-1:2001".to_string()],
            &["PDF/X-1a:2001".to_string()]
        ));
        assert!(PdfXProfile::PdfX3_2002.declaration_matches(&["PDF/X-3:2002".to_string()], &[]));
        assert!(PdfXProfile::PdfX4.declaration_matches(&["PDF/X-4".to_string()], &[]));
        assert!(!PdfXProfile::PdfX4.declaration_matches(&["PDF/X-3:2002".to_string()], &[]));
    }

    #[test]
    fn parses_xmp_element_and_attribute_identifiers() {
        let fields = parse_xmp_fields(
            br#"<x:xmpmeta xmlns:x="adobe:ns:meta/" xmlns:rdf="rdf" xmlns:pdfxid="pdfxid" xmlns:pdf="pdf"><rdf:RDF><rdf:Description pdf:Trapped="False"><pdfxid:GTS_PDFXVersion>PDF/X-4</pdfxid:GTS_PDFXVersion></rdf:Description></rdf:RDF></x:xmpmeta>"#,
        )
        .unwrap();
        assert_eq!(fields.versions, vec!["PDF/X-4"]);
        assert_eq!(fields.trapped, vec!["False"]);
    }

    #[test]
    fn bare_pdf_fails_bounded_pdfx_requirements_without_claiming_conformance() {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => dictionary! {},
            "Contents" => content_id,
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id =
            document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        document.trailer.set("Root", catalog_id);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "paperworks-pdfx-preflight-{}-{nonce}.pdf",
            std::process::id()
        ));
        document.save(&path).unwrap().sync_all().unwrap();

        let report = run_pdfx_preflight(
            &path,
            PdfXProfile::PdfX4,
            true,
            &PdfJobExecutionControl::direct(),
        )
        .unwrap();
        fs::remove_file(&path).unwrap();

        assert!(report.failed_checks >= 5);
        for code in ["declaration", "encryption", "output-intent", "page-boxes"] {
            assert!(report.failures.iter().any(|failure| failure.code == code));
        }
    }
}
