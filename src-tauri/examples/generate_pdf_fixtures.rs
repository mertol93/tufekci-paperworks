use lopdf::content::{Content, Operation};
use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::{
    dictionary, Document, EncryptionState, EncryptionVersion, Object, Permissions, Stream,
    StringFormat,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const ENCRYPTED_FIXTURE_PASSWORD: &str = "paperworks-test";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RenderingCorpusManifest {
    schema_version: u32,
    fixtures: Vec<RenderingFixture>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RenderingFixture {
    name: &'static str,
    filename: &'static str,
    expected_outcome: &'static str,
    page_count: Option<u32>,
    password: Option<&'static str>,
    require_password_challenge: bool,
    sample_pages: Vec<u32>,
    minimum_ink_pixels: u32,
    expected_text: Vec<&'static str>,
    expected_code_points: Vec<&'static str>,
    require_rtl: bool,
    require_no_text: bool,
    minimum_annotations: u32,
    expected_page_sizes: Vec<[f64; 2]>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output_directory = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("qa-fixtures"));
    fs::create_dir_all(&output_directory)?;
    save_fixture(
        annotation_fixture(),
        &output_directory,
        "annotations-and-form.pdf",
    )?;
    save_fixture(
        range_loading_fixture(320),
        &output_directory,
        "range-loading.pdf",
    )?;
    save_fixture(
        accessibility_fixture(),
        &output_directory,
        "accessibility-review.pdf",
    )?;
    save_fixture(
        encrypted_fixture()?,
        &output_directory,
        "encrypted-aes256.pdf",
    )?;
    save_fixture(
        signed_structure_fixture(),
        &output_directory,
        "signed-structure.pdf",
    )?;
    save_fixture(
        scanned_image_fixture(),
        &output_directory,
        "scanned-image.pdf",
    )?;
    save_fixture(
        multilingual_fixture(),
        &output_directory,
        "cjk-rtl-type3.pdf",
    )?;
    save_fixture(
        unusual_page_sizes_fixture(),
        &output_directory,
        "unusual-page-sizes.pdf",
    )?;

    let malformed_path = output_directory.join("malformed-truncated.pdf");
    fs::write(
        &malformed_path,
        b"%PDF-1.7\n1 0 obj\n<< /Type /Catalog >>\nendobj\n%%EOF\n",
    )?;
    println!("{}", malformed_path.display());

    let manifest_path = output_directory.join("rendering-corpus.json");
    let mut manifest_bytes = serde_json::to_vec_pretty(&rendering_manifest())?;
    manifest_bytes.push(b'\n');
    fs::write(&manifest_path, manifest_bytes)?;
    println!("{}", manifest_path.display());
    Ok(())
}

fn save_fixture(
    mut document: Document,
    output_directory: &Path,
    filename: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = output_directory.join(filename);
    document.save(&output)?.sync_all()?;
    println!("{}", output.display());
    Ok(())
}

fn rendering_manifest() -> RenderingCorpusManifest {
    RenderingCorpusManifest {
        schema_version: 1,
        fixtures: vec![
            RenderingFixture {
                name: "Annotations and form",
                filename: "annotations-and-form.pdf",
                expected_outcome: "render",
                page_count: Some(1),
                password: None,
                require_password_challenge: false,
                sample_pages: vec![1],
                minimum_ink_pixels: 1_000,
                expected_text: vec!["Annotation and form display fixture"],
                expected_code_points: vec![],
                require_rtl: false,
                require_no_text: false,
                minimum_annotations: 3,
                expected_page_sizes: vec![[595.0, 842.0]],
            },
            RenderingFixture {
                name: "Large range-loaded document",
                filename: "range-loading.pdf",
                expected_outcome: "render",
                page_count: Some(320),
                password: None,
                require_password_challenge: false,
                sample_pages: vec![1, 160, 320],
                minimum_ink_pixels: 1_000,
                expected_text: vec![
                    "Range fixture page 1",
                    "Range fixture page 160",
                    "Range fixture page 320",
                ],
                expected_code_points: vec![],
                require_rtl: false,
                require_no_text: false,
                minimum_annotations: 0,
                expected_page_sizes: vec![],
            },
            RenderingFixture {
                name: "Tagged accessibility structure",
                filename: "accessibility-review.pdf",
                expected_outcome: "render",
                page_count: Some(1),
                password: None,
                require_password_challenge: false,
                sample_pages: vec![1],
                minimum_ink_pixels: 10_000,
                expected_text: vec![],
                expected_code_points: vec![],
                require_rtl: false,
                require_no_text: true,
                minimum_annotations: 0,
                expected_page_sizes: vec![[595.0, 842.0]],
            },
            RenderingFixture {
                name: "AES-256 encrypted input",
                filename: "encrypted-aes256.pdf",
                expected_outcome: "render",
                page_count: Some(1),
                password: Some(ENCRYPTED_FIXTURE_PASSWORD),
                require_password_challenge: true,
                sample_pages: vec![1],
                minimum_ink_pixels: 100,
                expected_text: vec!["Encrypted rendering fixture"],
                expected_code_points: vec![],
                require_rtl: false,
                require_no_text: false,
                minimum_annotations: 0,
                expected_page_sizes: vec![[595.0, 842.0]],
            },
            RenderingFixture {
                name: "Certificate-signature structure",
                filename: "signed-structure.pdf",
                expected_outcome: "render",
                page_count: Some(1),
                password: None,
                require_password_challenge: false,
                sample_pages: vec![1],
                minimum_ink_pixels: 100,
                expected_text: vec!["Signed structure rendering fixture"],
                expected_code_points: vec![],
                require_rtl: false,
                require_no_text: false,
                minimum_annotations: 1,
                expected_page_sizes: vec![[595.0, 842.0]],
            },
            RenderingFixture {
                name: "Image-only scanned page",
                filename: "scanned-image.pdf",
                expected_outcome: "render",
                page_count: Some(1),
                password: None,
                require_password_challenge: false,
                sample_pages: vec![1],
                minimum_ink_pixels: 20_000,
                expected_text: vec![],
                expected_code_points: vec![],
                require_rtl: false,
                require_no_text: true,
                minimum_annotations: 0,
                expected_page_sizes: vec![[595.0, 842.0]],
            },
            RenderingFixture {
                name: "CJK and right-to-left text",
                filename: "cjk-rtl-type3.pdf",
                expected_outcome: "render",
                page_count: Some(1),
                password: None,
                require_password_challenge: false,
                sample_pages: vec![1],
                minimum_ink_pixels: 4_000,
                expected_text: vec!["\u{6587}\u{66f8}\u{8a66}\u{9a13}"],
                expected_code_points: vec![
                    "\u{6587}", "\u{66f8}", "\u{8a66}", "\u{9a13}", "\u{627}", "\u{62e}",
                    "\u{62a}", "\u{628}", "\u{631}",
                ],
                require_rtl: true,
                require_no_text: false,
                minimum_annotations: 0,
                expected_page_sizes: vec![[595.0, 842.0]],
            },
            RenderingFixture {
                name: "Unusual page sizes",
                filename: "unusual-page-sizes.pdf",
                expected_outcome: "render",
                page_count: Some(4),
                password: None,
                require_password_challenge: false,
                sample_pages: vec![1, 2, 3, 4],
                minimum_ink_pixels: 100,
                expected_text: vec![
                    "Business card",
                    "A4 portrait",
                    "Wide landscape",
                    "Large square",
                ],
                expected_code_points: vec![],
                require_rtl: false,
                require_no_text: false,
                minimum_annotations: 0,
                expected_page_sizes: vec![
                    [144.0, 252.0],
                    [595.0, 842.0],
                    [1_008.0, 612.0],
                    [2_000.0, 2_000.0],
                ],
            },
            RenderingFixture {
                name: "Malformed truncated input",
                filename: "malformed-truncated.pdf",
                expected_outcome: "reject",
                page_count: None,
                password: None,
                require_password_challenge: false,
                sample_pages: vec![],
                minimum_ink_pixels: 0,
                expected_text: vec![],
                expected_code_points: vec![],
                require_rtl: false,
                require_no_text: false,
                minimum_annotations: 0,
                expected_page_sizes: vec![],
            },
        ],
    }
}

fn annotation_fixture() -> Document {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 18.into()]),
            Operation::new("Td", vec![72.into(), 760.into()]),
            Operation::new(
                "Tj",
                vec![Object::string_literal(
                    "Annotation and form display fixture",
                )],
            ),
            Operation::new("ET", vec![]),
        ],
    };
    let content_id = document.add_object(Stream::new(
        dictionary! {},
        content.encode().expect("fixture content must encode"),
    ));
    let note_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Text",
        "Rect" => vec![72.into(), 680.into(), 96.into(), 704.into()],
        "Contents" => Object::string_literal("This note is rendered by the PDF.js annotation layer."),
        "Name" => "Comment",
        "C" => vec![1.into(), 0.85.into(), 0.2.into()],
        "F" => 4,
        "P" => page_id,
    });
    let link_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Link",
        "Rect" => vec![72.into(), 620.into(), 260.into(), 650.into()],
        "Border" => vec![0.into(), 0.into(), 1.into()],
        "C" => vec![0.1.into(), 0.35.into(), 0.85.into()],
        "A" => dictionary! {
            "S" => "URI",
            "URI" => Object::string_literal("https://example.com/fixture-link"),
        },
        "P" => page_id,
    });
    let widget_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "T" => Object::string_literal("Reviewer"),
        "TU" => Object::string_literal("Reviewer name"),
        "V" => Object::string_literal("Display only"),
        "DV" => Object::string_literal("Display only"),
        "Rect" => vec![72.into(), 540.into(), 300.into(), 575.into()],
        "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
        "F" => 4,
        "MK" => dictionary! {
            "BC" => vec![0.2.into(), 0.35.into(), 0.65.into()],
            "BG" => vec![0.95.into(), 0.97.into(), 1.into()],
        },
        "BS" => dictionary! { "W" => 1, "S" => "S" },
        "P" => page_id,
    });
    document.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => dictionary! {
                "Font" => dictionary! { "F1" => font_id },
            },
            "Contents" => content_id,
            "Annots" => vec![note_id.into(), link_id.into(), widget_id.into()],
        }),
    );
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "AcroForm" => dictionary! {
            "Fields" => vec![widget_id.into()],
            "NeedAppearances" => true,
            "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
            "DR" => dictionary! {
                "Font" => dictionary! { "Helv" => font_id },
            },
        },
    });
    document.trailer.set("Root", catalog_id);
    document
}

fn range_loading_fixture(page_count: u32) -> Document {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let mut kids = Vec::with_capacity(page_count as usize);
    for page_number in 1..=page_count {
        let commands = format!(
            "0.92 0.95 1 rg 72 670 451 90 re f\n\
             0.12 0.28 0.62 RG 3 w 72 670 451 90 re S\n\
             BT /F1 18 Tf 92 710 Td (Range fixture page {page_number}) Tj ET\n"
        );
        let mut content = commands.into_bytes();
        content.resize(4 * 1024, b' ');
        let content_id = document.add_object(Stream::new(dictionary! {}, content));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => dictionary! {
                "Font" => dictionary! { "F1" => font_id },
            },
            "Contents" => content_id,
        });
        kids.push(Object::Reference(page_id));
    }
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => page_count as i64,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document
}

fn accessibility_fixture() -> Document {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    let structure_root_id = document.new_object_id();
    let content = b"/Figure <</MCID 0>> BDC 0.15 0.38 0.75 rg 72 540 200 110 re f EMC\n\
/Illustration <</MCID 1>> BDC 0.85 0.35 0.25 rg 320 540 200 110 re f EMC\n";
    let content_id = document.add_object(Stream::new(dictionary! {}, content.to_vec()));
    let described_figure_id = document.add_object(dictionary! {
        "Type" => "StructElem",
        "S" => "Figure",
        "P" => structure_root_id,
        "Pg" => page_id,
        "K" => 0,
        "Alt" => Object::string_literal("Blue rectangle representing a described figure"),
    });
    let review_figure_id = document.add_object(dictionary! {
        "Type" => "StructElem",
        "S" => "Illustration",
        "P" => structure_root_id,
        "Pg" => page_id,
        "K" => 1,
    });
    document.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => dictionary! {},
            "Contents" => content_id,
            "StructParents" => 0,
        }),
    );
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    document.objects.insert(
        structure_root_id,
        Object::Dictionary(dictionary! {
            "Type" => "StructTreeRoot",
            "K" => vec![described_figure_id.into(), review_figure_id.into()],
            "RoleMap" => dictionary! { "Illustration" => "Figure" },
            "ParentTree" => dictionary! {
                "Nums" => vec![
                    0.into(),
                    Object::Array(vec![described_figure_id.into(), review_figure_id.into()]),
                ],
            },
            "ParentTreeNextKey" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "Lang" => Object::string_literal("en-GB"),
        "MarkInfo" => dictionary! { "Marked" => true },
        "StructTreeRoot" => structure_root_id,
        "ViewerPreferences" => dictionary! { "DisplayDocTitle" => true },
    });
    let info_id = document.add_object(dictionary! {
        "Title" => Object::string_literal("Accessibility review fixture"),
    });
    document.trailer.set("Root", catalog_id);
    document.trailer.set("Info", info_id);
    document
}

fn simple_text_fixture(text: &str) -> Document {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new("rg", vec![0.12.into(), 0.28.into(), 0.62.into()]),
            Operation::new("re", vec![72.into(), 650.into(), 451.into(), 100.into()]),
            Operation::new("f", vec![]),
            Operation::new("Q", vec![]),
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 18.into()]),
            Operation::new("Td", vec![90.into(), 700.into()]),
            Operation::new("rg", vec![1.into(), 1.into(), 1.into()]),
            Operation::new("Tj", vec![Object::string_literal(text)]),
            Operation::new("ET", vec![]),
        ],
    };
    let content_id = document.add_object(Stream::new(
        dictionary! {},
        content.encode().expect("fixture content must encode"),
    ));
    document.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => dictionary! {
                "Font" => dictionary! { "F1" => font_id },
            },
            "Contents" => content_id,
        }),
    );
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document
}

fn encrypted_fixture() -> Result<Document, Box<dyn std::error::Error>> {
    let mut document = simple_text_fixture("Encrypted rendering fixture");
    let file_key = [0x5a_u8; 32];
    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
    let state = EncryptionState::try_from(EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        file_encryption_key: &file_key,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: "paperworks-owner-test",
        user_password: ENCRYPTED_FIXTURE_PASSWORD,
        permissions: Permissions::all(),
    })?;
    document.encrypt(&state)?;
    Ok(document)
}

fn signed_structure_fixture() -> Document {
    let mut document = simple_text_fixture("Signed structure rendering fixture");
    let page_id = *document
        .get_pages()
        .get(&1)
        .expect("fixture must contain a page");
    let signature_id = document.add_object(dictionary! {
        "Type" => "Sig",
        "Filter" => "Adobe.PPKLite",
        "SubFilter" => "adbe.pkcs7.detached",
        "ByteRange" => vec![0.into(), 0.into(), 0.into(), 0.into()],
        "Contents" => Object::String(vec![0; 256], StringFormat::Hexadecimal),
        "Name" => Object::string_literal("Synthetic rendering fixture"),
        "Reason" => Object::string_literal("Rendering compatibility only"),
        "M" => Object::string_literal("D:20260101000000Z"),
    });
    let widget_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Sig",
        "T" => Object::string_literal("RenderingSignature"),
        "Rect" => vec![72.into(), 580.into(), 260.into(), 620.into()],
        "F" => 4,
        "P" => page_id,
        "V" => signature_id,
    });
    document
        .get_object_mut(page_id)
        .expect("fixture page must exist")
        .as_dict_mut()
        .expect("fixture page must be a dictionary")
        .set("Annots", vec![Object::Reference(widget_id)]);
    let root_id = document
        .trailer
        .get(b"Root")
        .expect("fixture root must exist")
        .as_reference()
        .expect("fixture root must be indirect");
    document
        .get_object_mut(root_id)
        .expect("fixture catalogue must exist")
        .as_dict_mut()
        .expect("fixture catalogue must be a dictionary")
        .set(
            "AcroForm",
            dictionary! {
                "Fields" => vec![Object::Reference(widget_id)],
                "SigFlags" => 3,
            },
        );
    document
}

fn scanned_image_fixture() -> Document {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    let width = 300_u32;
    let height = 420_u32;
    let mut pixels = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            let on_border = x < 8 || y < 8 || x >= width - 8 || y >= height - 8;
            let on_text_line =
                (55..=310).contains(&y) && y % 42 < 9 && (28..=270).contains(&x) && x % 58 < 44;
            let on_stamp = (x as i64 - 230).pow(2) + (y as i64 - 350).pow(2) < 34_i64.pow(2);
            let colour = if on_border || on_text_line {
                [28, 36, 52]
            } else if on_stamp {
                [174, 42, 58]
            } else {
                let shade = 242_u8.saturating_sub(((x + y) % 17) as u8);
                [shade, shade, shade.saturating_sub(3)]
            };
            pixels.extend_from_slice(&colour);
        }
    }
    let mut image = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => width as i64,
            "Height" => height as i64,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
        },
        pixels,
    );
    image.compress().expect("fixture image must compress");
    let image_id = document.add_object(image);
    let content_id = document.add_object(Stream::new(
        dictionary! {},
        b"q 451 0 0 632 72 105 cm /Scan Do Q\n".to_vec(),
    ));
    document.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => dictionary! {
                "XObject" => dictionary! { "Scan" => image_id },
            },
            "Contents" => content_id,
        }),
    );
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document
}

fn multilingual_fixture() -> Document {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    let unicode_values = [
        0x6587_u16, 0x66f8, 0x8a66, 0x9a13, 0x0627, 0x062e, 0x062a, 0x0628, 0x0627, 0x0631,
    ];
    let font_id = add_type3_unicode_font(&mut document, &unicode_values);
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new(
                "Tf",
                vec![Object::Name(b"UnicodeFixture".to_vec()), 60.into()],
            ),
            Operation::new("Td", vec![72.into(), 700.into()]),
            Operation::new(
                "Tj",
                vec![Object::String(vec![1, 2, 3, 4], StringFormat::Hexadecimal)],
            ),
            Operation::new("ET", vec![]),
            Operation::new("BT", vec![]),
            Operation::new(
                "Tf",
                vec![Object::Name(b"UnicodeFixture".to_vec()), 60.into()],
            ),
            Operation::new("Td", vec![72.into(), 610.into()]),
            Operation::new(
                "Tj",
                vec![Object::String(
                    vec![5, 6, 7, 8, 9, 10],
                    StringFormat::Hexadecimal,
                )],
            ),
            Operation::new("ET", vec![]),
        ],
    };
    let content_id = document.add_object(Stream::new(
        dictionary! {},
        content.encode().expect("fixture content must encode"),
    ));
    document.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => dictionary! {
                "Font" => dictionary! { "UnicodeFixture" => font_id },
            },
            "Contents" => content_id,
        }),
    );
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "Lang" => Object::string_literal("mul"),
    });
    document.trailer.set("Root", catalog_id);
    document
}

fn add_type3_unicode_font(document: &mut Document, unicode_values: &[u16]) -> lopdf::ObjectId {
    let mut char_procedures = lopdf::Dictionary::new();
    let mut differences = vec![1.into()];
    let mut widths = Vec::with_capacity(unicode_values.len());
    for (index, _) in unicode_values.iter().enumerate() {
        let glyph_name = format!("g{:02}", index + 1).into_bytes();
        let inset = 45 + (index as i64 % 4) * 24;
        let glyph = format!(
            "600 0 0 0 560 700 d1\n\
             0.08 0.18 0.38 RG 18 w {inset} 70 m {} 630 l S\n\
             0.16 0.48 0.76 rg {} {} {} {} re f\n\
             0.82 0.22 0.28 rg {} {} 80 80 re f\n",
            560 - inset,
            75 + (index as i64 % 3) * 40,
            115 + (index as i64 % 4) * 55,
            390 - (index as i64 % 3) * 35,
            105 + (index as i64 % 2) * 75,
            390 - (index as i64 % 4) * 65,
            180 + (index as i64 % 5) * 55,
        );
        let glyph_id = document.add_object(Stream::new(dictionary! {}, glyph.into_bytes()));
        char_procedures.set(glyph_name.clone(), glyph_id);
        differences.push(Object::Name(glyph_name));
        widths.push(600.into());
    }

    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\n\
         begincmap\n\
         /CIDSystemInfo << /Registry (Paperworks) /Ordering (Unicode) /Supplement 0 >> def\n\
         /CMapName /PaperworksUnicode def\n\
         /CMapType 2 def\n\
         1 begincodespacerange\n<00> <FF>\nendcodespacerange\n",
    );
    cmap.push_str(&format!("{} beginbfchar\n", unicode_values.len()));
    for (index, unicode) in unicode_values.iter().enumerate() {
        cmap.push_str(&format!("<{:02X}> <{unicode:04X}>\n", index + 1));
    }
    cmap.push_str(
        "endbfchar\n\
         endcmap\n\
         CMapName currentdict /CMap defineresource pop\n\
         end\n\
         end\n",
    );
    let to_unicode_id = document.add_object(Stream::new(dictionary! {}, cmap.into_bytes()));
    document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type3",
        "Name" => "UnicodeFixture",
        "FontBBox" => vec![0.into(), 0.into(), 560.into(), 700.into()],
        "FontMatrix" => vec![
            0.001.into(),
            0.into(),
            0.into(),
            0.001.into(),
            0.into(),
            0.into(),
        ],
        "CharProcs" => char_procedures,
        "Encoding" => dictionary! { "Type" => "Encoding", "Differences" => differences },
        "FirstChar" => 1,
        "LastChar" => unicode_values.len() as i64,
        "Widths" => widths,
        "Resources" => dictionary! {},
        "ToUnicode" => to_unicode_id,
    })
}

fn unusual_page_sizes_fixture() -> Document {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let specifications = [
        ("Business card", 144_i64, 252_i64),
        ("A4 portrait", 595, 842),
        ("Wide landscape", 1_008, 612),
        ("Large square", 2_000, 2_000),
    ];
    let mut kids = Vec::with_capacity(specifications.len());
    for (label, width, height) in &specifications {
        let font_size = if *width < 200 { 9 } else { 18 };
        let x = if *width < 200 { 10 } else { 48 };
        let y = height - if *height < 300 { 36 } else { 72 };
        let rectangle_width = (width - x * 2).max(20);
        let rectangle_height = (height / 4).max(20);
        let commands = format!(
            "0.12 0.28 0.62 rg {x} {} {rectangle_width} {rectangle_height} re f\n\
             BT /F1 {font_size} Tf 1 1 1 rg {} {} Td ({label}) Tj ET\n",
            height / 3,
            x + 4,
            y
        );
        let content_id = document.add_object(Stream::new(dictionary! {}, commands.into_bytes()));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), (*width).into(), (*height).into()],
            "Resources" => dictionary! {
                "Font" => dictionary! { "F1" => font_id },
            },
            "Contents" => content_id,
        });
        kids.push(Object::Reference(page_id));
    }
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => specifications.len() as i64,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document
}
