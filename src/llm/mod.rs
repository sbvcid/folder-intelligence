pub mod chat;
pub mod config;
pub mod mock;
pub mod parser;
pub mod provider;

#[cfg(feature = "network")]
pub mod ollama;
#[cfg(feature = "network")]
pub mod openai;

#[allow(unused_imports)]
pub use chat::ChatCommand;
#[allow(unused_imports)]
pub use config::LlmConfig;
#[allow(unused_imports)]
pub use mock::MockLlmProvider;
#[allow(unused_imports)]
pub use parser::LlmIntentParser;
#[allow(unused_imports)]
pub use provider::{ChatMessage, LlmError, LlmProvider};

#[cfg(feature = "network")]
#[allow(unused_imports)]
pub use ollama::OllamaProvider;
#[cfg(feature = "network")]
#[allow(unused_imports)]
pub use openai::OpenAiCompatibleProvider;

#[allow(unused_imports)]
pub use chat::create_provider;
