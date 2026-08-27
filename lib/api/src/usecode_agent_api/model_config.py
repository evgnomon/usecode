"""Configurable fields for kick-starting the AI model container (llama-server).

Defaults reproduce:
  llama-server -hf ggml-org/Qwen3-0.6B-GGUF:Q4_0 --device Vulkan0 -ngl 99 \
    --alias local-model -c 32768 --host 127.0.0.1 --port 8080
"""

from dataclasses import dataclass


@dataclass(frozen=True)
class ModelField:
    name: str
    default: object
    options: list | None = None
    description: str = ""


MODEL_FIELDS: list[ModelField] = [
    ModelField(
        "image",
        "ghcr.io/ggml-org/llama.cpp:server-vulkan",
        options=[
            "ghcr.io/ggml-org/llama.cpp:server-vulkan",
            "ghcr.io/ggml-org/llama.cpp:server-cuda",
            "ghcr.io/ggml-org/llama.cpp:server",
        ],
        description="Container image that runs llama-server",
    ),
    ModelField(
        "hf_repo",
        "ggml-org/Qwen3-0.6B-GGUF:Q4_0",
        options=[
            "ggml-org/Qwen3-0.6B-GGUF:Q4_0",
            "ggml-org/Qwen3-1.7B-GGUF:Q4_0",
            "ggml-org/Qwen3-4B-GGUF:Q4_0",
            "ggml-org/Qwen3-8B-GGUF:Q4_0",
            "ggml-org/gemma-3-4b-it-GGUF:Q4_K_M",
            "bartowski/Llama-3.2-3B-Instruct-GGUF:Q4_K_M",
        ],
        description="HuggingFace GGUF repo:quant to load with -hf",
    ),
    ModelField(
        "device",
        "Vulkan0",
        options=["Vulkan0", "Vulkan1", "CPU", "CUDA0", "CUDA1"],
        description="Backend device passed to --device",
    ),
    ModelField(
        "ngl",
        99,
        options=None,
        description="Number of layers offloaded to GPU (-ngl)",
    ),
    ModelField(
        "alias",
        "local-model",
        options=None,
        description="Model alias exposed by the server (--alias)",
    ),
    ModelField(
        "ctx_size",
        32768,
        options=[2048, 4096, 8192, 16384, 32768, 65536, 131072],
        description="Context window size (-c)",
    ),
    ModelField(
        "host",
        "127.0.0.1",
        options=None,
        description="Host interface the container port is published on",
    ),
    ModelField(
        "port",
        8080,
        options=None,
        description="Host port the container is published on",
    ),
]

DEFAULTS: dict[str, object] = {field.name: field.default for field in MODEL_FIELDS}


def known_options() -> list[dict]:
    return [
        {
            "name": field.name,
            "default": field.default,
            "options": field.options,
            "description": field.description,
        }
        for field in MODEL_FIELDS
    ]


def resolve(overrides: dict[str, object]) -> dict[str, object]:
    """Merge overrides onto the defaults and validate against each field's options."""
    merged = dict(DEFAULTS)
    merged.update({k: v for k, v in overrides.items() if v is not None})
    for field in MODEL_FIELDS:
        if field.options and merged[field.name] not in field.options:
            raise ValueError(
                f"Invalid value {merged[field.name]!r} for {field.name!r}; "
                f"choose one of {field.options}"
            )
    return merged
