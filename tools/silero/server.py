"""Локальный синтез речи Silero для Yuki.

Отвечает по протоколу, который Yuki уже понимает (взят у GPT-SoVITS):
``POST /tts`` с ``{text, ref_audio_path, ...}`` возвращает WAV целиком.

Голос выбирается по имени файла образца: ``xenia.wav`` → голос ``xenia``,
``silero_baya.wav`` → ``baya``. Сам звук образца Silero не нужен — у модели
голоса встроены, — поэтому в папке голосов достаточно пустых файлов с нужными
именами. Так в Yuki ничего не приходится менять: выбор голоса в настройках
остаётся выбором файла.

Запуск (из окружения с torch, например ``D:\\github\\silero-ru\\.venv``)::

    python tools/silero/server.py --port 9880

Лицензия моделей Silero — CC BY-NC-SA: для себя можно, продавать нельзя.
"""

from __future__ import annotations

import argparse
import io
import json
import re
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import soundfile as sf
import torch

SPEAKERS = ("xenia", "baya", "kseniya", "aidar", "eugene")
SAMPLE_RATE = 48000
_model = None
_lock = threading.Lock()


def model():
    """Загружает модель один раз: первая фраза ждёт секунды, остальные — нет."""
    global _model
    if _model is None:
        _model, _ = torch.hub.load(
            "snakers4/silero-models", "silero_tts", language="ru", speaker="v4_ru", trust_repo=True
        )
        # Видеокарта, если есть: фраза за доли секунды вместо двух. Модель весит
        # десятки мегабайт и видеопамяти почти не занимает.
        if torch.cuda.is_available():
            _model.to(torch.device("cuda"))
        else:
            # Фоновому помощнику хватит пары потоков: он не должен занимать весь процессор.
            torch.set_num_threads(2)
    return _model


def speaker_of(reference: str) -> str:
    """Голос из имени файла образца; незнакомое имя — голос по умолчанию."""
    stem = Path(reference).stem.lower().removeprefix("silero_")
    return stem if stem in SPEAKERS else "xenia"


def numbers_to_words(text: str) -> str:
    """Silero не читает цифры: «20:40» он пропустил бы молча."""
    try:
        from num2words import num2words
    except ImportError:
        return text
    return re.sub(r"\d+", lambda m: num2words(int(m.group()), lang="ru"), text)


def synthesize(text: str, speaker: str) -> bytes:
    clean = numbers_to_words(text).strip()[:900] or "Готово."
    with _lock:
        audio = model().apply_tts(text=clean, speaker=speaker, sample_rate=SAMPLE_RATE)
    buffer = io.BytesIO()
    sf.write(buffer, audio.numpy(), SAMPLE_RATE, format="WAV", subtype="PCM_16")
    return buffer.getvalue()


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):  # noqa: N802 — имя задаёт http.server
        if self.path.rstrip("/") != "/tts":
            self.send_error(404)
            return
        try:
            length = int(self.headers.get("Content-Length", 0))
            body = json.loads(self.rfile.read(length) or b"{}")
            wav = synthesize(str(body.get("text", "")), speaker_of(str(body.get("ref_audio_path", ""))))
        except Exception as error:  # сервис не должен падать от одной плохой фразы
            self.send_response(500)
            self.end_headers()
            self.wfile.write(str(error).encode("utf-8"))
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
    print(f"Silero готов: http://127.0.0.1:{args.port}/tts", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
