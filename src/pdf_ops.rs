use lopdf::{Document, Object, Dictionary};
use std::collections::BTreeMap;
use printpdf::{PdfDocument, Mm, Image, ImageTransform};
use printpdf::image_crate::ImageDecoder;
use printpdf::image_crate::codecs::png::PngDecoder;
use printpdf::image_crate::codecs::jpeg::JpegDecoder;
use std::io::{BufWriter, Cursor};

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


/// Decode raw image bytes to a printpdf Image, returning the image object along with dimensions (width, height)
fn decode_to_printpdf_image(raw_bytes: &[u8]) -> Result<(Image, u32, u32), String> {
    let cursor = Cursor::new(raw_bytes);
    
    // Determine image format via PNG magic bytes (137 80 78 71 13 10 26 10)
    let is_png = raw_bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]);
    
    if is_png {
        let decoder = PngDecoder::new(cursor).map_err(|e| format!("Failed to initialize PNG decoder: {}", e))?;
        let (w, h) = decoder.dimensions();
        let img = Image::try_from(decoder).map_err(|e| format!("Failed to load PNG into PDF: {:?}", e))?;
        Ok((img, w, h))
    } else {
        let decoder = JpegDecoder::new(cursor).map_err(|e| format!("Failed to initialize JPEG decoder: {}", e))?;
        let (w, h) = decoder.dimensions();
        let img = Image::try_from(decoder).map_err(|e| format!("Failed to load JPEG into PDF: {:?}", e))?;
        Ok((img, w, h))
    }
}

/// Convert multiple images (PNG/JPG) to a single PDF in-memory
pub fn images_to_pdf(images: Vec<Vec<u8>>) -> Result<Vec<u8>, String> {
    if images.is_empty() {
        return Err("Nenhuma imagem fornecida.".to_string());
    }
    for (i, raw_img) in images.iter().enumerate() {
        validate_image_header(raw_img).map_err(|e| format!("Imagem #{}: {}", i + 1, e))?;
    }

    // Load first image to initialize the document size
    let (first_img, w1, h1) = decode_to_printpdf_image(&images[0])?;

    // Set A4 as baseline scale math: 210mm width
    let page_width = 210.0;
    let page_height = page_width * (h1 as f64) / (w1 as f64);

    let (doc, page1, layer1) = PdfDocument::new("Images PDF", Mm(page_width as f32), Mm(page_height as f32), "Layer 1");
    
    // Add first image to first page
    {
        let current_layer = doc.get_page(page1).get_layer(layer1);
        let target_dpi = (w1 as f64) * 25.4 / page_width;
        
        first_img.add_to_layer(
            current_layer,
            ImageTransform {
                translate_x: Some(Mm(0.0)),
                translate_y: Some(Mm(0.0)),
                rotate: None,
                scale_x: None,
                scale_y: None,
                dpi: Some(target_dpi as f32),
            },
        );
    }

    // Add subsequent pages and images
    for raw_img in images.iter().skip(1) {
        let (img, w, h) = decode_to_printpdf_image(raw_img)?;
        
        let p_width = 210.0;
        let p_height = p_width * (h as f64) / (w as f64);
        
        let (page_id, layer_id) = doc.add_page(Mm(p_width as f32), Mm(p_height as f32), "Layer 1");
        let current_layer = doc.get_page(page_id).get_layer(layer_id);
        
        let target_dpi = (w as f64) * 25.4 / p_width;
        img.add_to_layer(
            current_layer,
            ImageTransform {
                translate_x: Some(Mm(0.0)),
                translate_y: Some(Mm(0.0)),
                rotate: None,
                scale_x: None,
                scale_y: None,
                dpi: Some(target_dpi as f32),
            },
        );
    }

    let mut output = Vec::new();
    let mut writer = BufWriter::new(&mut output);
    doc.save(&mut writer).map_err(|e| format!("Failed to compile PDF from images: {}", e))?;
    std::mem::drop(writer);
    
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
}
