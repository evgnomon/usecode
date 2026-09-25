// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Configurable fields for kick-starting the AI model container
//! (llama-server).
//!
//! Defaults reproduce:
//!   llama-server -hf ggml-org/Qwen3-0.6B-GGUF:Q4_0 --device Vulkan0 -ngl 99 \
//!     --alias local-model -c 32768 --host 127.0.0.1 --port 8080

use serde_json::{Map, Value, json};

pub struct ModelField {
    pub name: &'static str,
    pub default: Value,
    pub options: Option<Vec<Value>>,
    pub description: &'static str,
    /// Integer fields; everything else is a string.
    pub integer: bool,
}

fn values<T: Into<Value> + Clone>(items: &[T]) -> Option<Vec<Value>> {
    Some(items.iter().cloned().map(Into::into).collect())
}

pub fn fields() -> Vec<ModelField> {
    vec![
        ModelField {
            name: "image",
            default: json!("ghcr.io/ggml-org/llama.cpp:server-vulkan"),
            options: values(&[
                "ghcr.io/ggml-org/llama.cpp:server-vulkan",
                "ghcr.io/ggml-org/llama.cpp:server-cuda",
                "ghcr.io/ggml-org/llama.cpp:server",
            ]),
            description: "Container image that runs llama-server",
            integer: false,
        },
        ModelField {
            name: "hf_repo",
            default: json!("ggml-org/Qwen3-0.6B-GGUF:Q4_0"),
            options: values(&[
                "ggml-org/Qwen3-0.6B-GGUF:Q4_0",
                "ggml-org/Qwen3-1.7B-GGUF:Q4_0",
                "ggml-org/Qwen3-4B-GGUF:Q4_0",
                "ggml-org/Qwen3-8B-GGUF:Q4_0",
                "ggml-org/gemma-3-4b-it-GGUF:Q4_K_M",
                "bartowski/Llama-3.2-3B-Instruct-GGUF:Q4_K_M",
            ]),
            description: "HuggingFace GGUF repo:quant to load with -hf",
            integer: false,
        },
        ModelField {
            name: "device",
            default: json!("Vulkan0"),
            options: values(&["Vulkan0", "Vulkan1", "CPU", "CUDA0", "CUDA1"]),
            description: "Backend device passed to --device",
            integer: false,
        },
        ModelField {
            name: "ngl",
            default: json!(99),
            options: None,
            description: "Number of layers offloaded to GPU (-ngl)",
            integer: true,
        },
        ModelField {
            name: "alias",
            default: json!("local-model"),
            options: None,
            description: "Model alias exposed by the server (--alias)",
            integer: false,
        },
        ModelField {
            name: "ctx_size",
            default: json!(32768),
            options: values(&[2048, 4096, 8192, 16384, 32768, 65536, 131072]),
            description: "Context window size (-c)",
            integer: true,
        },
        ModelField {
            name: "host",
            default: json!("127.0.0.1"),
            options: None,
            description: "Host interface the container port is published on",
            integer: false,
        },
        ModelField {
            name: "port",
            default: json!(8080),
            options: None,
            description: "Host port the container is published on",
            integer: true,
        },
    ]
}

pub fn known_options() -> Vec<Value> {
    fields()
        .into_iter()
        .map(|field| {
            json!({
                "name": field.name,
                "default": field.default,
                "options": field.options,
                "description": field.description,
            })
        })
        .collect()
}

/// Merge overrides onto the defaults and validate each against its field's
/// type and options. Unknown keys and nulls are ignored.
pub fn resolve(overrides: &Map<String, Value>) -> Result<Map<String, Value>, String> {
    let mut merged = Map::new();
    for field in fields() {
        let value = match overrides.get(field.name) {
            None | Some(Value::Null) => field.default.clone(),
            Some(value) => value.clone(),
        };
        let typed = if field.integer {
            value.is_i64()
        } else {
            value.is_string()
        };
        if !typed {
            let kind = if field.integer {
                "an integer"
            } else {
                "a string"
            };
            return Err(format!(
                "Invalid value {value} for '{}'; expected {kind}",
                field.name
            ));
        }
        if let Some(options) = &field.options
            && !options.contains(&value)
        {
            let choices: Vec<String> = options.iter().map(Value::to_string).collect();
            return Err(format!(
                "Invalid value {value} for '{}'; choose one of [{}]",
                field.name,
                choices.join(", ")
            ));
        }
        merged.insert(field.name.to_string(), value);
    }
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_overrides() {
        let config = resolve(&Map::new()).unwrap();
        assert_eq!(config["hf_repo"], "ggml-org/Qwen3-0.6B-GGUF:Q4_0");
        assert_eq!(config["port"], 8080);

        let overrides = json!({"hf_repo": "ggml-org/Qwen3-4B-GGUF:Q4_0", "ngl": null});
        let config = resolve(overrides.as_object().unwrap()).unwrap();
        assert_eq!(config["hf_repo"], "ggml-org/Qwen3-4B-GGUF:Q4_0");
        assert_eq!(config["ngl"], 99);

        assert!(resolve(json!({"device": "TPU"}).as_object().unwrap()).is_err());
        assert!(resolve(json!({"port": "80"}).as_object().unwrap()).is_err());
    }
}
