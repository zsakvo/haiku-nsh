"""Generate isolated, copyright-free FLAC/embedded PNG/LRC acceptance media.

Run from the repository root: python3 scripts/generate-player-fixture.py
Uses a FLAC verbatim subframe encoder so no external audio/image tools are needed.
"""
from pathlib import Path
import hashlib
import math
import struct
import zlib

ROOT = Path(__file__).resolve().parents[1] / ".artifacts/m1-04/media"
RATE, SECONDS, BLOCK = 8000, 120, 4096


def crc(data, bits, polynomial):
    value = 0
    mask = (1 << bits) - 1
    for byte in data:
        value ^= byte << (bits - 8)
        for _ in range(8):
            value = ((value << 1) ^ (polynomial if value & (1 << (bits - 1)) else 0)) & mask
    return value


def chunk(name, data):
    return struct.pack(">I", len(data)) + name + data + struct.pack(">I", zlib.crc32(name + data))


def cover(warm):
    rows = []
    for y in range(320):
        row = bytearray([0])
        for x in range(320):
            sky = (28 + y // 4, 95 + y // 6, 112 + y // 8) if not warm else (138 + y // 4, 72 + y // 6, 42 + y // 8)
            color = sky
            if (x - 224) ** 2 + (y - 95) ** 2 < 38 ** 2:
                color = (245, 192, 121)
            if y > 190 + 15 * math.sin(x / 53):
                color = (22 + y // 16, 55 + y // 12, 66 + y // 10)
            if y > 255 + 8 * math.sin(x / 31):
                color = (16, 35, 43)
            row.extend(color)
        rows.append(row)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 320, 320, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(b"".join(rows))) + chunk(b"IEND", b""))


def audio():
    samples = [round(180 * math.sin(2 * math.pi * 220 * i / RATE)) for i in range(RATE * SECONDS)]
    pcm = struct.pack("<" + "h" * len(samples), *samples)
    info = (struct.pack(">HH", BLOCK, BLOCK) + b"\0" * 6
            + ((RATE << 44) | (15 << 36) | len(samples)).to_bytes(8, "big") + hashlib.md5(pcm).digest())
    frames = bytearray()
    for frame, start in enumerate(range(0, len(samples), BLOCK)):
        block = samples[start:start + BLOCK]
        header = b"\xff\xf8\x70\x08" + chr(frame).encode("utf-8") + struct.pack(">H", len(block) - 1)
        body = header + bytes([crc(header, 8, 0x07)]) + b"\x02" + struct.pack(">" + "h" * len(block), *block)
        frames.extend(body + struct.pack(">H", crc(body, 16, 0x8005)))
    return info, frames


def block(kind, payload, last=False):
    return bytes([kind | (128 if last else 0)]) + len(payload).to_bytes(3, "big") + payload


def main():
    ROOT.mkdir(parents=True, exist_ok=True)
    info, frames = audio()
    for index, title in enumerate(("海边的清晨", "暮色归途")):
        png = cover(bool(index))
        comments = [f"TITLE={title}", "ARTIST=Haiku 测试音源", "ALBUM=窗口与时钟", "ALBUMARTIST=Haiku 测试音源"]
        vendor = b"Haiku M1-04 generated fixture"
        vorbis = struct.pack("<I", len(vendor)) + vendor + struct.pack("<I", len(comments))
        for text in comments:
            value = text.encode()
            vorbis += struct.pack("<I", len(value)) + value
        mime = b"image/png"
        picture = (struct.pack(">II", 3, len(mime)) + mime + struct.pack(">I", 0)
                   + struct.pack(">IIIII", 320, 320, 24, 0, len(png)) + png)
        path = ROOT / f"{index + 1:02}.flac"
        path.write_bytes(b"fLaC" + block(0, info) + block(4, vorbis) + block(6, picture, True) + frames)
        lines = []
        phrases = ["晨光落在海面", "潮水轻轻靠岸", "风从窗边经过", "沿着声音前行",
                   "把片刻留给自己", "远处的灯亮起来", "每一步都有回响", "让旋律慢慢展开"]
        for line in range(40):
            second = line * 3
            stamp = f"{second // 60:02}:{second % 60:02}"
            phrase = phrases[(line + index) % len(phrases)]
            # Enhanced LRC exercises the word-fill view in the real window.
            words = f"<{stamp}.00>{phrase[:3]}<{second // 60:02}:{second % 60 + 1:02}.50>{phrase[3:]}"
            lines.append(f"[{stamp}.00]{words}")
        path.with_suffix(".lrc").write_text("\n".join(lines) + "\n", encoding="utf-8")
        path.with_suffix(".zh.lrc").write_text("\n".join(
            f"[{i * 3 // 60:02}:{i * 3 % 60:02}.00]本地译文 · 第 {i + 1} 行" for i in range(40)), encoding="utf-8")
        print(f"{path}: 120s quiet 220Hz, {path.stat().st_size} bytes, embedded PNG {len(png)} bytes, 40 LRC lines")


if __name__ == "__main__":
    main()
