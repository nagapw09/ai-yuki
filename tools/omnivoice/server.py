"""Голос по образцу для Yuki: OmniVoice (k2-fsa, движок VoiceStudio).

На слух владельца ближе всех к образцу (Мита из MiSide) и на полном качестве
(32 шага) звучит естественно; ускоренные 8 и 16 шагов «тараторят», поэтому по
умолчанию стоит 32. Лицензия Apache-2.0.

Протокол тот же, что у остальных сервисов Yuki: ``POST /tts`` с
``{text, ref_audio_path, prompt_text}`` возвращает WAV. Текст образца — из
файла рядом (``mita.wav`` → ``mita.txt``) или из настроек Yuki.

Запуск::

    python tools/omnivoice/server.py --port 9880 --ref C:\\...\\voices\\mita.wav
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

_lock = threading.Lock()
_state: dict = {}
# Отпечатки голоса по образцам: считать их на каждую фразу — лишние полсекунды.
_prompts: dict[tuple[str, float, str], object] = {}

# Короткие фразы повторяются («Секунду», «Да?») — готовим один раз.
_cache: dict[tuple[str, str], bytes] = {}
CACHE_TEXT = 60
CACHE_SIZE = 128
# Отклики из apps/desktop/src/agent/ack.ts, voice.ts и quick.ts.
WARM = [
    "Да?", "Слушаю.", "Да-да?", "М?",
    "Секунду.", "Сейчас сделаю.", "Минутку.", "Секунду, открываю.", "Сейчас открою.",
    "Сейчас поищу.", "Минутку, смотрю.", "Сейчас запишу.", "Секунду, запоминаю.",
    "Сейчас напишу.", "Сейчас подумаю.", "Хм, секунду.",
    "Открываю.", "Открываю, секунду.", "Готово.", "Сделала.", "Есть.",
    "Я тут!", "Пока-пока!", "Звук включён.", "Звук выключен.",
]


def setup(ref: str, steps: int, model_path: str = "k2-fsa/OmniVoice"):
    from omnivoice import OmniVoice

    device = "cuda:0" if torch.cuda.is_available() else ("mps" if torch.backends.mps.is_available() else "cpu")
    _state["model"] = OmniVoice.from_pretrained(model_path, device_map=device)
    _state["ref"] = ref
    _state["steps"] = steps
    model = _state["model"]
    _state["rate"] = int(getattr(model, "sampling_rate", 0) or getattr(getattr(model, "config", None), "sampling_rate", 24000))


def transcript(ref: str, fallback: str = "") -> str:
    """Текст образца: файл рядом важнее поля в настройках — он принадлежит образцу."""
    sidecar = os.path.splitext(ref)[0] + ".txt"
    if os.path.isfile(sidecar):
        return open(sidecar, encoding="utf-8").read().strip()
    if fallback.strip():
        return fallback.strip()
    raise ValueError(f"нет текста образца: положите {os.path.basename(sidecar)} рядом или впишите текст в настройках")


def voice_prompt(ref: str, text: str):
    """Отпечаток голоса; пересчитывается, только если образец изменили."""
    key = (os.path.normcase(os.path.abspath(ref)), os.path.getmtime(ref), text)
    if key not in _prompts:
        # Образец читаем сами: torchaudio на Windows без FFmpeg его не откроет.
        data, rate = sf.read(ref, dtype="float32", always_2d=True)
        audio = (torch.from_numpy(data.T.copy()), rate)
        _prompts[key] = _state["model"].create_voice_clone_prompt(ref_audio=audio, ref_text=text)
    return _prompts[key]


UNITS = {"%": ("процент", "процента", "процентов"), "°": ("градус", "градуса", "градусов")}


def plural(n: int, forms: tuple[str, str, str]) -> str:
    if n % 10 == 1 and n % 100 != 11:
        return forms[0]
    if 2 <= n % 10 <= 4 and not 12 <= n % 100 <= 14:
        return forms[1]
    return forms[2]


def say_numbers(text: str) -> str:
    """Цифры — словами: модель читает «20:40» и «15%» как попало.

    Время «20:40» → «двадцать сорок», «15%» → «пятнадцать процентов»,
    «+14°» → «плюс четырнадцать градусов», остальные числа — словами.
    """
    try:
        from num2words import num2words
    except ImportError:
        return text

    def words(n: str) -> str:
        return num2words(int(n), lang="ru")

    text = re.sub(r"\b(\d{1,2}):(\d{2})\b",
                  lambda m: f"{words(m[1])} {'ноль ' + words(m[2]) if m[2].startswith('0') and m[2] != '00' else ('ровно' if m[2] == '00' else words(m[2]))}",
                  text)
    text = re.sub(r"\+(\d)", r"плюс \1", text)
    text = re.sub(r"(?<![\w])-(\d)", r"минус \1", text)
    text = re.sub(r"(\d+)\s*([%°])", lambda m: f"{words(m[1])} {plural(int(m[1]), UNITS[m[2]])}", text)
    text = re.sub(r"(\d+)[.,](\d+)", lambda m: f"{words(m[1])} запятая {words(m[2])}", text)
    return re.sub(r"\d+", lambda m: words(m[0]), text)


def synthesize(text: str, ref: str | None, prompt_text: str = "") -> bytes:
    if not ref or not os.path.isfile(ref):
        ref, prompt_text = _state["ref"], ""
    clean = say_numbers(text).strip()[:600] or "Готово."
    key = (os.path.normcase(os.path.abspath(ref)), clean)
    if key in _cache:
        return _cache[key]

    from omnivoice.models.omnivoice import OmniVoiceGenerationConfig

    with _lock:
        prompt = voice_prompt(ref, transcript(ref, prompt_text))
        audio = _state["model"].generate(
            text=clean, language="ru", voice_clone_prompt=prompt,
            generation_config=OmniVoiceGenerationConfig(num_step=_state["steps"]),
        )[0]
    buffer = io.BytesIO()
    sf.write(buffer, audio, _state["rate"], format="WAV", subtype="PCM_16")
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
    parser.add_argument("--ref", default=r"C:\Users\alex\Documents\Yuki\voices\mita.wav")
    parser.add_argument("--steps", type=int, default=32, help="шаги генерации: меньше — быстрее, но тараторит")
    parser.add_argument("--model", default="k2-fsa/OmniVoice", help="папка модели или её имя на Hugging Face")
    args, _ = parser.parse_known_args()
    setup(args.ref, args.steps, args.model)
    # Прогрев, только если образец есть: на свежей установке его ещё нет,
    # а сервис должен подняться и ответить понятной ошибкой, а не упасть.
    if os.path.isfile(args.ref):
        synthesize("Привет.", None)
    # Отклики — в фоне: сервис уже отвечает, пока они готовятся.
    if os.path.isfile(args.ref):
        threading.Thread(target=lambda: [synthesize(t, None) for t in WARM], daemon=True).start()
    # Только петлевой адрес: голос не должен быть доступен соседям по сети.
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"OmniVoice готов: http://127.0.0.1:{args.port}/tts", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
