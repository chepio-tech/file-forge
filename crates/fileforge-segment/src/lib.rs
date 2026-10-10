//! SAM 2.1 adapter. Only hash-verified, application-pinned graphs reach tract.
// Core
use image::{RgbaImage, imageops};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use tract_linalg::multithread::{Executor, multithread_tract_scope};
use tract_onnx::prelude::*;
// Ports
use fileforge_core::raster::background::{Mask, Segmenter};
// Types
use fileforge_core::raster::RasterError;

type Model = Arc<TypedRunnableModel>;
pub struct Sam2 {
    encoder: Model,
    decoder: Model,
    executor: Executor,
}
pub struct Embedding {
    pub features: TVec<TValue>,
}
fn internal(e: impl std::fmt::Display) -> RasterError {
    RasterError::Internal(e.to_string())
}

/// Stream the verification before the ONNX parser allocates anything.
pub fn verify(path: &Path, size: u64, hash: &str) -> Result<(), RasterError> {
    let mut file = std::fs::File::open(path).map_err(internal)?;
    if file.metadata().map_err(internal)?.len() != size {
        return Err(internal("model size mismatch"));
    }
    let mut sha = Sha256::new();
    let mut buf = [0; 65536];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buf).map_err(internal)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > size {
            return Err(internal("model grew"));
        }
        sha.update(&buf[..n]);
    }
    if total != size || format!("{:x}", sha.finalize()) != hash {
        return Err(internal("model hash mismatch"));
    }
    Ok(())
}
impl Sam2 {
    /// Call only after `verify` on both files, in the shell's protected model directory.
    pub fn load(encoder: &Path, decoder: &Path) -> Result<Self, RasterError> {
        let load = |path: &Path| -> Result<Model, RasterError> {
            tract_onnx::onnx()
                .with_ignore_value_info(true)
                .model_for_path(path)
                .map_err(internal)?
                .into_optimized()
                .map_err(internal)?
                .into_runnable()
                .map_err(internal)
        };
        let executor = Executor::multithread_with_name(
            std::thread::available_parallelism().map_or(1, usize::from).min(4),
            "fileforge-sam",
        );
        Ok(Self { encoder: load(encoder)?, decoder: load(decoder)?, executor })
    }
    pub fn input_tensor(image: &RgbaImage) -> Result<Tensor, RasterError> {
        let resized = imageops::resize(image, 1024, 1024, imageops::FilterType::Triangle);
        let mut data = vec![0f32; 3 * 1024 * 1024];
        let mean = [0.485, 0.456, 0.406];
        let std = [0.229, 0.224, 0.225];
        for (i, p) in resized.pixels().enumerate() {
            for c in 0..3 {
                data[c * 1024 * 1024 + i] = (f32::from(p[c]) / 255.0 - mean[c]) / std[c];
            }
        }
        Tensor::from_shape(&[1, 3, 1024, 1024], &data).map_err(internal)
    }
    pub fn encode_tensor(&self, tensor: Tensor) -> Result<Embedding, RasterError> {
        let features = multithread_tract_scope(self.executor.clone(), || self.encoder.run(tvec!(tensor.into())))
            .map_err(internal)?;
        if features.len() != 3 {
            return Err(internal("encoder outputs"));
        }
        Ok(Embedding { features })
    }
    pub fn decode_tensors(&self, embedding: &Embedding, point: [f32; 2]) -> Result<TVec<TValue>, RasterError> {
        let coords =
            Tensor::from_shape(&[1, 2, 2], &[point[0] * 1024.0, point[1] * 1024.0, 0.0, 0.0]).map_err(internal)?;
        let labels = Tensor::from_shape(&[1, 2], &[1.0f32, -1.0]).map_err(internal)?;
        let zeros = Tensor::zero::<f32>(&[1, 1, 256, 256]).map_err(internal)?;
        let has = Tensor::from_shape(&[1], &[0.0f32]).map_err(internal)?;
        let mut inputs = embedding.features.clone();
        inputs.extend([coords.into(), labels.into(), zeros.into(), has.into()]);
        multithread_tract_scope(self.executor.clone(), || self.decoder.run(inputs)).map_err(internal)
    }
}
impl Segmenter for Sam2 {
    type Embedding = Embedding;
    fn embed(&self, image: &RgbaImage) -> Result<Embedding, RasterError> {
        self.encode_tensor(Self::input_tensor(image)?)
    }
    fn mask(&self, embedding: &Embedding, point: [f32; 2], automatic: bool) -> Result<Mask, RasterError> {
        if point.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)) {
            return Err(RasterError::InvalidOptions("point".into()));
        }
        let outputs = self.decode_tensors(embedding, point)?;
        if outputs.len() != 3 || outputs[0].shape() != [1, 4, 256, 256] || outputs[1].shape() != [1, 4] {
            return Err(internal("decoder shape"));
        }
        let masks: Vec<f32> = outputs[0].to_plain_array_view::<f32>().map_err(internal)?.iter().copied().collect();
        let scores: Vec<f32> = outputs[1].to_plain_array_view::<f32>().map_err(internal)?.iter().copied().collect();
        if masks.iter().chain(scores.iter()).any(|v| !v.is_finite()) {
            return Err(internal("non-finite decoder output"));
        }
        let chosen = (1..4)
            .max_by(|a, b| {
                if automatic {
                    let size = |i: usize| masks[i * 65536..(i + 1) * 65536].iter().filter(|v| **v > 0.0).count();
                    size(*a).cmp(&size(*b))
                } else {
                    scores[*a].total_cmp(&scores[*b])
                }
            })
            .ok_or_else(|| internal("mask selection"))?;
        Ok(Mask { width: 256, height: 256, logits: masks[chosen * 65536..(chosen + 1) * 65536].to_vec() })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hash_checks_reject_missing_models() {
        assert!(verify(Path::new("definitely-missing.onnx"), 1, &"0".repeat(64)).is_err());
    }
}
