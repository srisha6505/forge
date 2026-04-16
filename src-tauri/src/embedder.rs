use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use tokenizers::Tokenizer;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

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

        let tokenizer =
            Tokenizer::from_file(tokenizer_path).map_err(|e| format!("{}", e))?;

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
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| format!("{}", e))?;
        let tokens = encoding.get_ids();
        let token_ids = Tensor::new(tokens, &Device::Cpu)?.unsqueeze(0)?;
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
