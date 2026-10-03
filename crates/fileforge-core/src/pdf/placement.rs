//! How large each image is drawn, from the page content streams. Drives DPI-based downsampling: an image is only
//! downsampled when every placement found is smaller than its resolution warrants.

// Core
use std::collections::{HashMap, HashSet};

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};
// Domain
use super::objects::{dict_name, number, resolve, resolve_dict};

/// Form XObjects nested deeper than this are not inspected.
const MAX_FORM_DEPTH: usize = 12;

/// Largest size, in PDF points (1/72 inch), at which an image is drawn anywhere in the document.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DisplaySize {
    pub width_pt: f64,
    pub height_pt: f64,
}

/// Affine transform `[a b c d e f]` in PDF's row-vector convention.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Matrix([f64; 6]);

impl Matrix {
    pub const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    fn from_operands(operands: &[Object]) -> Option<Self> {
        let values: Vec<f64> = operands.iter().map(number).collect::<Option<_>>()?;
        let values: [f64; 6] = values.try_into().ok()?;
        values.iter().all(|value| value.is_finite()).then_some(Self(values))
    }

    /// `self × other`: apply `self` first, then `other` (how `cm` and form matrices combine with the CTM).
    pub fn then(self, other: Self) -> Self {
        let [a, b, c, d, e, f] = self.0;
        let [oa, ob, oc, od, oe, of] = other.0;
        Self([
            a * oa + b * oc,
            a * ob + b * od,
            c * oa + d * oc,
            c * ob + d * od,
            e * oa + f * oc + oe,
            e * ob + f * od + of,
        ])
    }

    /// Length of the image's unit-square edges after transformation: its drawn width and height in points.
    fn unit_square_size(self) -> DisplaySize {
        let [a, b, c, d, _, _] = self.0;
        DisplaySize { width_pt: a.hypot(b), height_pt: c.hypot(d) }
    }
}

/// Maps image object ids to the largest size they are drawn at. Images not found here (annotations, patterns,
/// soft masks) are never downsampled.
pub(crate) fn image_display_sizes(doc: &Document, max_stream_bytes: usize) -> HashMap<ObjectId, DisplaySize> {
    let mut sizes = HashMap::new();
    for page_id in doc.get_pages().into_values() {
        let resources = page_resources(doc, page_id);
        let mut content = Vec::new();
        for content_id in doc.get_page_contents(page_id) {
            if let Some(Object::Stream(stream)) = doc.objects.get(&content_id)
                && let Ok(data) = stream.get_plain_content_with_limit(max_stream_bytes)
            {
                content.extend_from_slice(&data);
                content.push(b'\n');
            }
        }
        let mut walker = Walker { doc, max_stream_bytes, sizes: &mut sizes, forms_in_progress: HashSet::new() };
        walker.walk(&content, &resources, Matrix::IDENTITY, 0);
    }
    sizes
}

/// The page's resource dictionaries, nearest first (page, then inherited from parents).
fn page_resources(doc: &Document, page_id: ObjectId) -> Vec<&Dictionary> {
    let Ok((inline, inherited)) = doc.get_page_resources(page_id) else { return Vec::new() };
    inline.into_iter().chain(inherited.into_iter().filter_map(|id| doc.get_dictionary(id).ok())).collect()
}

struct Walker<'a, 's> {
    doc: &'a Document,
    max_stream_bytes: usize,
    sizes: &'s mut HashMap<ObjectId, DisplaySize>,
    forms_in_progress: HashSet<ObjectId>,
}

impl<'a> Walker<'a, '_> {
    fn walk(&mut self, content: &[u8], resources: &[&'a Dictionary], base: Matrix, depth: usize) {
        let Ok(content) = Content::decode(content) else { return };
        let mut ctm = base;
        let mut saved = Vec::new();
        for operation in &content.operations {
            match operation.operator.as_str() {
                "q" => saved.push(ctm),
                "Q" => ctm = saved.pop().unwrap_or(base),
                "cm" => {
                    if let Some(matrix) = Matrix::from_operands(&operation.operands) {
                        ctm = matrix.then(ctm);
                    }
                }
                "Do" => {
                    if let Some(Ok(name)) = operation.operands.first().map(Object::as_name) {
                        self.draw(name, resources, ctm, depth);
                    }
                }
                _ => {}
            }
        }
    }

    fn draw(&mut self, name: &[u8], resources: &[&'a Dictionary], ctm: Matrix, depth: usize) {
        let Some(id) = self.xobject_id(name, resources) else { return };
        let Some(Object::Stream(stream)) = self.doc.objects.get(&id) else { return };
        match dict_name(self.doc, &stream.dict, b"Subtype") {
            Some(b"Image") => {
                let drawn = ctm.unit_square_size();
                let entry = self.sizes.entry(id).or_insert(DisplaySize { width_pt: 0.0, height_pt: 0.0 });
                entry.width_pt = entry.width_pt.max(drawn.width_pt);
                entry.height_pt = entry.height_pt.max(drawn.height_pt);
            }
            Some(b"Form") if depth < MAX_FORM_DEPTH && self.forms_in_progress.insert(id) => {
                let form_matrix = stream
                    .dict
                    .get(b"Matrix")
                    .ok()
                    .and_then(|m| resolve(self.doc, m))
                    .and_then(|m| m.as_array().ok())
                    .and_then(|m| Matrix::from_operands(m))
                    .unwrap_or(Matrix::IDENTITY);
                let mut form_resources = Vec::with_capacity(resources.len() + 1);
                if let Some(own) = stream.dict.get(b"Resources").ok().and_then(|r| resolve_dict(self.doc, r)) {
                    form_resources.push(own);
                }
                form_resources.extend_from_slice(resources);
                if let Ok(content) = stream.get_plain_content_with_limit(self.max_stream_bytes) {
                    self.walk(&content, &form_resources, form_matrix.then(ctm), depth + 1);
                }
                self.forms_in_progress.remove(&id);
            }
            _ => {}
        }
    }

    fn xobject_id(&self, name: &[u8], resources: &[&'a Dictionary]) -> Option<ObjectId> {
        resources.iter().find_map(|dict| {
            let xobjects = resolve_dict(self.doc, dict.get(b"XObject").ok()?)?;
            xobjects.get(name).ok()?.as_reference().ok()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matrix(values: [f64; 6]) -> Matrix {
        Matrix(values)
    }

    #[test]
    fn scaling_then_translation_composes_like_pdf() {
        let scale = matrix([200.0, 0.0, 0.0, 100.0, 0.0, 0.0]);
        let translate = matrix([1.0, 0.0, 0.0, 1.0, 50.0, 60.0]);
        assert_eq!(scale.then(translate), matrix([200.0, 0.0, 0.0, 100.0, 50.0, 60.0]));
        assert_eq!(translate.then(scale), matrix([200.0, 0.0, 0.0, 100.0, 10_000.0, 6_000.0]));
    }

    #[test]
    fn rotated_placement_keeps_edge_lengths() {
        let (sin, cos) = 30f64.to_radians().sin_cos();
        let rotated = matrix([cos, sin, -sin, cos, 0.0, 0.0]);
        let size = matrix([288.0, 0.0, 0.0, 144.0, 0.0, 0.0]).then(rotated).unit_square_size();
        assert!((size.width_pt - 288.0).abs() < 1e-9);
        assert!((size.height_pt - 144.0).abs() < 1e-9);
    }

    #[test]
    fn operands_must_be_six_finite_numbers() {
        assert!(Matrix::from_operands(&[1.into(), 0.into(), 0.into(), 1.into(), 0.into()]).is_none());
        let with_nan = [Object::Real(f32::NAN), 0.into(), 0.into(), 1.into(), 0.into(), 0.into()];
        assert!(Matrix::from_operands(&with_nan).is_none());
        let valid = [Object::Real(0.5), 0.into(), 0.into(), 2.into(), 10.into(), 20.into()];
        assert_eq!(Matrix::from_operands(&valid), Some(matrix([0.5, 0.0, 0.0, 2.0, 10.0, 20.0])));
    }
}
