use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use tokenizers::{Tokenizer, TruncationDirection, TruncationParams, TruncationStrategy};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// MiniLM-L6 was trained with a 512-token positional embedding table. A
/// chunk longer than that produces a position id that overflows the
/// table; depending on which BLAS path candle picks the symptoms range
/// from a clean Result::Err to an aborted process (the C++ side bypasses
/// Rust panics, and catch_unwind doesn't see it). 256 leaves comfortable
/// headroom and is what most sentence-encoder retrieval setups use.
const MAX_TOKENS: usize = 256;
/// Hard char cap before we even tokenise. Tokenizers can spend a
/// surprising amount of time chewing through tens of MB of text only to
/// throw most of it away after truncation; this gates the pathological
/// pdftotext-output-on-one-line case at the door.
const MAX_INPUT_CHARS: usize = 4096;

pub struct LocalEmbedder {
    model: BertModel,
    tokenizer: Tokenizer,
}

impl LocalEmbedder {
    pub fn new() -> Result<Self> {
        let api = hf_hub::api::sync::Api::new()?;
        let repo = api.model("sentence-transformers/all-MiniLM-L6-v2".to_string());
        let tokenizer_path = repo.get("tokenizer.json")?;
        let weights_path = repo.get("model.safetensors")?;
        let config_path = repo.get("config.json")?;

        let mut tokenizer =
            Tokenizer::from_file(tokenizer_path).map_err(|e| format!("{}", e))?;

        // Force the tokenizer to truncate on the right at MAX_TOKENS so
        // embed() can never feed BertModel a sequence longer than the
        // model's positional embedding table. Without this, a single
        // long chunk (a PDF body extracted via pdftotext, no headings)
        // overflows the table and the process aborts.
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_TOKENS,
                direction: TruncationDirection::Right,
                strategy: TruncationStrategy::LongestFirst,
                stride: 0,
            }))
            .map_err(|e| format!("tokenizer truncation: {}", e))?;

        let device = Device::Cpu;
        let config: Config =
            serde_json::from_str(&std::fs::read_to_string(config_path)?)?;
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[weights_path], DType::F32, &device)?
        };
        let model = BertModel::load(vb, &config)?;

        Ok(Self { model, tokenizer })
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Pre-truncate at the char level so we don't ask the tokenizer to
        // chew through (and immediately discard) tens of MB of PDF text.
        let trimmed: String;
        let input: &str = if text.chars().count() > MAX_INPUT_CHARS {
            trimmed = text.chars().take(MAX_INPUT_CHARS).collect();
            &trimmed
        } else {
            text
        };

        let encoding = self
            .tokenizer
            .encode(input, true)
            .map_err(|e| format!("{}", e))?;
        let tokens = encoding.get_ids();
        // Final belt-and-braces: even with tokenizer truncation set, we
        // verify the slice is within MAX_TOKENS before constructing the
        // tensor. Any future regression in tokenizer config would still
        // be caught here.
        let safe_tokens = if tokens.len() > MAX_TOKENS {
            &tokens[..MAX_TOKENS]
        } else {
            tokens
        };
        if safe_tokens.is_empty() {
            return Err("empty token sequence after tokenisation".into());
        }
        let token_ids = Tensor::new(safe_tokens, &Device::Cpu)?.unsqueeze(0)?;
        let token_type_ids = token_ids.zeros_like()?;

        let embeddings = self.model.forward(&token_ids, &token_type_ids, None)?;

        // Mean pooling over the token dimension (dim=1).
        // embeddings shape: (1, seq_len, hidden_size)
        let (_, seq_len, _) = embeddings.dims3()?;
        let sum = embeddings.sum(1)?; // (1, hidden_size)
        let mean = (sum / seq_len as f64)?; // (1, hidden_size)
        let mean = mean.squeeze(0)?; // (hidden_size,) = 384 floats

        // L2 normalize.
        let norm = mean.sqr()?.sum_all()?.sqrt()?;
        let normalized = mean.broadcast_div(&norm)?;
        let vec: Vec<f32> = normalized.to_vec1()?;
        Ok(vec)
    }

    pub fn embed_batch(&self, texts: &[&str]) -> Vec<Option<Vec<f32>>> {
        texts.iter().map(|t| self.embed(t).ok()).collect()
    }

    pub fn dimensions() -> usize {
        384
    }
}
