use std::io::Write;

use flate2::write::ZlibEncoder;
use flate2::Compression;
use xlsxpdf_core::error::{Error, Result};

pub struct ImageXObject {
    pub obj_id: u32,
    pub name: String,
}

pub struct PdfWriter<W: Write> {
    out: W,
    offset: u64,
    next_obj_id: u32,
    obj_offsets: Vec<u64>,
    page_obj_ids: Vec<u32>,
    pages_obj_id: u32,
    font_obj_id: Option<u32>,
    image_objects: Vec<ImageXObject>,
}

impl<W: Write> PdfWriter<W> {
    pub fn new(out: W) -> Result<Self> {
        let mut writer = Self {
            out,
            offset: 0,
            next_obj_id: 1,
            obj_offsets: Vec::new(),
            page_obj_ids: Vec::new(),
            pages_obj_id: 0,
            font_obj_id: None,
            image_objects: Vec::new(),
        };
        writer.write_header()?;
        // Pre-allocate the Pages object ID so page dicts can reference /Parent
        writer.pages_obj_id = writer.alloc_obj_id();
        Ok(writer)
    }

    fn write_header(&mut self) -> Result<()> {
        self.write_bytes(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n")
    }

    pub fn alloc_obj_id(&mut self) -> u32 {
        let id = self.next_obj_id;
        self.next_obj_id += 1;
        id
    }

    fn begin_obj(&mut self, id: u32) -> Result<()> {
        while self.obj_offsets.len() < id as usize {
            self.obj_offsets.push(0);
        }
        self.obj_offsets[id as usize - 1] = self.offset;
        self.write_bytes(format!("{id} 0 obj\n").as_bytes())
    }

    fn end_obj(&mut self) -> Result<()> {
        self.write_bytes(b"endobj\n")
    }

    pub fn write_font_resources(&mut self) -> Result<u32> {
        if let Some(id) = self.font_obj_id {
            return Ok(id);
        }

        let helvetica_id = self.alloc_obj_id();
        self.begin_obj(helvetica_id)?;
        self.write_bytes(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>\n")?;
        self.end_obj()?;

        let helvetica_bold_id = self.alloc_obj_id();
        self.begin_obj(helvetica_bold_id)?;
        self.write_bytes(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>\n")?;
        self.end_obj()?;

        let times_id = self.alloc_obj_id();
        self.begin_obj(times_id)?;
        self.write_bytes(b"<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman /Encoding /WinAnsiEncoding >>\n")?;
        self.end_obj()?;

        let helvetica_oblique_id = self.alloc_obj_id();
        self.begin_obj(helvetica_oblique_id)?;
        self.write_bytes(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Oblique /Encoding /WinAnsiEncoding >>\n")?;
        self.end_obj()?;

        let helvetica_bold_oblique_id = self.alloc_obj_id();
        self.begin_obj(helvetica_bold_oblique_id)?;
        self.write_bytes(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-BoldOblique /Encoding /WinAnsiEncoding >>\n")?;
        self.end_obj()?;

        let font_dict_id = self.alloc_obj_id();
        self.begin_obj(font_dict_id)?;
        self.write_bytes(
            format!(
                "<< /F1 {helvetica_id} 0 R /F2 {helvetica_bold_id} 0 R /F3 {times_id} 0 R /F4 {helvetica_oblique_id} 0 R /F5 {helvetica_bold_oblique_id} 0 R >>\n"
            )
            .as_bytes(),
        )?;
        self.end_obj()?;

        self.font_obj_id = Some(font_dict_id);
        Ok(font_dict_id)
    }

    pub fn write_jpeg_xobject(
        &mut self,
        name: &str,
        jpeg_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<()> {
        let obj_id = self.alloc_obj_id();
        self.begin_obj(obj_id)?;
        self.write_bytes(
            format!(
                concat!(
                    "<< /Type /XObject /Subtype /Image",
                    " /Width {w} /Height {h}",
                    " /ColorSpace /DeviceRGB",
                    " /BitsPerComponent 8",
                    " /Filter /DCTDecode",
                    " /Length {len}",
                    " >>\nstream\n"
                ),
                w = width,
                h = height,
                len = jpeg_data.len(),
            )
            .as_bytes(),
        )?;
        self.write_bytes(jpeg_data)?;
        self.write_bytes(b"\nendstream\n")?;
        self.end_obj()?;

        self.image_objects.push(ImageXObject {
            obj_id,
            name: name.to_string(),
        });
        Ok(())
    }

    pub fn write_raw_image_xobject(
        &mut self,
        name: &str,
        rgb_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<()> {
        let compressed = compress(rgb_data)?;
        let obj_id = self.alloc_obj_id();
        self.begin_obj(obj_id)?;
        self.write_bytes(
            format!(
                concat!(
                    "<< /Type /XObject /Subtype /Image",
                    " /Width {w} /Height {h}",
                    " /ColorSpace /DeviceRGB",
                    " /BitsPerComponent 8",
                    " /Filter /FlateDecode",
                    " /Length {len}",
                    " >>\nstream\n"
                ),
                w = width,
                h = height,
                len = compressed.len(),
            )
            .as_bytes(),
        )?;
        self.write_bytes(&compressed)?;
        self.write_bytes(b"\nendstream\n")?;
        self.end_obj()?;

        self.image_objects.push(ImageXObject {
            obj_id,
            name: name.to_string(),
        });
        Ok(())
    }

    pub fn write_raw_image_xobject_with_smask(
        &mut self,
        name: &str,
        rgb_data: &[u8],
        alpha_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<()> {
        // Write SMask (grayscale alpha channel)
        let alpha_compressed = compress(alpha_data)?;
        let smask_id = self.alloc_obj_id();
        self.begin_obj(smask_id)?;
        self.write_bytes(
            format!(
                concat!(
                    "<< /Type /XObject /Subtype /Image",
                    " /Width {w} /Height {h}",
                    " /ColorSpace /DeviceGray",
                    " /BitsPerComponent 8",
                    " /Filter /FlateDecode",
                    " /Length {len}",
                    " >>\nstream\n"
                ),
                w = width,
                h = height,
                len = alpha_compressed.len(),
            )
            .as_bytes(),
        )?;
        self.write_bytes(&alpha_compressed)?;
        self.write_bytes(b"\nendstream\n")?;
        self.end_obj()?;

        // Write main image referencing SMask
        let rgb_compressed = compress(rgb_data)?;
        let obj_id = self.alloc_obj_id();
        self.begin_obj(obj_id)?;
        self.write_bytes(
            format!(
                concat!(
                    "<< /Type /XObject /Subtype /Image",
                    " /Width {w} /Height {h}",
                    " /ColorSpace /DeviceRGB",
                    " /BitsPerComponent 8",
                    " /Filter /FlateDecode",
                    " /SMask {smask} 0 R",
                    " /Length {len}",
                    " >>\nstream\n"
                ),
                w = width,
                h = height,
                smask = smask_id,
                len = rgb_compressed.len(),
            )
            .as_bytes(),
        )?;
        self.write_bytes(&rgb_compressed)?;
        self.write_bytes(b"\nendstream\n")?;
        self.end_obj()?;

        self.image_objects.push(ImageXObject {
            obj_id,
            name: name.to_string(),
        });
        Ok(())
    }

    fn build_xobject_dict(&self) -> String {
        if self.image_objects.is_empty() {
            return String::new();
        }
        let mut s = String::from(" /XObject <<");
        for img in &self.image_objects {
            s.push_str(&format!(" /{} {} 0 R", img.name, img.obj_id));
        }
        s.push_str(" >>");
        s
    }

    pub fn write_page(
        &mut self,
        content_stream: &[u8],
        page_width_pt: f64,
        page_height_pt: f64,
        font_dict_id: u32,
    ) -> Result<()> {
        let compressed = compress(content_stream)?;

        let stream_id = self.alloc_obj_id();
        self.begin_obj(stream_id)?;
        self.write_bytes(
            format!(
                "<< /Length {} /Filter /FlateDecode >>\nstream\n",
                compressed.len()
            )
            .as_bytes(),
        )?;
        self.write_bytes(&compressed)?;
        self.write_bytes(b"\nendstream\n")?;
        self.end_obj()?;

        let pages_id = self.pages_obj_id;
        let xobj_dict = self.build_xobject_dict();
        let page_id = self.alloc_obj_id();
        self.begin_obj(page_id)?;
        self.write_bytes(
            format!(
                concat!(
                    "<< /Type /Page",
                    " /Parent {pid} 0 R",
                    " /MediaBox [0 0 {pw:.2} {ph:.2}]",
                    " /Contents {sid} 0 R",
                    " /Resources << /Font {fid} 0 R{xobj}>>",
                    " >>\n"
                ),
                pid = pages_id,
                pw = page_width_pt,
                ph = page_height_pt,
                sid = stream_id,
                fid = font_dict_id,
                xobj = xobj_dict,
            )
            .as_bytes(),
        )?;
        self.end_obj()?;

        self.page_obj_ids.push(page_id);
        Ok(())
    }

    pub fn finish(mut self) -> Result<W> {
        let pages_id = self.pages_obj_id;
        self.begin_obj(pages_id)?;
        let kids: Vec<String> = self.page_obj_ids.iter().map(|id| format!("{id} 0 R")).collect();
        let count = self.page_obj_ids.len();
        self.write_bytes(
            format!(
                "<< /Type /Pages /Kids [{}] /Count {count} >>\n",
                kids.join(" ")
            )
            .as_bytes(),
        )?;
        self.end_obj()?;

        let catalog_id = self.alloc_obj_id();
        self.begin_obj(catalog_id)?;
        self.write_bytes(format!("<< /Type /Catalog /Pages {pages_id} 0 R >>\n").as_bytes())?;
        self.end_obj()?;

        let info_id = self.alloc_obj_id();
        self.begin_obj(info_id)?;
        self.write_bytes(b"<< /Producer (xlsxpdf 0.1) >>\n")?;
        self.end_obj()?;

        let xref_offset = self.offset;
        let obj_count = self.obj_offsets.len() + 1;
        self.write_bytes(format!("xref\n0 {obj_count}\n").as_bytes())?;
        self.write_bytes(b"0000000000 65535 f \n")?;
        let xref_lines: Vec<String> = self
            .obj_offsets
            .iter()
            .map(|off| format!("{off:010} 00000 n \n"))
            .collect();
        for line in &xref_lines {
            self.write_bytes(line.as_bytes())?;
        }

        self.write_bytes(
            format!(
                concat!(
                    "trailer\n",
                    "<< /Size {size} /Root {cat} 0 R /Info {info} 0 R >>\n",
                    "startxref\n{xref}\n%%EOF\n",
                ),
                size = obj_count,
                cat = catalog_id,
                info = info_id,
                xref = xref_offset,
            )
            .as_bytes(),
        )?;

        Ok(self.out)
    }

    fn write_bytes(&mut self, data: &[u8]) -> Result<()> {
        self.out.write_all(data).map_err(Error::Io)?;
        self.offset += data.len() as u64;
        Ok(())
    }
}

fn compress(data: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).map_err(Error::Io)?;
    encoder.finish().map_err(Error::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_pdf() {
        let buf = Vec::new();
        let mut writer = PdfWriter::new(buf).unwrap();
        let font_id = writer.write_font_resources().unwrap();

        let content = b"BT /F1 10 Tf 72 700 Td (Hello World) Tj ET";
        writer
            .write_page(content, 612.0, 792.0, font_id)
            .unwrap();

        let output = writer.finish().unwrap();
        let pdf = String::from_utf8_lossy(&output);
        assert!(pdf.starts_with("%PDF-1.7"));
        assert!(pdf.contains("/Type /Catalog"));
        assert!(pdf.contains("/Type /Page"));
        assert!(pdf.contains("/Parent 1 0 R"));
        assert!(pdf.contains("xref"));
        assert!(pdf.contains("%%EOF"));
    }

    #[test]
    fn test_multi_page_pdf() {
        let buf = Vec::new();
        let mut writer = PdfWriter::new(buf).unwrap();
        let font_id = writer.write_font_resources().unwrap();

        for i in 0..3 {
            let content = format!("BT /F1 10 Tf 72 700 Td (Page {}) Tj ET", i + 1);
            writer
                .write_page(content.as_bytes(), 612.0, 792.0, font_id)
                .unwrap();
        }

        let output = writer.finish().unwrap();
        let pdf = String::from_utf8_lossy(&output);
        assert!(pdf.contains("/Count 3"));
        assert!(pdf.contains("/Parent 1 0 R"));
    }

    // M14: Hardening tests

    #[test]
    fn test_empty_content_page() {
        let buf = Vec::new();
        let mut writer = PdfWriter::new(buf).unwrap();
        let font_id = writer.write_font_resources().unwrap();
        writer.write_page(b"", 612.0, 792.0, font_id).unwrap();
        let output = writer.finish().unwrap();
        let pdf = String::from_utf8_lossy(&output);
        assert!(pdf.contains("/Count 1"));
    }

    #[test]
    fn test_large_page_count() {
        let buf = Vec::new();
        let mut writer = PdfWriter::new(buf).unwrap();
        let font_id = writer.write_font_resources().unwrap();
        for _ in 0..100 {
            writer.write_page(b"q Q", 612.0, 792.0, font_id).unwrap();
        }
        let output = writer.finish().unwrap();
        let pdf = String::from_utf8_lossy(&output);
        assert!(pdf.contains("/Count 100"));
    }

    #[test]
    fn test_jpeg_xobject() {
        let buf = Vec::new();
        let mut writer = PdfWriter::new(buf).unwrap();
        let font_id = writer.write_font_resources().unwrap();
        let fake_jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        writer.write_jpeg_xobject("Im0", &fake_jpeg, 100, 50).unwrap();
        let content = b"q 100 0 0 50 72 700 cm /Im0 Do Q";
        writer.write_page(content, 612.0, 792.0, font_id).unwrap();
        let output = writer.finish().unwrap();
        let pdf = String::from_utf8_lossy(&output);
        assert!(pdf.contains("/DCTDecode"));
        assert!(pdf.contains("/Im0"));
    }

    #[test]
    fn test_binary_content() {
        let buf = Vec::new();
        let mut writer = PdfWriter::new(buf).unwrap();
        let font_id = writer.write_font_resources().unwrap();
        let content: Vec<u8> = (0..255).collect();
        writer.write_page(&content, 612.0, 792.0, font_id).unwrap();
        let _ = writer.finish().unwrap();
    }
}
