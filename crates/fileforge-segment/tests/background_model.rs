//! Optional real-model check: point FILEFORGE_BACKGROUND_MODEL_DIR at the release files.
// Core
use std::path::PathBuf;
// Domain
use fileforge_core::raster::background::{self, Options, Segmenter};
// Adapters
use fileforge_segment::{Sam2, verify};

#[test]
fn real_model_produces_finite_masks_and_verified_cutouts() {
    let Some(dir) = std::env::var_os("FILEFORGE_BACKGROUND_MODEL_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let encoder = dir.join("sam2.1-hiera-tiny-encoder.onnx");
    let decoder = dir.join("sam2.1-hiera-tiny-decoder.onnx");
    verify(&encoder, 67624839, "6c6ef6bcf30cbc4e481d55d97756222847d7aa265a5159a0c551f8290c046de3")
        .expect("encoder pin");
    verify(&decoder, 14912939, "e53f68b0ff065fa617ff2717dffc49cefe40143ccf465b19070184f7adb5aff1")
        .expect("decoder pin");
    let start = std::time::Instant::now();
    let engine = Sam2::load(&encoder, &decoder).expect("model");
    println!("load: {:?}", start.elapsed());
    for fixture in ["chelsea.png", "rocket.jpg"] {
        let bytes = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fileforge-core/tests/fixtures").join(fixture),
        )
        .expect("fixture");
        let image = background::decode(&bytes).expect("decode");
        let start = std::time::Instant::now();
        let tensor = Sam2::input_tensor(&image.pixels).expect("tensor");
        let raw: Vec<u8> =
            tensor.to_plain_array_view::<f32>().expect("f32").iter().flat_map(|v| v.to_le_bytes()).collect();
        std::fs::write(dir.join(format!("{fixture}.input.f32")), raw).expect("reference input");
        let embedding = engine.encode_tensor(tensor).expect("encode");
        let reference = dir.join(format!("{fixture}.masks.f32"));
        if reference.exists() {
            let expected: Vec<f32> = std::fs::read(reference)
                .expect("reference")
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect();
            let outputs = engine.decode_tensors(&embedding, [0.5, 0.5]).expect("raw masks");
            let actual = outputs[0].to_plain_array_view::<f32>().expect("output");
            assert_eq!(actual.len(), expected.len());
            let diff = actual.iter().zip(&expected).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
            println!("{fixture} max logit difference vs ONNX Runtime: {diff}");
            assert!(diff < 0.001, "tract differs from ONNX Runtime: {diff}");
        }
        println!("{fixture} encoder: {:?}", start.elapsed());
        for automatic in [true, false] {
            let start = std::time::Instant::now();
            let mask = engine.mask(&embedding, [0.5, 0.5], automatic).expect("mask");
            println!("decoder: {:?}", start.elapsed());
            let visible = mask.logits.iter().filter(|v| **v > 0.0).count();
            assert!(visible > 100 && visible < 65536, "plausible foreground: {visible}");
            let pixels = background::render(&image, &mask, &Options::default(), &()).expect("render");
            assert!(pixels.pixels().any(|p| p[3] == 0));
            assert!(pixels.pixels().any(|p| p[3] == 255));
            let output = background::encode(&pixels, image.icc.as_deref(), background::OutputFormat::Png, &())
                .expect("verified PNG");
            std::fs::write(dir.join(format!("{fixture}.{automatic}.cutout.png")), output).expect("preview");
        }
    }
}
