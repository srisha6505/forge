//! Local Silero VAD wrapper backed by ONNX Runtime.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use ort::{session::Session, value::Tensor};

const MODEL_URL: &str =
    "https://raw.githubusercontent.com/snakers4/silero-vad/v6.1/src/silero_vad/data/silero_vad.onnx";
const MODEL_FILENAME: &str = "silero_vad.onnx";
const SAMPLE_RATE_16K: i64 = 16_000;
const CHUNK_SAMPLES_16K: usize = 512;
const CONTEXT_SAMPLES_16K: usize = 64;
const STATE_FLOATS: usize = 2 * 1 * 128;

fn model_dir() -> PathBuf {
    let dir = crate::models::data_dir().join("vad");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn model_path() -> PathBuf {
    model_dir().join(MODEL_FILENAME)
}

pub fn ensure_model() -> Result<PathBuf, String> {
    let path = model_path();
    if path.is_file() {
        return Ok(path);
    }

    let parent = path
        .parent()
        .ok_or_else(|| "silero model path missing parent".to_string())?;
    fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;

    let tmp = path.with_extension("onnx.part");
    let resp = ureq::get(MODEL_URL)
        .call()
        .map_err(|e| format!("download silero vad model: {e}"))?;
    let mut reader = resp.into_reader();
    let mut out = fs::File::create(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("read silero vad model: {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])
            .map_err(|e| format!("write {}: {e}", tmp.display()))?;
    }
    drop(out);
    fs::rename(&tmp, &path).map_err(|e| format!("move {} -> {}: {e}", tmp.display(), path.display()))?;
    Ok(path)
}

pub struct SileroVad {
    session: Session,
    state: [f32; STATE_FLOATS],
    context: [f32; CONTEXT_SAMPLES_16K],
}

impl SileroVad {
    pub fn new() -> Result<Self, String> {
        let path = ensure_model()?;
        let session = Session::builder()
            .map_err(fmt_ort)?
            .with_intra_threads(1)
            .map_err(fmt_ort)?
            .with_inter_threads(1)
            .map_err(fmt_ort)?
            .commit_from_file(&path)
            .map_err(fmt_ort)?;
        Ok(Self {
            session,
            state: [0.0; STATE_FLOATS],
            context: [0.0; CONTEXT_SAMPLES_16K],
        })
    }

    pub fn reset_states(&mut self) {
        self.state.fill(0.0);
        self.context.fill(0.0);
    }

    pub fn probability_16khz(&mut self, chunk: &[f32]) -> Result<f32, String> {
        if chunk.len() != CHUNK_SAMPLES_16K {
            return Err(format!(
                "silero vad expects {CHUNK_SAMPLES_16K} samples, got {}",
                chunk.len()
            ));
        }

        let mut input = Vec::with_capacity(CONTEXT_SAMPLES_16K + CHUNK_SAMPLES_16K);
        input.extend_from_slice(&self.context);
        input.extend_from_slice(chunk);

        let outputs = self
            .session
            .run(ort::inputs![
                "input" => Tensor::from_array(([1_i64, input.len() as i64], input.clone().into_boxed_slice())).map_err(fmt_ort)?,
                "state" => Tensor::from_array(([2_i64, 1_i64, 128_i64], self.state.to_vec().into_boxed_slice())).map_err(fmt_ort)?,
                "sr" => Tensor::from_array(((), vec![SAMPLE_RATE_16K])).map_err(fmt_ort)?,
            ])
            .map_err(fmt_ort)?;

        // Output shape is [batch, 1] so extract as tensor and read first
        // element. try_extract_scalar only accepts truly 0-dim tensors.
        let (_, prob_data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(fmt_ort)?;
        let prob = *prob_data.first().ok_or("silero vad: empty output")?;
        let (_, new_state) = outputs[1]
            .try_extract_tensor::<f32>()
            .map_err(fmt_ort)?;
        if new_state.len() != STATE_FLOATS {
            return Err(format!(
                "silero vad state size mismatch: expected {STATE_FLOATS}, got {}",
                new_state.len()
            ));
        }
        self.state.copy_from_slice(new_state);
        self.context.copy_from_slice(&input[input.len() - CONTEXT_SAMPLES_16K..]);
        Ok(prob)
    }
}

fn fmt_ort<E: std::fmt::Display>(err: E) -> String {
    format!("onnx runtime: {err}")
}

#[allow(dead_code)]
fn _is_model_present(path: &Path) -> bool {
    path.is_file()
}
