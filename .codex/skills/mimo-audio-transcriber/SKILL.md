---
name: mimo-audio-transcriber
description: Project-level workflow for turning local audio recordings into Markdown transcripts through Xiaomi MiMo v2.5 audio understanding. Use when Codex needs to transcribe MP3/WAV/FLAC/M4A/OGG/AAC recordings, convert unsupported audio formats, call the MiMo Token Plan API, save transcript/JSON/usage outputs beside the audio file, or process the TheBrain project's default temporary recording inbox.
---

# MiMo Audio Transcriber

Use the bundled script for repeatable audio-to-transcript work. Do not create one-off request scripts unless the user asks for an experiment that the script cannot cover.

## Default Workflow

1. Resolve the audio input.
   - If the user provides a path, pass it to `scripts/mimo_audio_transcribe.py`.
   - If no path is provided, run the script without an audio argument; it will use the newest supported audio file in `<project-root>/000-收集箱/临时录音转文字`.
2. Prefer whole-audio analysis by default.
   - The project preference is one request for the whole recording, not 5-minute chunking.
   - Warn the user when a long recording may produce incomplete coverage; keep the raw JSON and usage files for audit.
3. Use MiMo Token Plan China cluster by default.
   - Base URL: `https://token-plan-cn.xiaomimimo.com/v1`
   - Model: `mimo-v2.5`
   - `max_completion_tokens`: `128000`
   - `thinking`: disabled
4. Save outputs beside the audio file unless `--output-dir` is provided.
   - `<stem>_mimo_<timestamp>_transcript.md`
   - `<stem>_mimo_<timestamp>_response.json`
   - `<stem>_mimo_<timestamp>_usage.json`
   - `<stem>_mimo_<timestamp>_meta.json`
5. Keep production clean.
   - Do not leave ad hoc scripts in project folders.
   - Temporary converted MP3 files are deleted by default; use `--keep-temp` only for debugging.
   - Do not print API keys.

## Script Usage

From the TheBrain project root:

```powershell
python .\.codex\skills\mimo-audio-transcriber\scripts\mimo_audio_transcribe.py "D:\path\to\recording.aac"
```

Without an explicit audio path:

```powershell
python .\.codex\skills\mimo-audio-transcriber\scripts\mimo_audio_transcribe.py
```

Dry run without uploading:

```powershell
python .\.codex\skills\mimo-audio-transcriber\scripts\mimo_audio_transcribe.py "D:\path\to\recording.mp3" --dry-run
```

## API Key Lookup

The script looks for a MiMo key in this order:

1. `--api-key`
2. `MIMO_API_KEY`
3. `<project-root>/.secrets/mimo_api_key.txt`

The local `.secrets/` directory is gitignored. Do not echo keys in assistant responses or command output.

## Output Quality Notes

MiMo audio understanding can produce speaker-labeled transcripts and uncertainty markers, but speaker labels are model estimates, not biometric identity. For long recordings, inspect the last transcript timestamp against the media duration; if coverage is incomplete, rerun with a shorter audio segment or add chunking support after user confirmation.
