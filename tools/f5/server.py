"""Голос по образцу для Yuki: F5-TTS с русской моделью.

По одному отрывку F5 переносит не только тембр, но и манеру речи — на слух
владельца это оказалось ближе к персонажу, чем XTTS и чем RVC, который меняет
лишь тембр поверх чужой интонации.

Протокол тот же, что у остальных сервисов Yuki: ``POST /tts`` с
``{text, ref_audio_path}`` возвращает WAV. Образцу нужен текст — он берётся из
файла рядом (``mita.wav`` → ``mita.txt``). Образец не длиннее 12 секунд.

Запуск::

    D:\\yuki-voice\\f5\\.venv\\Scripts\\python.exe tools/f5/server.py --port 9880 \\
        --models D:\\yuki-voice\\f5\\models --ref C:\\...\\voices\\mita.wav

Модель: Misha24-10/F5-TTS_RUSSIAN, лицензия CC BY-NC 4.0 — для себя можно.
"""

from __future__ import annotations

import argparse
import io
import json
import os
import re
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import soundfile as sf
import torch
import torchaudio


def _load(path, *args, **kwargs):
    """torchaudio.load без FFmpeg: torchcodec на Windows требует его библиотек."""
    data, rate = sf.read(str(path), dtype="float32", always_2d=True)
    return torch.from_numpy(data.T.copy()), rate


torchaudio.load = _load

_lock = threading.Lock()
_state: dict = {}


def setup(models: str, ref: str, steps: int):
    from f5_tts.api import F5TTS

    _state["tts"] = F5TTS(
        model="F5TTS_v1_Base",
        ckpt_file=os.path.join(models, "model_v2.safetensors"),
        vocab_file=os.path.join(models, "vocab.txt"),
        device="cuda" if torch.cuda.is_available() else "cpu",
    )
    _state["ref"] = ref
    _state["steps"] = steps


def transcript(ref: str) -> str:
    """Текст образца лежит рядом с ним: F5 сверяет звук со словами."""
    sidecar = os.path.splitext(ref)[0] + ".txt"
    if not os.path.isfile(sidecar):
        raise ValueError(f"нет текста образца: {sidecar}")
    return open(sidecar, encoding="utf-8").read().strip()


def numbers_to_words(text: str) -> str:
    """Цифры словами: модель учили на словах, «20:40» она прочтёт как попало."""
    try:
        from num2words import num2words
    except ImportError:
        return text
    return re.sub(r"\d+", lambda m: num2words(int(m.group()), lang="ru"), text)


def synthesize(text: str, ref: str | None) -> bytes:
    # Образец из запроса — если у него есть текст; иначе тот, с которым запущен сервис.
    if not ref or not os.path.isfile(os.path.splitext(ref)[0] + ".txt"):
        ref = _state["ref"]
    clean = numbers_to_words(text).strip()[:600] or "Готово."
    with _lock:
        wav, rate, _ = _state["tts"].infer(
            ref, transcript(ref), clean, nfe_step=_state["steps"], seed=42, show_info=lambda *a: None,
        )
    buffer = io.BytesIO()
    sf.write(buffer, wav, rate, format="WAV", subtype="PCM_16")
    return buffer.getvalue()


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):  # noqa: N802 — имя задаёт http.server
        if self.path.rstrip("/") != "/tts":
            self.send_error(404)
            return
        try:
            length = int(self.headers.get("Content-Length", 0))
            body = json.loads(self.rfile.read(length) or b"{}")
            wav = synthesize(str(body.get("text", "")), body.get("ref_audio_path"))
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
    parser.add_argument("--models", default=r"D:\yuki-voice\f5\models")
    parser.add_argument("--ref", default=r"C:\Users\alex\Documents\Yuki\voices\mita.wav")
    parser.add_argument("--steps", type=int, default=16, help="шаги генерации: меньше — быстрее")
    args = parser.parse_args()
    setup(args.models, args.ref, args.steps)
    synthesize("Привет.", None)  # прогрев: первая настоящая фраза не должна ждать
    # Только петлевой адрес: голос не должен быть доступен соседям по сети.
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"F5 готов: http://127.0.0.1:{args.port}/tts", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
