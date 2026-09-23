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

# Короткие фразы повторяются: «Секунду», «Да?», «Открываю». Синтез каждой — две
# секунды видеокарты, а отклик, пока Yuki думает, должен звучать сразу.
_cache: dict[tuple[str, str, str], bytes] = {}
CACHE_TEXT = 60
CACHE_SIZE = 128

# Отклики из apps/desktop/src/agent/ack.ts и voice.ts — готовятся при старте.
WARM = [
    "Да?", "Слушаю.", "Да-да?", "М?",
    "Секунду.", "Сейчас сделаю.", "Минутку.", "Секунду, открываю.", "Сейчас открою.",
    "Сейчас поищу.", "Минутку, смотрю.", "Сейчас запишу.", "Секунду, запоминаю.",
    "Сейчас напишу.", "Сейчас подумаю.", "Хм, секунду.",
    "Открываю.", "Открываю, секунду.", "Готово.", "Я тут!", "Пока-пока!",
]


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


def transcript(ref: str, fallback: str = "") -> str:
    """Текст образца: из файла рядом (``mita.wav`` → ``mita.txt``), иначе из настроек Yuki.

    Файл важнее: он принадлежит конкретному образцу, а поле в настройках одно
    на все и после смены образца может остаться от прежнего.
    """
    sidecar = os.path.splitext(ref)[0] + ".txt"
    if os.path.isfile(sidecar):
        return open(sidecar, encoding="utf-8").read().strip()
    if fallback.strip():
        return fallback.strip()
    raise ValueError(f"нет текста образца: положите {os.path.basename(sidecar)} рядом или впишите текст в настройках")


def numbers_to_words(text: str) -> str:
    """Цифры словами: модель учили на словах, «20:40» она прочтёт как попало."""
    try:
        from num2words import num2words
    except ImportError:
        return text
    return re.sub(r"\d+", lambda m: num2words(int(m.group()), lang="ru"), text)


def synthesize(text: str, ref: str | None, prompt: str = "") -> bytes:
    # Образец из запроса, если он есть на диске; иначе тот, с которым запущен сервис.
    if not ref or not os.path.isfile(ref):
        ref, prompt = _state["ref"], ""
    clean = numbers_to_words(text).strip()[:600] or "Готово."
    # Путь нормализуется: Yuki и прогрев могут записать один файл по-разному.
    key = (os.path.normcase(os.path.abspath(ref)), prompt, clean)
    if key in _cache:
        return _cache[key]
    with _lock:
        wav, rate, _ = _state["tts"].infer(
            ref, transcript(ref, prompt), clean, nfe_step=_state["steps"], seed=42, show_info=lambda *a: None,
        )
    buffer = io.BytesIO()
    sf.write(buffer, wav, rate, format="WAV", subtype="PCM_16")
    data = buffer.getvalue()
    if len(clean) <= CACHE_TEXT:
        if len(_cache) >= CACHE_SIZE:
            _cache.pop(next(iter(_cache)))
        _cache[key] = data
    return data


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):  # noqa: N802 — имя задаёт http.server
        if self.path.rstrip("/") != "/tts":
            self.send_error(404)
            return
        try:
            length = int(self.headers.get("Content-Length", 0))
            body = json.loads(self.rfile.read(length) or b"{}")
            wav = synthesize(str(body.get("text", "")), body.get("ref_audio_path"), str(body.get("prompt_text") or ""))
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
    # Отклики — в фоне: сервис уже отвечает, пока они готовятся.
    threading.Thread(target=lambda: [synthesize(t, None) for t in WARM], daemon=True).start()
    # Только петлевой адрес: голос не должен быть доступен соседям по сети.
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"F5 готов: http://127.0.0.1:{args.port}/tts", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
