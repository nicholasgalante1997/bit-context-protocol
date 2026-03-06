"""
Ollama client for response generation using the OpenAI-compatible API.

=== What is Ollama? ===
Ollama is a local model server that runs LLMs on your machine. On the DGX Spark,
it runs in a Docker container and exposes an API at http://localhost:11434.

=== Why OpenAI-compatible API? ===
Ollama provides an endpoint at /v1/chat/completions that matches the OpenAI API
format exactly. This means we can use the standard `openai` Python package to
talk to our local models. If you ever want to switch to a cloud API (OpenAI,
Anthropic via proxy, etc.), you only change the base_url and api_key.

=== Available models on this DGX Spark ===
  qwen3-coder-next:q8_0  (79GB) — Primary: frontier code model, Q8 quantization
  devstral-2:123b         (70GB) — Alternative: strong general + code
  gpt-oss:120b            (61GB) — Alternative: general purpose
  qwen2.5-coder:32b      (18GB) — Lightweight fallback

Usage:
    client = OllamaClient(model="qwen3-coder-next:q8_0")
    response = client.generate("Review this code...", temperature=0.3)
"""

from openai import OpenAI


class OllamaClient:
    """
    Wrapper around Ollama's OpenAI-compatible API.

    The DGX Spark runs Ollama in Docker, typically accessible at localhost:11434.
    The /v1 endpoint speaks the OpenAI chat completions protocol.

    Attributes:
        client: The OpenAI Python client, pointed at Ollama's local server.
        model: Which Ollama model to use (must be already pulled/cached).
    """

    def __init__(
        self,
        model: str = "qwen3-coder-next:q8_0",
        base_url: str = "http://localhost:11434/v1",
    ):
        # The OpenAI client doesn't need a real API key for Ollama,
        # but the library requires _something_ in the field.
        self.client = OpenAI(base_url=base_url, api_key="ollama")
        self.model = model

    def generate(
        self,
        prompt: str,
        system_prompt: str = "",
        temperature: float = 0.3,
        max_tokens: int = 2048,
    ) -> str:
        """
        Generate a response from the local model.

        Args:
            prompt: The user message (context + task).
            system_prompt: Optional system instructions for the model.
            temperature: Controls randomness. 0.0 = deterministic, 1.0 = creative.
                         We use 0.3 for consistent, correct code responses.
            max_tokens: Maximum length of the generated response in tokens.

        Returns:
            The model's response text.

        Raises:
            openai.APIConnectionError: If Ollama is not running.
            openai.APIError: If the model is not available.
        """
        messages = []
        if system_prompt:
            messages.append({"role": "system", "content": system_prompt})
        messages.append({"role": "user", "content": prompt})

        response = self.client.chat.completions.create(
            model=self.model,
            messages=messages,
            temperature=temperature,
            max_tokens=max_tokens,
        )

        return response.choices[0].message.content

    def is_available(self) -> bool:
        """
        Check if Ollama is running and the model is accessible.
        Call this before starting a long data generation run.
        """
        try:
            self.client.models.list()
            return True
        except Exception:
            return False
