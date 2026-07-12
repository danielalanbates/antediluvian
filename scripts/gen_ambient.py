#!/usr/bin/env python3
"""Synthesize per-act CC0 ambient loops (CHUNK C12) — no external samples.

Each act gets a ~24 s seamless loop built from filtered noise + simple tones:
eden birdsong forest, hermon high wind, nephilim low war-drum rumble, enoch
city murmur + forge, flood rain + thunder. Deterministic (fixed seed).
"""
import numpy as np, subprocess, os

SR = 22050
DUR = 24.0
N = int(SR * DUR)
rng = np.random.default_rng(7)
t = np.arange(N) / SR
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = f"{ROOT}/assets/audio/ambient"
os.makedirs(OUT, exist_ok=True)

def lowpass(x, alpha):
    y = np.empty_like(x); acc = 0.0
    for i, v in enumerate(x):
        acc += alpha * (v - acc); y[i] = acc
    return y

def loopify(x):
    # Crossfade tail into head for a seamless loop.
    f = int(SR * 1.5); w = np.linspace(0, 1, f)
    x[:f] = x[:f] * w + x[-f:] * (1 - w)
    return x[: N - f]

def norm(x, level=0.4):
    return (x / (np.abs(x).max() + 1e-9)) * level

def save(name, x):
    x = loopify(x.astype(np.float32))
    raw = (norm(x) * 32767).astype(np.int16).tobytes()
    p = subprocess.run(
        ["ffmpeg", "-y", "-f", "s16le", "-ar", str(SR), "-ac", "1", "-i", "-",
         "-c:a", "vorbis", "-strict", "-2", "-sample_fmt", "fltp", "-ac", "2", f"{OUT}/{name}.ogg"],
        input=raw, capture_output=True)
    assert p.returncode == 0, p.stderr[-300:]
    print(name, os.path.getsize(f"{OUT}/{name}.ogg") // 1024, "KB")

wind = lowpass(rng.standard_normal(N), 0.02)
breeze = wind * (0.6 + 0.4 * np.sin(2 * np.pi * 0.05 * t))

# Eden: gentle breeze + sparse birdsong chirps.
eden = breeze.copy() * 0.5
for _ in range(26):
    s = rng.integers(0, N - SR); f0 = rng.uniform(1800, 3600); d = rng.uniform(0.08, 0.25)
    n = int(d * SR); tt = np.arange(n) / SR
    chirp = np.sin(2 * np.pi * (f0 + 600 * np.sin(2 * np.pi * 8 * tt)) * tt) * np.hanning(n)
    eden[s:s + n] += 0.35 * chirp
save("eden", eden)

# Hermon: strong high-altitude wind, slow gusts.
save("hermon", lowpass(rng.standard_normal(N), 0.05) * (0.5 + 0.5 * np.sin(2 * np.pi * 0.03 * t + 1)))

# Nephilim: menacing rumble + distant war drums.
neph = lowpass(rng.standard_normal(N), 0.008) * 1.2
beat = (np.sin(2 * np.pi * 55 * t) * np.exp(-((t % 2.0) * 6))) * 0.7
save("nephilim", neph + beat)

# Enoch: city murmur (mid noise) + rhythmic forge clanks.
enoch = lowpass(rng.standard_normal(N), 0.12) * 0.5
for s in range(0, N - SR, int(3.1 * SR)):
    n = int(0.12 * SR); tt = np.arange(n) / SR
    enoch[s:s + n] += 0.3 * np.sin(2 * np.pi * 1200 * tt) * np.exp(-tt * 40)
save("enoch", enoch)

# Flood: dense rain + occasional thunder roll.
rain = lowpass(rng.standard_normal(N), 0.3) * 0.7
for _ in range(3):
    s = rng.integers(0, N - 3 * SR); n = 3 * SR; tt = np.arange(n) / SR
    rain[s:s + n] += lowpass(rng.standard_normal(n), 0.01) * np.exp(-tt * 1.2) * 1.5
save("flood", rain)
