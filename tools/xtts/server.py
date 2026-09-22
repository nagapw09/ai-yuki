"""Голос по образцу для Yuki: XTTS-v2.

Отвечает по протоколу, который Yuki уже понимает (взят у GPT-SoVITS):
``POST /tts`` с ``{text, ref_audio_path, text_lang, ...}`` возвращает WAV.

Голос берётся из образца ``ref_audio_path`` — 6–15 секунд чистой речи. Его
«отпечаток» считается один раз и кэшируется: повторные фразы тем же голосом
не пересчитывают образец. GPT-SoVITS здесь не подошёл — русского он не знает.

Запуск::

    D:\\yuki-voice\\xtts\\.venv\\Scripts\\python.exe tools/xtts/server.py --port 9880

Лицензия модели — Coqui Public Model License: для себя можно, продавать нельзя.
"""

from __future__ import annotations

import argparse
import io
import json
import os
import re
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

os.environ.setdefault("COQUI_TOS_AGREED", "1")

import numpy as np  # noqa: E402
import soundfile as sf  # noqa: E402
import torch  # noqa: E402

SAMPLE_RATE = 24000
_lock = threading.Lock()
_model = None
_latents: dict[tuple[str, float], tuple] = {}


def _load_audio(path, sampling_rate):
    """Чтение образца без FFmpeg.

    torchaudio с torch 2.9+ читает звук через torchcodec, а тому на Windows
    нужны библиотеки FFmpeg. Ставить FFmpeg ради чтения одного WAV незачем:
    soundfile читает его сам, а пересчёт частоты у torchaudio работает и так.
    """
    import torchaudio

    data, rate = sf.read(path, dtype="float32", always_2d=True)
    audio = torch.from_numpy(data.T.copy())
    if audio.size(0) != 1:
        audio = audio.mean(dim=0, keepdim=True)
    if rate != sampling_rate:
        audio = torchaudio.functional.resample(audio, rate, sampling_rate)
    return audio.clip_(-1, 1)


def model():
    """Загружает XTTS один раз; первая загрузка скачивает ~1,9 ГБ."""
    global _model
    if _model is None:
        import TTS.tts.models.xtts as xtts_module
        from TTS.api import TTS

        xtts_module.load_audio = _load_audio

        device = "cuda" if torch.cuda.is_available() else "cpu"
        _model = TTS("tts_models/multilingual/multi-dataset/xtts_v2").to(device)
    return _model


def latents(reference: str):
    """Отпечаток голоса из образца; пересчитывается, только если файл изменили."""
    key = (reference, os.path.getmtime(reference))
    if key not in _latents:
        xtts = model().synthesizer.tts_model
        _latents[key] = xtts.get_conditioning_latents(audio_path=[reference])
    return _latents[key]


def numbers_to_words(text: str, lang: str) -> str:
    """Цифры словами: модель читает «20:40» непредсказуемо."""
    try:
        from num2words import num2words
    except ImportError:
        return text
    return re.sub(r"\d+", lambda m: num2words(int(m.group()), lang=lang), text)


def synthesize(text: str, reference: str, lang: str) -> bytes:
    if not reference or not os.path.isfile(reference):
        raise ValueError(f"нет образца голоса: {reference!r}")
    clean = numbers_to_words(text, lang).strip()[:600] or "Готово."
    with _lock:
        gpt_latent, speaker_embedding = latents(reference)
        out = model().synthesizer.tts_model.inference(
            clean, lang, gpt_latent, speaker_embedding, temperature=0.7, enable_text_splitting=True
        )
    audio = np.asarray(out["wav"], dtype=np.float32)
    buffer = io.BytesIO()
    sf.write(buffer, audio, SAMPLE_RATE, format="WAV", subtype="PCM_16")
    return buffer.getvalue()


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):  # noqa: N802 — имя задаёт http.server
        if self.path.rstrip("/") != "/tts":
            self.send_error(404)
            return
        try:
            length = int(self.headers.get("Content-Length", 0))
            body = json.loads(self.rfile.read(length) or b"{}")
            wav = synthesize(
                str(body.get("text", "")),
                str(body.get("ref_audio_path", "")),
                str(body.get("text_lang") or "ru"),
            )
        except Exception as error:  # одна плохая фраза не должна ронять сервис
            message = str(error).encode("utf-8")
            self.send_response(500)
            self.send_header("Content-Length", str(len(message)))
            self.end_headers()
            self.wfile.write(message)
            return
        self.send_response(200)
        self.send_header("Content-Type", "audio/wav")
        self.send_header("Content-Length", str(len(wav)))
        self.end_headers()
        self.wfile.write(wav)

    def log_message(self, *_):
        pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=9880)
    args = parser.parse_args()
    model()
    # Только петлевой адрес: голос не должен быть доступен соседям по сети.
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"XTTS готов: http://127.0.0.1:{args.port}/tts", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
