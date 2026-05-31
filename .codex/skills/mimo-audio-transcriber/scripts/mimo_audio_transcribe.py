#!/usr/bin/env python
from __future__ import annotations

import argparse
import base64
import datetime as dt
import json
import mimetypes
import os
import sys
import tempfile
from pathlib import Path
from typing import Any
from urllib import error, request


DEFAULT_BASE_URL = "https://token-plan-cn.xiaomimimo.com/v1"
DEFAULT_MODEL = "mimo-v2.5"
DEFAULT_MAX_COMPLETION_TOKENS = 128000
DEFAULT_TIMEOUT_SECONDS = 1200
SUPPORTED_DIRECT_EXTENSIONS = {".mp3", ".wav", ".flac", ".m4a", ".ogg"}
AUDIO_EXTENSIONS = SUPPORTED_DIRECT_EXTENSIONS | {".aac", ".acc"}
MIME_BY_EXTENSION = {
    ".mp3": "audio/mpeg",
    ".wav": "audio/wav",
    ".flac": "audio/flac",
    ".m4a": "audio/mp4",
    ".ogg": "audio/ogg",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Transcribe local audio through MiMo v2.5 audio understanding."
    )
    parser.add_argument(
        "audio",
        nargs="?",
        help="Audio file path. If omitted, use the newest audio in the project temporary recording inbox.",
    )
    parser.add_argument("--api-key", help="MiMo API key. Prefer --api-key, MIMO_API_KEY, or .secrets/mimo_api_key.txt.")
    parser.add_argument("--base-url", default=DEFAULT_BASE_URL)
    parser.add_argument("--model", default=DEFAULT_MODEL)
    parser.add_argument("--output-dir", help="Directory for transcript, JSON response, usage, and metadata.")
    parser.add_argument("--max-completion-tokens", type=int, default=DEFAULT_MAX_COMPLETION_TOKENS)
    parser.add_argument("--temperature", type=float, default=1.0)
    parser.add_argument("--top-p", type=float, default=0.95)
    parser.add_argument("--timeout", type=int, default=DEFAULT_TIMEOUT_SECONDS)
    parser.add_argument("--keep-temp", action="store_true", help="Keep converted temporary MP3 files.")
    parser.add_argument("--dry-run", action="store_true", help="Prepare inputs and outputs without calling MiMo.")
    parser.add_argument(
        "--prompt",
        help="Override the default transcript prompt. Use with care; the default prompt is tuned for this project.",
    )
    return parser.parse_args()


def find_project_root() -> Path:
    here = Path(__file__).resolve()
    for parent in here.parents:
        if (parent / "AGENTS.md").exists() and (parent / ".codex").exists():
            return parent
    return here.parents[4]


def default_inbox(project_root: Path) -> Path:
    return project_root / "000-收集箱" / "临时录音转文字"


def find_newest_audio(inbox: Path) -> Path:
    inbox.mkdir(parents=True, exist_ok=True)
    candidates = [
        path
        for path in inbox.iterdir()
        if path.is_file() and path.suffix.lower() in AUDIO_EXTENSIONS
    ]
    if not candidates:
        raise FileNotFoundError(
            f"No audio files found in default inbox: {inbox}. "
            "Pass an audio path or place an audio file there."
        )
    return max(candidates, key=lambda path: path.stat().st_mtime)


def resolve_api_key(args: argparse.Namespace, project_root: Path) -> str:
    key = args.api_key or os.environ.get("MIMO_API_KEY") or read_api_key_from_secret_file(project_root)
    if not key:
        raise RuntimeError(
            "No MiMo API key found. Set MIMO_API_KEY, pass --api-key, or create .secrets/mimo_api_key.txt."
        )
    return key


def read_api_key_from_secret_file(project_root: Path) -> str | None:
    secret_file = project_root / ".secrets" / "mimo_api_key.txt"
    if not secret_file.exists():
        return None
    for line in secret_file.read_text(encoding="utf-8", errors="replace").splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if stripped.startswith("MIMO_API_KEY="):
            return stripped.split("=", 1)[1].strip() or None
        return stripped
    return None


def media_duration_seconds(path: Path) -> float | None:
    try:
        import av
    except ImportError:
        return None

    container = av.open(str(path))
    try:
        if container.duration is None:
            return None
        return float(container.duration / av.time_base)
    finally:
        container.close()


def convert_to_mp3(input_path: Path, temp_dir: Path) -> Path:
    try:
        import av
    except ImportError as exc:
        raise RuntimeError(
            "PyAV is required to convert unsupported audio formats. "
            "Install av or provide MP3/WAV/FLAC/M4A/OGG input."
        ) from exc

    output_path = temp_dir / f"{input_path.stem}_converted.mp3"
    input_container = av.open(str(input_path))
    try:
        input_stream = input_container.streams.audio[0]
        output_container = av.open(str(output_path), mode="w")
        try:
            output_stream = output_container.add_stream("mp3", rate=input_stream.rate)
            output_stream.layout = input_stream.layout
            for frame in input_container.decode(input_stream):
                for packet in output_stream.encode(frame):
                    output_container.mux(packet)
            for packet in output_stream.encode():
                output_container.mux(packet)
        finally:
            output_container.close()
    finally:
        input_container.close()
    return output_path


def prepare_audio(input_path: Path, temp_dir: Path | None) -> tuple[Path, str, bool]:
    suffix = input_path.suffix.lower()
    if suffix in SUPPORTED_DIRECT_EXTENSIONS:
        mime = MIME_BY_EXTENSION.get(suffix) or mimetypes.guess_type(input_path.name)[0] or "audio/mpeg"
        return input_path, mime, False
    if temp_dir is None:
        raise RuntimeError("Internal error: temp_dir is required for conversion.")
    converted = convert_to_mp3(input_path, temp_dir)
    return converted, "audio/mpeg", True


def make_output_paths(audio_path: Path, output_dir: Path | None) -> dict[str, Path]:
    output_root = output_dir if output_dir else audio_path.parent
    output_root.mkdir(parents=True, exist_ok=True)
    timestamp = dt.datetime.now().strftime("%Y%m%d_%H%M%S")
    prefix = f"{audio_path.stem}_mimo_{timestamp}"
    return {
        "transcript": output_root / f"{prefix}_transcript.md",
        "response": output_root / f"{prefix}_response.json",
        "usage": output_root / f"{prefix}_usage.json",
        "meta": output_root / f"{prefix}_meta.json",
    }


def default_prompt() -> str:
    return "\n".join(
        [
            "请对整段音频做完整逐字稿。要求：",
            "1. 只输出最终结果，不输出推理过程。",
            "2. 用 Markdown 表格输出，列为：开始时间、说话人、逐字稿、不确定标注。",
            "3. 开始时间格式用 mm:ss 或 hh:mm:ss，尽量给到每句话或每个自然语句的开始时间。",
            "4. 说话人用 A/B/C/D 标记；如果无法确认是否同一人，请保持保守并在不确定标注中说明。",
            "5. 对听不清、重叠说话、噪声遮挡、音色无法判断的地方，分别标注为 [听不清]、[重叠]、[噪声]、[说话人不确定]。",
            "6. 不要编造没听清的内容；无法确认的词用 [不确定: 可能是...] 或 [听不清]。",
            "7. 请尽量完整覆盖整段录音，不要只摘要。",
            "8. 表格后面补充一个简短的说话人估计：估计人数、各说话人大致声音特征、这些判断的置信度。",
        ]
    )


def build_payload(args: argparse.Namespace, audio_path: Path, mime_type: str, prompt: str) -> dict[str, Any]:
    audio_b64 = base64.b64encode(audio_path.read_bytes()).decode("ascii")
    return {
        "model": args.model,
        "messages": [
            {
                "role": "system",
                "content": (
                    "你是MiMo（中文名称也是MiMo），是小米公司研发的AI智能助手。"
                    "今天的日期：2026年5月29日，星期五。你的知识截止日期是 2024 年 12 月。"
                ),
            },
            {
                "role": "user",
                "content": [
                    {
                        "type": "input_audio",
                        "input_audio": {
                            "data": f"data:{mime_type};base64,{audio_b64}",
                        },
                    },
                    {
                        "type": "text",
                        "text": prompt,
                    },
                ],
            },
        ],
        "max_completion_tokens": args.max_completion_tokens,
        "temperature": args.temperature,
        "top_p": args.top_p,
        "stream": False,
        "thinking": {
            "type": "disabled",
        },
    }


def call_mimo(args: argparse.Namespace, api_key: str, payload: dict[str, Any]) -> tuple[int, dict[str, Any]]:
    body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
    req = request.Request(
        f"{args.base_url.rstrip('/')}/chat/completions",
        data=body,
        method="POST",
        headers={
            "api-key": api_key,
            "Content-Type": "application/json",
        },
    )

    try:
        with request.urlopen(req, timeout=args.timeout) as resp:
            raw = resp.read().decode("utf-8", errors="replace")
            return resp.status, json.loads(raw)
    except error.HTTPError as exc:
        raw = exc.read().decode("utf-8", errors="replace")
        try:
            parsed = json.loads(raw)
        except json.JSONDecodeError:
            parsed = {"raw": raw}
        return exc.code, parsed


def extract_message(body: dict[str, Any]) -> tuple[str, str, dict[str, Any], str | None, str | None]:
    choice = body.get("choices", [{}])[0]
    message = choice.get("message", {})
    return (
        message.get("content") or "",
        message.get("reasoning_content") or "",
        body.get("usage", {}),
        body.get("model"),
        choice.get("finish_reason"),
    )


def write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding="utf-8")


def main() -> int:
    args = parse_args()
    project_root = find_project_root()

    if args.audio:
        input_audio = Path(args.audio).expanduser().resolve()
    else:
        input_audio = find_newest_audio(default_inbox(project_root)).resolve()

    if not input_audio.exists():
        raise FileNotFoundError(input_audio)

    output_dir = Path(args.output_dir).expanduser().resolve() if args.output_dir else None
    output_paths = make_output_paths(input_audio, output_dir)
    prompt = args.prompt or default_prompt()

    temp_ctx = tempfile.TemporaryDirectory(prefix="mimo_audio_")
    temp_dir = Path(temp_ctx.name)
    try:
        prepared_audio, mime_type, converted = prepare_audio(input_audio, temp_dir)
        duration = media_duration_seconds(prepared_audio)
        meta = {
            "input_audio": str(input_audio),
            "prepared_audio": str(prepared_audio),
            "converted_to_mp3": converted,
            "mime_type": mime_type,
            "duration_seconds": duration,
            "base_url": args.base_url,
            "model": args.model,
            "max_completion_tokens": args.max_completion_tokens,
            "output_paths": {key: str(path) for key, path in output_paths.items()},
            "created_at": dt.datetime.now().isoformat(timespec="seconds"),
            "dry_run": args.dry_run,
        }

        print(f"INPUT_AUDIO={input_audio}")
        print(f"PREPARED_AUDIO={prepared_audio}")
        print(f"MIME_TYPE={mime_type}")
        print(f"DURATION_SECONDS={duration}")
        print(f"TRANSCRIPT_MD={output_paths['transcript']}")

        if args.dry_run:
            write_json(output_paths["meta"], meta)
            print("DRY_RUN=1")
            return 0

        api_key = resolve_api_key(args, project_root)
        payload = build_payload(args, prepared_audio, mime_type, prompt)
        status, body = call_mimo(args, api_key, payload)
        meta["http_status"] = status
        write_json(output_paths["response"], body)

        print(f"HTTP_STATUS={status}")
        if status >= 400:
            write_json(output_paths["meta"], meta)
            print(f"RESPONSE_JSON={output_paths['response']}")
            print(json.dumps(body, ensure_ascii=False, indent=2)[:4000])
            return 1

        content, reasoning, usage, model, finish_reason = extract_message(body)
        output_paths["transcript"].write_text(content, encoding="utf-8")
        write_json(output_paths["usage"], usage)
        meta.update(
            {
                "model_returned": model,
                "finish_reason": finish_reason,
                "content_chars": len(content),
                "reasoning_chars": len(reasoning),
                "usage": usage,
            }
        )
        write_json(output_paths["meta"], meta)

        print(f"MODEL={model}")
        print(f"FINISH_REASON={finish_reason}")
        print("USAGE=" + json.dumps(usage, ensure_ascii=False))
        print(f"CONTENT_CHARS={len(content)}")
        print(f"REASONING_CHARS={len(reasoning)}")
        print(f"RESPONSE_JSON={output_paths['response']}")
        print(f"USAGE_JSON={output_paths['usage']}")
        print(f"META_JSON={output_paths['meta']}")
        return 0
    finally:
        if args.keep_temp:
            print(f"TEMP_DIR_KEPT={temp_dir}")
        else:
            temp_ctx.cleanup()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except error.URLError as exc:
        print(f"NETWORK_ERROR={exc}", file=sys.stderr)
        raise SystemExit(2)
