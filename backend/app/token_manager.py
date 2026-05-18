"""
token_manager.py — Token/TPM overflow prevention utilities.

Bước 1: Ước lượng token, kiểm tra trước khi gọi LLM.
Bước 2: Cung cấp ngưỡng an toàn có buffer cho từng provider.
Bước 3: Phát hiện lỗi rate_limit để hỗ trợ retry thông minh.
"""

import logging

logger = logging.getLogger(__name__)

# ~4 chars per token (English/Vietnamese mixed average)
CHARS_PER_TOKEN: float = 4.0

# Overhead for prompt template + system instructions (estimated tokens)
PROMPT_OVERHEAD_TOKENS: int = 600

# Max retries when rate limit is hit
MAX_RETRIES: int = 3

# Exponential backoff base (seconds): 1 → 2 → 4
INITIAL_BACKOFF_SECONDS: float = 1.0

# Reduce chunk size by this factor after persistent rate limit errors
CHUNK_REDUCTION_FACTOR: float = 0.7

# Safe input token limits per provider/model at ~65% of actual limits.
# Balances: context window, TPM (tokens-per-minute), and output space.
PROVIDER_SAFE_INPUT_TOKENS: dict = {
    "claude": {
        # Claude 3.x has 200k context window — be conservative for cost/latency
        "default": 100_000,
    },
    "groq": {
        # Groq free tier: ~14,400 TPM → use ~6,000 tokens per chunk
        # (leaves headroom for output + multiple chunks per minute)
        "default": 6_000,
        "gemma2-9b": 4_000,   # smaller context model
    },
    "openai": {
        "default":           10_000,
        "gpt-3.5-turbo":     10_000,
        "gpt-4":             80_000,
        "gpt-4-turbo":       80_000,
        "gpt-4o":            80_000,
        "gpt-4o-mini":       80_000,
    },
    "ollama": {
        # Local — no TPM, but respect model context window
        "default": 20_000,
        "phi4":    8_000,
        "llama":   8_000,
    },
}


# ─── Core helpers ────────────────────────────────────────────────────────────

def estimate_tokens(text: str) -> int:
    """Rough token estimate using char count (~4 chars/token)."""
    return max(1, int(len(text) / CHARS_PER_TOKEN))


def tokens_to_chars(tokens: int) -> int:
    """Convert approximate token count back to character count."""
    return int(tokens * CHARS_PER_TOKEN)


def get_safe_input_tokens(provider: str, model_name: str) -> int:
    """Return safe input-token limit for a given provider + model (with buffer)."""
    provider_limits = PROVIDER_SAFE_INPUT_TOKENS.get(provider, {"default": 5_000})
    model_lower = (model_name or "").lower()

    # Partial-match against model families (e.g. "gpt-4o" matches "gpt-4o-mini")
    for key, limit in provider_limits.items():
        if key != "default" and key in model_lower:
            return limit

    return provider_limits.get("default", 5_000)


def get_safe_chunk_chars(provider: str, model_name: str) -> int:
    """Return safe chunk size in *characters* (content only, prompt overhead subtracted)."""
    safe_tokens = get_safe_input_tokens(provider, model_name)
    content_tokens = max(200, safe_tokens - PROMPT_OVERHEAD_TOKENS)
    return tokens_to_chars(content_tokens)


def is_rate_limit_error(exc: Exception) -> bool:
    """Detect rate-limit / token-quota errors from any provider (TPM and TPD)."""
    msg = str(exc).lower()
    rate_limit_keywords = [
        "rate_limit",
        "ratelimit",
        "rate limit",
        "too many requests",
        "status code: 429",
        "status_code=429",
        "http 429",
        "tokens per minute",
        "tpm exceeded",
        "quota exceeded",
        "capacity exceeded",
        "overloaded_error",
        "overloaded",
        "service_unavailable",
        # TPD (tokens per day) keywords
        "tokens per day",
        "tokens_per_day",
        "tpd exceeded",
        "daily limit",
        "daily token",
        "per day",
        "day limit",
    ]
    return any(kw in msg for kw in rate_limit_keywords)


def is_daily_limit_error(exc: Exception) -> bool:
    """Phân biệt lỗi TPD (giới hạn theo ngày) với lỗi TPM (giới hạn theo phút).

    Lỗi TPD cần chờ hàng giờ — không nên retry ngay lập tức.
    """
    msg = str(exc).lower()
    daily_keywords = [
        "tokens per day",
        "tokens_per_day",
        "tpd",
        "daily limit",
        "daily token",
        "per day",
        "day limit",
    ]
    return any(kw in msg for kw in daily_keywords)


def extract_retry_after(exc: Exception) -> str:
    """Trích xuất thời gian chờ từ message lỗi, ví dụ 'Please try again in 2h18m2.304s'."""
    import re
    msg = str(exc)
    match = re.search(r'[Pp]lease try again in\s+([\d hms.]+)', msg)
    return match.group(1).strip() if match else ""
