"""Голос персонажа для Yuki: Silero произносит, RVC перекрашивает тембр.

Клонирование по одному образцу (XTTS, F5) передаёт тембр лишь примерно. RVC
учится на минутах голоса конкретного персонажа и превращает любую речь в его
тембр — так обычно и делают голоса аниме-героев. Текст произносит Silero:
он быстрый и внятный, а его собственный тембр RVC всё равно заменит.

Протокол тот же, что у остальных сервисов Yuki (взят у GPT-SoVITS):
``POST /tts`` с ``{text}`` возвращает WAV. Образец из запроса не нужен —
голос задаёт обученная модель.

Запуск из окружения Applio (https://github.com/IAHispano/Applio)::

    D:\\yuki-voice\\applio\\.venv\\Scripts\\python.exe tools/rvc/server.py --port 9880 \\
        --applio D:\\yuki-voice\\applio --model mita

Лицензии: Silero — CC BY-NC-SA, Applio — MIT. Для себя можно, продавать нельзя.
"""

from __future__ import annotations

import argparse
import glob
import io
import json
import os
import re
import sys
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import soundfile as sf
import torch

_lock = threading.Lock()
_state: dict = {}


def setup(applio: str, model: str, speaker: str, pitch: int, index_rate: float):
    """Грузит Silero и модель RVC один раз: первая фраза не должна ждать минуту."""
    sys.path.insert(0, applio)
    os.chdir(applio)  # Applio ищет свои модели относительно корня
    from rvc.infer.infer import VoiceConverter

    weights = sorted(glob.glob(os.path.join(applio, "logs", model, f"{model}_*e_*s.pth")), key=os.path.getmtime)
    if not weights:
        raise SystemExit(f"нет обученной модели {model} в {applio}\\logs")
    index = next(iter(glob.glob(os.path.join(applio, "logs", model, "*.index"))), "")

    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    silero, _ = torch.hub.load("snakers4/silero-models", "silero_tts", language="ru", speaker="v4_ru", trust_repo=True)
    silero.to(device)

    _state.update(
        silero=silero, converter=VoiceConverter(), weights=weights[-1], index=index,
        speaker=speaker, pitch=pitch, index_rate=index_rate,
    )
    print(f"модель: {os.path.basename(weights[-1])}, индекс: {os.path.basename(index) or 'нет'}", flush=True)


def numbers_to_words(text: str) -> str:
    """Silero не читает цифры: «20:40» он пропустил бы молча."""
    try:
        from num2words import num2words
    except ImportError:
        return text
    return re.sub(r"\d+", lambda m: num2words(int(m.group()), lang="ru"), text)


def synthesize(text: str) -> bytes:
    clean = numbers_to_words(text).strip()[:900] or "Готово."
    with _lock, tempfile.TemporaryDirectory() as tmp:
        base, out = os.path.join(tmp, "base.wav"), os.path.join(tmp, "out.wav")
        audio = _state["silero"].apply_tts(text=clean, speaker=_state["speaker"], sample_rate=48000)
        sf.write(base, audio.cpu().numpy(), 48000, subtype="PCM_16")
        _state["converter"].convert_audio(
            audio_input_path=base, audio_output_path=out, model_path=_state["weights"],
            index_path=_state["index"], pitch=_state["pitch"], index_rate=_state["index_rate"],
            f0_method="rmvpe", protect=0.4, embedder_model="contentvec",
        )
        with open(out, "rb") as f:
            return f.read()


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):  # noqa: N802 — имя задаёт http.server
        if self.path.rstrip("/") != "/tts":
            self.send_error(404)
            return
        try:
            length = int(self.headers.get("Content-Length", 0))
            body = json.loads(self.rfile.read(length) or b"{}")
            wav = synthesize(str(body.get("text", "")))
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
    parser.add_argument("--applio", default=r"D:\yuki-voice\applio")
    parser.add_argument("--model", default="mita")
    parser.add_argument("--speaker", default="xenia")
    parser.add_argument("--pitch", type=int, default=0, help="сдвиг высоты в полутонах")
    parser.add_argument("--index-rate", type=float, default=0.75)
    args = parser.parse_args()
    setup(args.applio, args.model, args.speaker, args.pitch, args.index_rate)
    # Только петлевой адрес: голос не должен быть доступен соседям по сети.
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"RVC готов: http://127.0.0.1:{args.port}/tts", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
