use lopdf::{Document, Object, Dictionary, EncryptionVersion, EncryptionState, Permissions};
use std::collections::BTreeMap;
use printpdf::{PdfDocument, Mm, Pt, PdfPage, Op, XObjectTransform};
use printpdf::image::RawImage;

/// Valida se os bytes representam um arquivo PDF válido (inicia com %PDF-)
pub fn validate_pdf_header(bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() {
        return Err("O arquivo PDF enviado está vazio (0 bytes).".to_string());
    }
    if bytes.len() < 5 || !bytes.starts_with(b"%PDF-") {
        return Err("Assinatura de arquivo PDF inválida. O arquivo deve começar com '%PDF-'.".to_string());
    }
    Ok(())
}

/// Valida se os bytes representam uma imagem PNG ou JPEG válida
pub fn validate_image_header(bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() {
        return Err("A imagem enviada está vazia (0 bytes).".to_string());
    }
    let is_png = bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]);
    let is_jpeg = bytes.starts_with(&[255, 216, 255]);
    if !is_png && !is_jpeg {
        return Err("Assinatura de imagem inválida. Apenas PNG e JPEG/JPG são aceitos.".to_string());
    }
    Ok(())
}

/// Helper to parse page range string like "1-3, 5, 7-10"
fn parse_range(range_str: &str, max_pages: u32) -> Result<Vec<u32>, String> {
    let mut pages = Vec::new();
    for part in range_str.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if part.contains('-') {
            let bounds: Vec<&str> = part.split('-').collect();
            if bounds.len() != 2 {
                return Err(format!("Invalid range format: {}", part));
            }
            let start: u32 = bounds[0].trim().parse().map_err(|_| format!("Invalid number: {}", bounds[0]))?;
            let end: u32 = bounds[1].trim().parse().map_err(|_| format!("Invalid number: {}", bounds[1]))?;
            if start > end {
                return Err(format!("Start page {} is greater than end page {}", start, end));
            }
            for p in start..=end {
                if p > 0 && p <= max_pages {
                    pages.push(p);
                } else {
                    return Err(format!("Page {} out of range (1-{})", p, max_pages));
                }
            }
        } else {
            let p: u32 = part.parse().map_err(|_| format!("Invalid page number: {}", part))?;
            if p > 0 && p <= max_pages {
                pages.push(p);
            } else {
                return Err(format!("Page {} out of range (1-{})", p, max_pages));
            }
        }
    }
    Ok(pages)
}

/// Merge multiple PDF documents in-memory
pub fn merge_pdfs(files: Vec<Vec<u8>>) -> Result<Vec<u8>, String> {
    if files.is_empty() {
        return Err("Nenhum arquivo fornecido para junção.".to_string());
    }
    for (i, file) in files.iter().enumerate() {
        validate_pdf_header(file).map_err(|e| format!("Arquivo #{}: {}", i + 1, e))?;
    }
    if files.len() == 1 {
        return Ok(files[0].clone());
    }

    let mut documents = Vec::new();
    for file in &files {
        let doc = Document::load_mem(file).map_err(|e| format!("Failed to load PDF: {}", e))?;
        documents.push(doc);
    }

    let mut max_id = 0;
    let mut documents_pages = Vec::new();
    let mut documents_objects = BTreeMap::new();

    for mut doc in documents {
        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;

        let pages = doc.get_pages();
        documents_pages.push(pages);

        for (id, object) in doc.objects {
            documents_objects.insert(id, object);
        }
    }

    let mut page_ids = Vec::new();
    for pages in documents_pages {
        for (_page_number, page_id) in pages {
            page_ids.push(Object::Reference(page_id));
        }
    }

    let new_pages_id = (max_id + 1, 0);
    let mut pages_dict = Dictionary::new();
    pages_dict.set("Type", Object::Name("Pages".as_bytes().to_vec()));
    pages_dict.set("Count", Object::Integer(page_ids.len() as i64));
    pages_dict.set("Kids", Object::Array(page_ids.clone()));

    documents_objects.insert(new_pages_id, Object::Dictionary(pages_dict));

    let new_catalog_id = (max_id + 2, 0);
    let mut catalog_dict = Dictionary::new();
    catalog_dict.set("Type", Object::Name("Catalog".as_bytes().to_vec()));
    catalog_dict.set("Pages", Object::Reference(new_pages_id));
    documents_objects.insert(new_catalog_id, Object::Dictionary(catalog_dict));

    for page_ref in &page_ids {
        if let Object::Reference(page_id) = page_ref {
            if let Some(Object::Dictionary(page_dict)) = documents_objects.get_mut(page_id) {
                page_dict.set("Parent", Object::Reference(new_pages_id));
            }
        }
    }

    let mut merged_doc = Document::new();
    merged_doc.objects = documents_objects;
    merged_doc.trailer.set("Root", Object::Reference(new_catalog_id));
    merged_doc.max_id = max_id + 2;

    merged_doc.renumber_objects();

    let mut output = Vec::new();
    merged_doc.save_to(&mut output).map_err(|e| format!("Failed to save merged PDF: {}", e))?;
    Ok(output)
}

/// Split single PDF by extracting range of pages
pub fn split_pdf(file: &[u8], range_str: &str) -> Result<Vec<u8>, String> {
    validate_pdf_header(file)?;
    let mut doc = Document::load_mem(file).map_err(|e| format!("Failed to load PDF: {}", e))?;
    
    let pages = doc.get_pages();
    let max_pages = pages.len() as u32;

    let pages_to_keep = if range_str.trim().is_empty() {
        (1..=max_pages).collect::<Vec<u32>>()
    } else {
        parse_range(range_str, max_pages)?
    };

    if pages_to_keep.is_empty() {
        return Err("No pages selected".to_string());
    }

    let mut original_page_ids = Vec::new();
    for page_num in &pages_to_keep {
        if let Some(&page_id) = pages.get(page_num) {
            original_page_ids.push(page_id);
        } else {
            return Err(format!("Page {} not found", page_num));
        }
    }

    let catalog = doc.catalog().map_err(|e| format!("Missing PDF Catalog: {}", e))?;
    let root_pages_ref = catalog.get(b"Pages").map_err(|_| "Catalog missing /Pages reference".to_string())?;
    let root_pages_id = match root_pages_ref {
        Object::Reference(id) => *id,
        _ => return Err("Pages catalog key is not an object reference".to_string()),
    };

    let all_page_ids: Vec<lopdf::ObjectId> = pages.values().cloned().collect();
    for id in all_page_ids {
        if !original_page_ids.contains(&id) {
            doc.objects.remove(&id);
        }
    }

    let mut new_kids = Vec::new();
    for id in &original_page_ids {
        new_kids.push(Object::Reference(*id));
    }

    if let Some(Object::Dictionary(pages_dict)) = doc.objects.get_mut(&root_pages_id) {
        pages_dict.set("Kids", Object::Array(new_kids));
        pages_dict.set("Count", Object::Integer(original_page_ids.len() as i64));
    } else {
        return Err("Could not find root Pages dictionary".to_string());
    }

    for id in &original_page_ids {
        if let Some(Object::Dictionary(page_dict)) = doc.objects.get_mut(id) {
            page_dict.set("Parent", Object::Reference(root_pages_id));
        }
    }

    doc.prune_objects();
    doc.renumber_objects();

    let mut output = Vec::new();
    doc.save_to(&mut output).map_err(|e| format!("Failed to save split PDF: {}", e))?;
    Ok(output)
}

/// Rotate PDF pages in-memory
pub fn rotate_pdf(file: &[u8], angle: i32) -> Result<Vec<u8>, String> {
    validate_pdf_header(file)?;
    if angle != 90 && angle != 180 && angle != 270 {
        return Err("Ângulo de rotação inválido. Deve ser 90, 180 ou 270.".to_string());
    }

    let mut doc = Document::load_mem(file).map_err(|e| format!("Failed to load PDF: {}", e))?;
    let pages = doc.get_pages();

    for (_page_num, page_id) in pages {
        if let Some(Object::Dictionary(page_dict)) = doc.objects.get_mut(&page_id) {
            let current_rotate = match page_dict.get(b"Rotate") {
                Ok(Object::Integer(r)) => *r as i32,
                _ => 0,
            };
            let new_rotate = (current_rotate + angle) % 360;
            page_dict.set("Rotate", Object::Integer(new_rotate as i64));
        }
    }

    let mut output = Vec::new();
    doc.save_to(&mut output).map_err(|e| format!("Failed to save rotated PDF: {}", e))?;
    Ok(output)
}


/// Convert multiple images (PNG/JPG) to a single PDF in-memory
pub fn images_to_pdf(images: Vec<Vec<u8>>) -> Result<Vec<u8>, String> {
    if images.is_empty() {
        return Err("Nenhuma imagem fornecida.".to_string());
    }
    for (i, raw_img) in images.iter().enumerate() {
        validate_image_header(raw_img).map_err(|e| format!("Imagem #{}: {}", i + 1, e))?;
    }

    let mut doc = PdfDocument::new("Images PDF");
    let mut warnings = Vec::new();

    for (i, raw_img) in images.iter().enumerate() {
        let image = RawImage::decode_from_bytes(raw_img, &mut warnings)
            .map_err(|e| format!("Imagem #{}: {}", i + 1, e))?;

        let p_width = 210.0;
        let p_height = p_width * (image.height as f64) / (image.width as f64);

        let image_xobject_id = doc.add_image(&image);

        let target_dpi = (image.width as f64) * 25.4 / p_width;
        let transform = XObjectTransform {
            translate_x: Some(Pt(0.0)),
            translate_y: Some(Pt(0.0)),
            rotate: None,
            scale_x: None,
            scale_y: None,
            dpi: Some(target_dpi as f32),
            no_auto_scale: false,
        };

        let op = Op::UseXobject {
            id: image_xobject_id,
            transform,
        };

        let page = PdfPage::new(Mm(p_width as f32), Mm(p_height as f32), vec![op]);
        doc.pages.push(page);
    }

    let output = doc.save(&printpdf::PdfSaveOptions::default(), &mut warnings);
    Ok(output)
}

/// Compress a PDF document to reduce its file size
pub fn compress_pdf(file: &[u8]) -> Result<Vec<u8>, String> {
    validate_pdf_header(file)?;
    let mut doc = Document::load_mem(file).map_err(|e| format!("Failed to load PDF: {}", e))?;
    
    // Compress stream objects
    for (_, object) in doc.objects.iter_mut() {
        if let Object::Stream(stream) = object {
            let _ = stream.compress();
        }
    }
    
    let mut output = Vec::new();
    doc.save_to(&mut output).map_err(|e| format!("Failed to save compressed PDF: {}", e))?;
    Ok(output)
}

/// Convert a Word document (DOCX) to PDF in-memory using office2pdf
pub fn docx_to_pdf(file: &[u8]) -> Result<Vec<u8>, String> {
    use office2pdf::config::{ConvertOptions, Format};
    
    let result = office2pdf::convert_bytes(
        file,
        Format::Docx,
        &ConvertOptions::default()
    ).map_err(|e| format!("Failed to convert DOCX to PDF: {:?}", e))?;
    
    Ok(result.pdf)
}

/// Protect PDF with a password using AES-128
pub fn protect_pdf(file: &[u8], password: &str) -> Result<Vec<u8>, String> {
    validate_pdf_header(file)?;

    let pass_len = password.chars().count();
    if pass_len < 4 || pass_len > 128 {
        return Err("A senha deve ter entre 4 e 128 caracteres.".to_string());
    }

    let mut doc = Document::load_mem(file).map_err(|e| format!("Falha ao carregar o PDF: {}", e))?;

    if doc.is_encrypted() {
        return Err("O arquivo PDF já está protegido por senha.".to_string());
    }

    // Ensure the PDF trailer has an ID element, which is required for PDF encryption.
    if !doc.trailer.has(b"ID") {
        let file_id = Object::String(vec![1u8; 16], lopdf::StringFormat::Hexadecimal);
        doc.trailer.set("ID", Object::Array(vec![file_id.clone(), file_id]));
    }

    let version = EncryptionVersion::V2 {
        document: &doc,
        owner_password: password,
        user_password: password,
        key_length: 128,
        permissions: Permissions::default(),
    };

    let state = EncryptionState::try_from(version)
        .map_err(|e| format!("Falha ao configurar a criptografia: {}", e))?;

    doc.encrypt(&state)
        .map_err(|e| format!("Falha ao criptografar o PDF: {}", e))?;

    let mut output = Vec::new();
    doc.save_to(&mut output)
        .map_err(|e| format!("Falha ao salvar o PDF criptografado: {}", e))?;

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_range() {
        assert_eq!(parse_range("1-3, 5", 10).unwrap(), vec![1, 2, 3, 5]);
        assert_eq!(parse_range("  2,4-6  ,  8 ", 10).unwrap(), vec![2, 4, 5, 6, 8]);
        assert_eq!(parse_range("", 10).unwrap(), Vec::<u32>::new());
        assert!(parse_range("1-11", 10).is_err());
        assert!(parse_range("abc", 10).is_err());
        assert!(parse_range("5-3", 10).is_err());
    }

    #[test]
    fn test_protect_pdf() {
        let mut doc = Document::new();
        let pages_id = doc.new_object_id();
        let mut pages_dict = Dictionary::new();
        pages_dict.set("Type", Object::Name("Pages".as_bytes().to_vec()));
        pages_dict.set("Count", Object::Integer(0));
        pages_dict.set("Kids", Object::Array(vec![]));
        doc.objects.insert(pages_id, Object::Dictionary(pages_dict));

        let catalog_id = doc.new_object_id();
        let mut catalog_dict = Dictionary::new();
        catalog_dict.set("Type", Object::Name("Catalog".as_bytes().to_vec()));
        catalog_dict.set("Pages", Object::Reference(pages_id));
        doc.objects.insert(catalog_id, Object::Dictionary(catalog_dict));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut pdf_bytes = Vec::new();
        doc.save_to(&mut pdf_bytes).unwrap();

        let password = "pdfpassword123";
        let protected_res = protect_pdf(&pdf_bytes, password);
        if let Err(ref e) = protected_res {
            panic!("protect_pdf failed with: {}", e);
        }
        let protected_bytes = protected_res.unwrap();

        let protected_doc = Document::load_mem(&protected_bytes).unwrap();
        assert!(protected_doc.is_encrypted());

        let double_protect = protect_pdf(&protected_bytes, password);
        assert!(double_protect.is_err());

        let short_protect = protect_pdf(&pdf_bytes, "123");
        assert!(short_protect.is_err());
    }
}
