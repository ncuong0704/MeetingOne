from pydantic import BaseModel
from typing import List, Tuple, Literal
from pydantic_ai import Agent
from pydantic_ai.models.anthropic import AnthropicModel
from pydantic_ai.models.groq import GroqModel
from pydantic_ai.models.openai import OpenAIModel
from pydantic_ai.providers.openai import OpenAIProvider
from pydantic_ai.providers.groq import GroqProvider
from pydantic_ai.providers.anthropic import AnthropicProvider

import logging
import os
from dotenv import load_dotenv
from db import DatabaseManager
import asyncio
from ollama import AsyncClient
from token_manager import (
    estimate_tokens,
    get_safe_chunk_chars,
    is_rate_limit_error,
    MAX_RETRIES,
    INITIAL_BACKOFF_SECONDS,
    CHUNK_REDUCTION_FACTOR,
)





# Set up logging
logging.basicConfig(
    level=logging.DEBUG,
    format='%(asctime)s - %(levelname)s - [%(filename)s:%(lineno)d] - %(message)s'
)
logger = logging.getLogger(__name__)

load_dotenv()  # Load environment variables from .env file

db = DatabaseManager()

# ── Language validation helpers ──────────────────────────────────────────────
_COMMON_ENGLISH_WORDS = {'the', 'and', 'for', 'are', 'was', 'were', 'this', 'that', 'with', 'have', 'from', 'they', 'will', 'been', 'their', 'about'}

def _is_likely_english(text: str) -> bool:
    """Return True if the text appears to be English rather than Vietnamese."""
    words = set(text.lower().split()[:30])
    return len(words & _COMMON_ENGLISH_WORDS) >= 3

def _validate_vietnamese_output(summary: "SummaryResponse", chunk_index: int) -> None:
    """Log a warning if the LLM returned English output instead of Vietnamese."""
    sample = ""
    if summary.SessionSummary.blocks:
        sample = summary.SessionSummary.blocks[0].content
    elif summary.MeetingNotes.sections:
        first_section = summary.MeetingNotes.sections[0]
        if first_section.blocks:
            sample = first_section.blocks[0].content
    if sample and _is_likely_english(sample):
        logger.warning(
            f"Chunk {chunk_index}: LLM có thể trả về tiếng Anh thay vì tiếng Việt. "
            f"Nên dùng model lớn hơn (vd: llama3.1:8b, qwen2.5:7b)."
        )

class Block(BaseModel):
    """Represents a block of content in a section.
    
    Block types must align with frontend rendering capabilities:
    - 'text': Plain text content
    - 'bullet': Bulleted list item
    - 'heading1': Large section heading
    - 'heading2': Medium section heading
    
    Colors currently supported:
    - 'gray': Gray text color
    - '' or any other value: Default text color
    """
    id: str
    type: Literal['bullet', 'heading1', 'heading2', 'text']
    content: str
    color: str  # Frontend currently only uses 'gray' or default

class Section(BaseModel):
    """Represents a section in the meeting summary"""
    title: str
    blocks: List[Block]

class MeetingNotes(BaseModel):
    """Represents the meeting notes"""
    meeting_name: str
    sections: List[Section]

class People(BaseModel):
    """Represents the people in the meeting. Always have this part in the output. Title - Person Name (Role, Details)"""
    title: str
    blocks: List[Block]

class SummaryResponse(BaseModel):
    """Represents the meeting summary response based on a section of the transcript"""
    MeetingName : str
    People : People
    SessionSummary : Section
    CriticalDeadlines: Section
    KeyItemsDecisions: Section
    ImmediateActionItems: Section
    NextSteps: Section
    MeetingNotes: MeetingNotes

# --- Main Class Used by main.py ---

class TranscriptProcessor:
    """Handles the processing of meeting transcripts using AI models."""
    def __init__(self):
        """Initialize the transcript processor."""
        logger.info("TranscriptProcessor initialized.")
        self.db = DatabaseManager()
        self.active_clients = []  # Track active Ollama client sessions
    async def _process_single_chunk(
        self,
        i: int,
        chunk: str,
        num_chunks: int,
        agent,
        model: str,
        model_name: str,
        custom_prompt: str,
    ):
        """Process one chunk with retry + timeout. Returns JSON string or None."""
        chunk_tokens = estimate_tokens(chunk)
        logger.info(f"Processing chunk {i+1}/{num_chunks} | estimated_tokens={chunk_tokens} | stage=chunk_start")

        PROMPT_TEMPLATE = f"""Dưới đây là một đoạn biên bản cuộc họp. Hãy trích xuất thông tin theo cấu trúc JSON yêu cầu. Nếu một phần (ví dụ: Thời hạn quan trọng) không có thông tin liên quan trong đoạn này, hãy trả về danh sách trống cho 'blocks'. Chỉ xuất dữ liệu JSON.

HƯỚNG DẪN TRÍCH XUẤT TỪNG PHẦN:
- People (Người tham dự): Liệt kê tất cả tên người được đề cập kèm chức vụ/vai trò nếu có. Mỗi người là một block kiểu 'bullet'.
- SessionSummary (Tóm tắt phiên họp): Tóm tắt 3-5 câu về nội dung chính của cuộc họp. Dùng block kiểu 'text'.
- CriticalDeadlines (Thời hạn quan trọng): CHỈ điền nếu có ngày/thời hạn cụ thể được đề cập; nếu không có thì để trống.
- KeyItemsDecisions (Quyết định chính): Các quyết định đã được thống nhất trong cuộc họp. Mỗi quyết định là một block kiểu 'bullet'.
- ImmediateActionItems (Việc cần làm ngay): Công việc được giao, có tên người chịu trách nhiệm nếu được đề cập. Mỗi đầu việc là một block kiểu 'bullet'.
- NextSteps (Bước tiếp theo): Kế hoạch tương lai, lịch họp tiếp theo. Mỗi bước là một block kiểu 'bullet'.
- MeetingNotes: Chỉ điền 'meeting_name' (tên cuộc họp), để 'sections' là mảng rỗng [].

QUAN TRỌNG - Loại block hợp lệ: 'text', 'bullet', 'heading1', 'heading2'
- 'text': đoạn văn thông thường
- 'bullet': mục danh sách
- 'heading1': tiêu đề chính
- 'heading2': tiêu đề phụ

QUAN TRỌNG - Trường 'id' của mỗi block PHẢI theo định dạng: "{{tên_section}}_{{số_thứ_tự}}"
Ví dụ: "session_summary_0", "action_items_1", "next_steps_0"

Trường 'color': dùng 'gray' cho nội dung ít quan trọng, '' (chuỗi rỗng) cho mặc định.

YÊU CẦU NGÔN NGỮ: Toàn bộ nội dung văn bản trong JSON (tiêu đề, mô tả, tóm tắt, việc cần làm, v.v.) BẮT BUỘC phải bằng tiếng Việt.

Đoạn biên bản:
---
{chunk}
---

Hãy nắm bắt tất cả các đầu việc liên quan. Biên bản có thể có lỗi chính tả, hãy sửa nếu cần. Bối cảnh rất quan trọng.

Thêm ngữ cảnh sau khi tạo tóm tắt:
---
{custom_prompt}
---

Chỉ xuất dữ liệu JSON. Toàn bộ nội dung phải bằng tiếng Việt."""

        # Ollama là local model, không có giới hạn API cost nên cho timeout dài hơn
        TIMEOUT = 300.0 if model == "ollama" else 120.0

        retry_count = 0
        while retry_count <= MAX_RETRIES:
            try:
                if model != "ollama":
                    summary_result = await asyncio.wait_for(
                        agent.run(PROMPT_TEMPLATE),
                        timeout=TIMEOUT,
                    )
                else:
                    logger.info(f"Ollama chunk {i+1}/{num_chunks} | chunk_size={len(chunk)} | estimated_tokens={chunk_tokens}")
                    response = await asyncio.wait_for(
                        self.chat_ollama_model(model_name, chunk, custom_prompt, full_prompt=PROMPT_TEMPLATE),
                        timeout=TIMEOUT,
                    )
                    if isinstance(response, SummaryResponse):
                        summary_result = response
                    else:
                        summary_result = SummaryResponse.model_validate_json(response)

                # Parse result
                if hasattr(summary_result, 'data') and isinstance(summary_result.data, SummaryResponse):
                    final_summary_pydantic = summary_result.data
                elif isinstance(summary_result, SummaryResponse):
                    final_summary_pydantic = summary_result
                else:
                    logger.error(f"Unexpected result type for chunk {i+1}: {type(summary_result)} | stage=parse_error")
                    return None

                _validate_vietnamese_output(final_summary_pydantic, i + 1)
                chunk_summary_json = final_summary_pydantic.model_dump_json()
                logger.info(f"Chunk {i+1}/{num_chunks} done | retry_count={retry_count} | stage=chunk_success")
                return chunk_summary_json

            except asyncio.TimeoutError:
                logger.warning(f"Chunk {i+1}/{num_chunks} timed out after {TIMEOUT:.0f}s | retry_count={retry_count} | stage=timeout")
                return None

            except asyncio.CancelledError:
                raise

            except Exception as chunk_error:
                if is_rate_limit_error(chunk_error) and retry_count < MAX_RETRIES:
                    retry_count += 1
                    backoff = INITIAL_BACKOFF_SECONDS * (2 ** (retry_count - 1))
                    logger.warning(
                        f"Rate/token limit hit on chunk {i+1} | retry_count={retry_count}/{MAX_RETRIES} "
                        f"| backoff={backoff}s | error={chunk_error} | stage=rate_limit_retry"
                    )
                    await asyncio.sleep(backoff)
                    if retry_count == MAX_RETRIES:
                        reduced = int(len(chunk) * CHUNK_REDUCTION_FACTOR)
                        chunk = chunk[:reduced]
                        PROMPT_TEMPLATE = PROMPT_TEMPLATE  # chunk updated, prompt template rebuilt on next iter
                        logger.warning(f"Reducing chunk {i+1} to {reduced} chars (~{estimate_tokens(chunk)} tokens) | stage=chunk_size_reduced")
                else:
                    if is_rate_limit_error(chunk_error):
                        logger.error(
                            f"Chunk {i+1}/{num_chunks} failed after {MAX_RETRIES} retries due to rate/token limit. "
                            f"estimated_tokens={chunk_tokens} | provider={model}/{model_name} | stage=rate_limit_exhausted | error={chunk_error}"
                        )
                    else:
                        logger.error(f"Chunk {i+1}/{num_chunks} failed | retry_count={retry_count} | stage=chunk_error | error={chunk_error}", exc_info=True)
                    return None

        logger.warning(f"Chunk {i+1}/{num_chunks} skipped (no result produced) | stage=chunk_skipped")
        return None

    async def process_transcript(self, text: str, model: str, model_name: str, chunk_size: int = 5000, overlap: int = 1000, custom_prompt: str = "") -> Tuple[int, List[str]]:
        """
        Process transcript text into chunks and generate structured summaries for each chunk using an AI model.

        Args:
            text: The transcript text.
            model: The AI model provider ('claude', 'ollama', 'groq', 'openai').
            model_name: The specific model name.
            chunk_size: The size of each text chunk (in characters).
            overlap: The overlap between consecutive chunks (in characters).
            custom_prompt: A custom prompt to use for the AI model.

        Returns:
            A tuple containing:
            - The number of chunks processed.
            - A list of JSON strings, where each string is the summary of a chunk.
        """
        estimated_tokens_total = estimate_tokens(text)
        logger.info(
            f"Processing transcript | length={len(text)} chars | estimated_tokens={estimated_tokens_total} "
            f"| provider={model} | model_name={model_name} | chunk_size={chunk_size} | overlap={overlap} | stage=init"
        )

        all_json_data = []
        agent = None
        llm = None

        try:
            # ── Bước 1 & 2: Pre-check chunk size against provider safe limits ──────
            # Compute safe chunk chars for this provider/model (with ~35% buffer).
            safe_chunk_chars = get_safe_chunk_chars(model, model_name)

            # For Ollama: override with model-family aware sizes (local, no TPM)
            if model == "ollama":
                model_lower = model_name.lower()
                if model_lower.startswith("phi4") or model_lower.startswith("llama"):
                    safe_chunk_chars = min(safe_chunk_chars, 10_000)
                else:
                    safe_chunk_chars = min(safe_chunk_chars, 30_000)

            # If requested chunk_size exceeds safe limit → reduce and warn
            if chunk_size > safe_chunk_chars:
                logger.warning(
                    f"Requested chunk_size={chunk_size} chars exceeds safe limit={safe_chunk_chars} chars "
                    f"for provider={model}/{model_name}. Reducing chunk_size to avoid token/TPM overflow. "
                    f"estimated_tokens_per_chunk_before={chunk_size // 4} | stage=pre_check"
                )
                chunk_size = safe_chunk_chars
                # Keep overlap proportional, max 20% of chunk_size
                overlap = min(overlap, chunk_size // 5)

            # ── Select and initialize the AI model ───────────────────────────────
            if model == "claude":
                api_key = await db.get_api_key("claude")
                if not api_key: raise ValueError("ANTHROPIC_API_KEY environment variable not set")
                llm = AnthropicModel(model_name, provider=AnthropicProvider(api_key=api_key))
                logger.info(f"Using Claude model: {model_name}")
            elif model == "ollama":
                ollama_host = os.getenv('OLLAMA_HOST', 'http://localhost:11434')
                ollama_base_url = f"{ollama_host}/v1"
                llm = OpenAIModel(model_name=model_name, provider=OpenAIProvider(base_url=ollama_base_url))
                logger.info(f"Using Ollama model: {model_name}")
            elif model == "groq":
                api_key = await db.get_api_key("groq")
                if not api_key: raise ValueError("GROQ_API_KEY environment variable not set")
                llm = GroqModel(model_name, provider=GroqProvider(api_key=api_key))
                logger.info(f"Using Groq model: {model_name}")
            elif model == "openai":
                api_key = await db.get_api_key("openai")
                if not api_key: raise ValueError("OPENAI_API_KEY environment variable not set")
                llm = OpenAIModel(model_name, provider=OpenAIProvider(api_key=api_key))
                logger.info(f"Using OpenAI model: {model_name}")
            else:
                logger.error(f"Unsupported model provider requested: {model}")
                raise ValueError(f"Unsupported model provider: {model}")

            # Initialize the pydantic-ai agent
            agent = Agent(llm, result_type=SummaryResponse, result_retries=0)
            logger.info("Pydantic-AI Agent initialized.")

            # ── Bước 2 & 3: Split transcript into safe-sized chunks ───────────────
            step = chunk_size - overlap
            if step <= 0:
                logger.warning(f"Overlap ({overlap}) >= chunk_size ({chunk_size}). Adjusting overlap.")
                overlap = max(0, chunk_size - 100)
                step = chunk_size - overlap

            chunks = [text[i:i + chunk_size] for i in range(0, len(text), step)]
            num_chunks = len(chunks)
            logger.info(
                f"Transcript split | chunk_count={num_chunks} | chunk_size={chunk_size} | overlap={overlap} "
                f"| estimated_tokens_per_chunk≈{chunk_size // 4} | stage=chunked"
            )

            # ── Bước 4 & 5: Process each chunk with retry on rate-limit ──────────
            if model == "ollama":
                # Ollama chạy tuần tự (local GPU, tránh tranh chấp tài nguyên)
                for i, chunk in enumerate(chunks):
                    result = await self._process_single_chunk(i, chunk, num_chunks, agent, model, model_name, custom_prompt)
                    if result is not None:
                        all_json_data.append(result)
            else:
                # Cloud providers: xử lý song song tối đa 3 chunk cùng lúc
                semaphore = asyncio.Semaphore(1)

                async def _bounded(i: int, chunk: str):
                    async with semaphore:
                        return await self._process_single_chunk(i, chunk, num_chunks, agent, model, model_name, custom_prompt)

                raw_results = await asyncio.gather(
                    *[_bounded(i, chunk) for i, chunk in enumerate(chunks)],
                    return_exceptions=True,
                )
                for i, result in enumerate(raw_results):
                    if isinstance(result, Exception):
                        logger.error(f"Chunk {i+1} raised exception during parallel processing: {result}", exc_info=True)
                    elif result is not None:
                        all_json_data.append(result)

            logger.info(f"Finished processing all {num_chunks} chunks.")
            return num_chunks, all_json_data

        except Exception as e:
            logger.error(f"Error during transcript processing: {str(e)}", exc_info=True)
            raise
    
    async def chat_ollama_model(self, model_name: str, transcript: str, custom_prompt: str, full_prompt: str = ""):
        # Dùng full_prompt (từ _process_single_chunk) nếu có, ngược lại build prompt cơ bản
        if full_prompt:
            content = full_prompt
        else:
            content = f'''Dưới đây là một đoạn biên bản cuộc họp. Hãy trích xuất thông tin theo cấu trúc JSON yêu cầu. Nếu một phần không có thông tin liên quan, hãy trả về danh sách trống cho 'blocks'. Chỉ xuất dữ liệu JSON. Toàn bộ nội dung phải bằng tiếng Việt.

Đoạn biên bản:
---
{transcript}
---

Ngữ cảnh bổ sung:
---
{custom_prompt}
---

Chỉ xuất dữ liệu JSON. Toàn bộ nội dung phải bằng tiếng Việt.'''

        message = {
            'role': 'user',
            'content': content,
        }

        # Create a client and track it for cleanup
        ollama_host = os.getenv('OLLAMA_HOST', 'http://127.0.0.1:11434')
        client = AsyncClient(host=ollama_host)
        self.active_clients.append(client)
        
        try:
            response = await client.chat(model=model_name, messages=[message], stream=True, format=SummaryResponse.model_json_schema(), options={"num_predict": 2048})
            
            full_response = ""
            async for part in response:
                content = part['message']['content']
                print(content, end='', flush=True)
                full_response += content
            
            try:
                summary = SummaryResponse.model_validate_json(full_response)
                print("\n", summary.model_dump_json(indent=2), type(summary))
                return summary
            except Exception as e:
                print(f"\nError parsing response: {e}")
                return full_response
        except asyncio.CancelledError:
            logger.info("Ollama request was cancelled during shutdown")
            raise
        except Exception as e:
            logger.error(f"Error in Ollama chat: {e}")
            raise
        finally:
            # Remove the client from active clients list
            if client in self.active_clients:
                self.active_clients.remove(client)

    def cleanup(self):
        """Clean up resources used by the TranscriptProcessor."""
        logger.info("Cleaning up TranscriptProcessor resources")
        try:
            # Close database connections if any
            if hasattr(self, 'db') and self.db is not None:
                # self.db.close()
                logger.info("Database connection cleanup (using context managers)")
                
            # Cancel any active Ollama client sessions
            if hasattr(self, 'active_clients') and self.active_clients:
                logger.info(f"Terminating {len(self.active_clients)} active Ollama client sessions")
                for client in self.active_clients:
                    try:
                        # Close the client's underlying connection
                        if hasattr(client, '_client') and hasattr(client._client, 'close'):
                            asyncio.create_task(client._client.aclose())
                    except Exception as client_error:
                        logger.error(f"Error closing Ollama client: {client_error}", exc_info=True)
                # Clear the list
                self.active_clients.clear()
                logger.info("All Ollama client sessions terminated")
        except Exception as e:
            logger.error(f"Error during TranscriptProcessor cleanup: {str(e)}", exc_info=True)

        