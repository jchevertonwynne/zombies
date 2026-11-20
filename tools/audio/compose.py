#!/usr/bin/env python3
"""Compose the game's jingles and stingers, and hand each to process.py as a CC0 cue of our own.

    python3 tools/audio/compose.py                 # every piece
    python3 tools/audio/compose.py round_start ... # just these slots
    python3 tools/audio/compose.py --list
    python3 tools/audio/compose.py --keep DIR      # also keep the rendered WAVs in DIR

Every piece here is an original work written for this project and released as CC0. Nothing is
sampled: each instrument is synthesised from scratch (band-limited wavetables, sines and seeded
noise), so a run is reproducible from this file alone, given the same ffmpeg.

The sound is meant to sit beside World at War's: minor-key brass and low strings stabs, timpani and
a field snare, a music box for the Box, and the Perk machines' jingles played through an old radio
speaker. Each piece goes through the same chain:

1. Score -> dry stereo mix (Python, standard library only).
2. A room (ffmpeg `afir` with a generated impulse response) and a tape-ish band (high- and
   low-pass); a Perk jingle gets a narrow radio band instead.
3. Tape: wow and flutter, a soft saturation and a little hiss (Python), and for some pieces the
   tape running down at the end.
4. `process.py`: loudness to the `music` group, mono for a Spatial slot, OGG q4 and the manifest
   entry, with licence "CC0 (composed for this project)" and this script as its source.

Needs Python 3.9+ and `ffmpeg` with libvorbis. No third-party Python packages.
"""
import argparse
import array
import math
import pathlib
import random
import shutil
import subprocess
import sys
import tempfile
import wave

SR = 44100
HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[1]
AUDIO = ROOT / "assets" / "audio"
LICENCE = "CC0 (composed for this project)"
SOURCE = "tools/audio/compose.py"


# --- pitch -----------------------------------------------------------------------------------

NOTE = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}


def hz(name):
    """'D3', 'F#4', 'Bb2' -> frequency in Hz (A4 = 440)."""
    pc = NOTE[name[0]]
    rest = name[1:]
    while rest and rest[0] in "#b":
        pc += 1 if rest[0] == "#" else -1
        rest = rest[1:]
    midi = 12 * (int(rest) + 1) + pc
    return 440.0 * 2 ** ((midi - 69) / 12)


# --- building blocks -------------------------------------------------------------------------

_TABLES = {}
TABLE = 2048


def saw_table(harmonics, rolloff=1.0):
    """One cycle of a band-limited saw with `harmonics` partials falling as 1/k^rolloff."""
    key = (harmonics, rolloff)
    if key not in _TABLES:
        t = [0.0] * TABLE
        for k in range(1, harmonics + 1):
            a = 1.0 / k**rolloff
            w = 2 * math.pi * k / TABLE
            for i in range(TABLE):
                t[i] += a * math.sin(w * i)
        peak = max(abs(x) for x in t)
        t = [x / peak for x in t]
        t.append(t[0])
        _TABLES[key] = t
    return _TABLES[key]


def harmonics_for(f, ceiling=15000.0, most=48):
    return max(1, min(most, int(ceiling / f)))


def pitch_curve(n, scoop=0.0, scoop_t=0.06, vib_cents=0.0, vib_rate=5.0, vib_delay=0.2, glide=0.0, glide_t=0.0, phase=0.0):
    """A per-sample frequency multiplier: an onset scoop (cents, closing over `scoop_t`), a delayed
    vibrato, and a glide of `glide` semitones over `glide_t` seconds."""
    out = [1.0] * n
    block = 32
    for b in range(0, n, block):
        t = b / SR
        cents = scoop * math.exp(-t / scoop_t) if scoop else 0.0
        if vib_cents and t > vib_delay:
            depth = min(1.0, (t - vib_delay) / 0.3)
            cents += vib_cents * depth * math.sin(2 * math.pi * vib_rate * t + phase)
        if glide:
            cents += 100 * glide * min(1.0, t / glide_t if glide_t else 1.0)
        m = 2 ** (cents / 1200)
        for i in range(b, min(n, b + block)):
            out[i] = m
    return out


def wt_osc(table, f, n, mult=None, phase=0.0):
    size = len(table) - 1
    out = [0.0] * n
    inc = f * size / SR
    p = (phase % 1.0) * size
    if mult is None:
        for i in range(n):
            ip = int(p)
            a = table[ip]
            out[i] = a + (table[ip + 1] - a) * (p - ip)
            p += inc
            if p >= size:
                p -= size
    else:
        for i in range(n):
            ip = int(p)
            a = table[ip]
            out[i] = a + (table[ip + 1] - a) * (p - ip)
            p += inc * mult[i]
            if p >= size:
                p -= size
    return out


def adsr(n, held, a, d, s, r, curve=1.0):
    """Linear attack, a curved decay to `s`, held for `held` seconds, then a release."""
    ha = max(1, int(a * SR))
    hd = max(1, int(d * SR))
    hh = max(1, int(held * SR))
    hr = max(1, int(r * SR))

    def held_level(i):
        if i < ha:
            return i / ha
        k = min(1.0, (i - ha) / hd)
        return s + (1 - s) * (1 - k) ** 2

    base = held_level(hh)
    out = [0.0] * n
    for i in range(n):
        level = held_level(i) if i < hh else base * max(0.0, 1 - (i - hh) / hr) ** 2
        out[i] = level**curve
    return out


def lowpass(x, cut, poles=2):
    """One-pole lowpass `poles` times; `cut` is a number or a per-sample list of Hz."""
    n = len(x)
    y = list(x)
    for _ in range(poles):
        z = 0.0
        if isinstance(cut, (int, float)):
            g = 1 - math.exp(-2 * math.pi * min(cut, SR * 0.45) / SR)
            for i in range(n):
                z += g * (y[i] - z)
                y[i] = z
        else:
            for b in range(0, n, 64):
                g = 1 - math.exp(-2 * math.pi * min(cut[b], SR * 0.45) / SR)
                for i in range(b, min(n, b + 64)):
                    z += g * (y[i] - z)
                    y[i] = z
    return y


def highpass(x, cut):
    low = lowpass(x, cut, poles=1)
    return [a - b for a, b in zip(x, low)]


def noise(rng, n):
    return [rng.uniform(-1.0, 1.0) for _ in range(n)]


# --- instruments -----------------------------------------------------------------------------


def brass(f, held, vel=1.0, bright=1.0, rel=0.22, a=None, ens=(-7, 0, 6), scoop=-35, vib=7, rng=None, muted=False, swell=0.0):
    """A brass section note: detuned band-limited saws whose brightness follows their loudness.
    `swell` > 0 makes the note crescendo over its length instead of attacking hard."""
    rng = rng or random.Random(int(f * 100))
    n = int((held + rel) * SR)
    attack = a if a is not None else 0.018 + 0.03 * (1 - vel)
    env = adsr(n, held, attack, 0.18, 0.72, rel)
    if swell:
        hh = int(held * SR)
        for i in range(n):
            k = min(1.0, i / max(1, hh))
            env[i] *= (1 - swell) + swell * k * k
    tab = saw_table(harmonics_for(f))
    out = [0.0] * n
    for cents in ens:
        mult = pitch_curve(n, scoop=scoop * vel, vib_cents=vib, vib_rate=4.6 + rng.random(), vib_delay=0.25, phase=rng.random() * 6)
        o = wt_osc(tab, f * 2 ** (cents / 1200), n, mult, rng.random())
        for i in range(n):
            out[i] += o[i]
    top = 2.2 if muted else 12.0
    cut = [min(11000.0, f * (1.3 + top * bright * vel * e * e)) for e in env]
    out = lowpass(out, cut)
    if muted:
        out = highpass(out, 500)
    g = vel / len(ens)
    return [s * e * g for s, e in zip(out, env)]


def strings(f, held, vel=1.0, rel=0.45, a=0.12, ens=(-12, -5, 0, 4, 11), tremolo=0.0, glide=0.0, glide_t=0.0, rng=None, swell=0.0, bright=5.0):
    """A string section: five detuned saws, a slower bow, vibrato; optionally bowed tremolo and a glide."""
    rng = rng or random.Random(int(f * 37))
    n = int((held + rel) * SR)
    env = adsr(n, held, a, 0.3, 0.85, rel)
    if swell:
        hh = int(held * SR)
        for i in range(n):
            k = min(1.0, i / max(1, hh))
            env[i] *= (1 - swell) + swell * k * k
    tab = saw_table(harmonics_for(f, 12000))
    out = [0.0] * n
    for cents in ens:
        mult = pitch_curve(n, vib_cents=9, vib_rate=5.2 + rng.random() * 0.8, vib_delay=0.15, glide=glide, glide_t=glide_t, phase=rng.random() * 6)
        o = wt_osc(tab, f * 2 ** (cents / 1200), n, mult, rng.random())
        for i in range(n):
            out[i] += o[i]
    out = lowpass(out, min(5200.0, f * bright + 600))
    if tremolo:
        # Bowed tremolo: each stroke a little bump, at a slightly unsteady rate.
        t = 0.0
        ph = 0.0
        for i in range(n):
            ph += (tremolo * (1 + 0.08 * math.sin(t * 3.1))) / SR
            t += 1 / SR
            frac = ph % 1.0
            env[i] *= 0.55 + 0.45 * (1 - frac) ** 2
    g = vel / len(ens)
    return [s * e * g for s, e in zip(out, env)]


def timpani(f, vel=1.0, decay=1.4, rng=None):
    rng = rng or random.Random(int(f * 13))
    n = int((decay * 2.2) * SR)
    partials = ((1.0, 1.0, 1.0), (1.504, 0.55, 0.7), (1.742, 0.35, 0.55), (2.0, 0.25, 0.5), (2.245, 0.15, 0.4))
    out = [0.0] * n
    for ratio, amp, dk in partials:
        ph = rng.random() * 6
        w = 2 * math.pi * f * ratio / SR
        tau = decay * dk * SR
        for i in range(n):
            bend = 1 + 0.035 * math.exp(-i / (0.06 * SR))
            ph += w * bend
            out[i] += amp * math.sin(ph) * math.exp(-i / tau)
    hit = lowpass(noise(rng, int(0.05 * SR)), 900)
    for i, s in enumerate(hit):
        out[i] += 2.2 * s * (1 - i / len(hit)) ** 2
    return [s * vel * 0.5 for s in out]


def timpani_roll(f, length, v0, v1, rate=14.0, rng=None):
    rng = rng or random.Random(int(f * 7 + length * 100))
    out = [0.0] * int((length + 1.6) * SR)
    t = 0.0
    while t < length:
        k = t / length
        hit = timpani(f, (v0 + (v1 - v0) * k) * (0.8 + 0.2 * rng.random()), decay=1.0, rng=rng)
        start = int(t * SR)
        for i, s in enumerate(hit[: len(out) - start]):
            out[start + i] += s * 0.45
        t += (1 / rate) * (0.9 + 0.2 * rng.random())
    return out


def bass_drum(vel=1.0, rng=None):
    rng = rng or random.Random(5)
    n = int(0.9 * SR)
    out = [0.0] * n
    ph = 0.0
    for i in range(n):
        t = i / SR
        f = 42 + 55 * math.exp(-t / 0.04)
        ph += 2 * math.pi * f / SR
        out[i] = math.sin(ph) * math.exp(-t / 0.28)
    click = lowpass(noise(rng, int(0.02 * SR)), 2500)
    for i, s in enumerate(click):
        out[i] += 0.6 * s * (1 - i / len(click))
    return [s * vel for s in out]


def snare(vel=1.0, rng=None, decay=0.13):
    rng = rng or random.Random(9)
    n = int(decay * 3 * SR)
    rattle = highpass(lowpass(noise(rng, n), 7000, poles=1), 1400)
    out = [0.0] * n
    ph = 0.0
    for i in range(n):
        t = i / SR
        ph += 2 * math.pi * 185 / SR
        out[i] = 0.9 * rattle[i] * math.exp(-t / decay) + 0.6 * math.sin(ph) * math.exp(-t / 0.035)
    return [s * vel * 0.6 for s in out]


def snare_roll(length, v0, v1, rng=None):
    rng = rng or random.Random(int(length * 1000))
    out = [0.0] * int((length + 0.5) * SR)
    t = 0.0
    while t < length:
        k = t / length
        hit = snare((v0 + (v1 - v0) * k) * (0.75 + 0.25 * rng.random()), rng, decay=0.07)
        start = int(t * SR)
        for i, s in enumerate(hit[: len(out) - start]):
            out[start + i] += s * 0.55
        t += (1 / 26) * (0.85 + 0.3 * rng.random())
    return out


def cymbal_swell(length, rng=None):
    """A reversed-cymbal crescendo into a hit `length` seconds in."""
    rng = rng or random.Random(int(length * 333))
    n = int(length * SR)
    hiss = highpass(lowpass(noise(rng, n), 9000, poles=1), 2500)
    return [s * (i / n) ** 3 * 0.5 for i, s in enumerate(hiss)]


def tine(f, vel=1.0, decay=1.1, rng=None, partials=((1.0, 1.0, 1.0), (2.0, 0.12, 0.5), (5.4, 0.18, 0.18), (8.9, 0.07, 0.08))):
    """A plucked metal tine: a music box (default) or, with other partials, a celesta or a bell."""
    rng = rng or random.Random(int(f * 3))
    n = int(decay * 3.2 * SR)
    out = [0.0] * n
    for ratio, amp, dk in partials:
        fr = f * ratio
        if fr > 16000:
            continue
        w = 2 * math.pi * fr / SR
        ph = rng.random() * 6
        tau = decay * dk * SR
        for i in range(n):
            out[i] += amp * math.sin(ph + w * i) * math.exp(-i / tau)
    for i in range(min(n, 60)):
        out[i] *= i / 60
    return [s * vel * 0.4 for s in out]


CELESTA = ((1.0, 1.0, 1.0), (2.0, 0.35, 0.45), (3.0, 0.12, 0.3), (4.0, 0.06, 0.2))
BELL = ((0.56, 0.5, 1.0), (0.92, 0.4, 0.7), (1.19, 0.35, 0.55), (1.71, 0.3, 0.4), (2.0, 0.5, 0.35), (2.74, 0.25, 0.25), (3.0, 0.2, 0.2), (3.76, 0.15, 0.15), (4.07, 0.12, 0.12))


def piano(f, held, vel=1.0, rng=None):
    """An out-of-tune upright: two detuned dark saws, a fast fall and a closing filter."""
    rng = rng or random.Random(int(f * 11))
    n = int((held + 0.25) * SR)
    tab = saw_table(harmonics_for(f, 9000), 1.4)
    out = [0.0] * n
    for cents in (-9, 8):
        o = wt_osc(tab, f * 2 ** (cents / 1200), n, None, rng.random())
        for i in range(n):
            out[i] += o[i]
    cut = [f * (1.5 + 10 * math.exp(-i / (0.12 * SR))) for i in range(n)]
    out = lowpass(out, cut)
    hh = int(held * SR)
    for i in range(n):
        e = math.exp(-i / (0.9 * SR)) * min(1.0, i / 40)
        if i > hh:
            e *= max(0.0, 1 - (i - hh) / (0.25 * SR))
        out[i] *= e * vel * 0.5
    return out


def growl(length, rng=None):
    """A low throat rumble under the Hellhound stinger."""
    rng = rng or random.Random(66)
    n = int(length * SR)
    src = lowpass(noise(rng, n), 220)
    out = [0.0] * n
    ph = 0.0
    for i in range(n):
        t = i / SR
        ph += 2 * math.pi * (23 + 6 * math.sin(t * 2.3)) / SR
        env = math.sin(math.pi * min(1.0, t / length)) ** 0.7
        out[i] = src[i] * (0.6 + 0.4 * math.sin(ph)) * env * 4.0
    return out


# --- a mix -----------------------------------------------------------------------------------


class Mix:
    """A stereo mix that grows to fit what's added; `secs` is only its starting length."""

    def __init__(self, secs, seed):
        self.n = int(secs * SR)
        self.L = [0.0] * self.n
        self.R = [0.0] * self.n
        self.rng = random.Random(seed)

    def pad(self, n):
        if n > self.n:
            self.L.extend([0.0] * (n - self.n))
            self.R.extend([0.0] * (n - self.n))
            self.n = n

    def add(self, sig, at, gain=1.0, pan=0.0):
        start = int(at * SR)
        theta = (pan + 1) * math.pi / 4
        gl, gr = math.cos(theta) * gain, math.sin(theta) * gain
        self.pad(start + len(sig))
        L, R = self.L, self.R
        for i, s in enumerate(sig):
            L[start + i] += s * gl
            R[start + i] += s * gr

    def chord(self, inst, notes, at, gain=1.0, spread=0.5, **kw):
        k = len(notes)
        for j, name in enumerate(notes):
            pan = 0.0 if k == 1 else -spread + 2 * spread * j / (k - 1)
            self.add(inst(hz(name), rng=self.rng, **kw), at, gain, pan)


# --- the pieces ------------------------------------------------------------------------------
# Each returns (mix, options). D minor is home; the Perk jingles each have their own key.

PIECES = {}


def piece(slot, desc, **fx):
    def register(fn):
        PIECES[slot] = (fn, desc, fx)
        return fn

    return register


@piece("round_start", "Round start: a pickup and a held D minor brass stab with a b6 lean, over low strings and timpani")
def round_start():
    m = Mix(3.3, 1)
    m.add(timpani(hz("A1"), 0.6, rng=m.rng), 0.0, 0.9)
    m.chord(brass, ["A2", "D3", "A3"], 0.0, 0.8, held=0.16, vel=0.85)
    m.add(timpani(hz("D2"), 1.0, rng=m.rng), 0.42)
    m.add(bass_drum(0.8, m.rng), 0.42, 0.7)
    m.chord(strings, ["D2", "A2"], 0.42, 0.9, held=1.9, vel=0.9, a=0.03)
    m.chord(brass, ["D3", "F3", "Bb3", "D4"], 0.42, 1.0, held=1.05, vel=1.0)
    m.chord(brass, ["D3", "F3", "A3", "D4"], 1.42, 0.8, held=0.9, vel=0.7, scoop=0, a=0.08)
    return m


@piece("round_end", "Round end: a falling brass line A-G-F-E to D over an open fifth, timpani dying away", max_secs=4.5)
def round_end():
    m = Mix(4.2, 2)
    m.chord(strings, ["D2", "A2"], 0.0, 0.7, held=3.2, vel=0.7, a=0.4)
    m.add(timpani_roll(hz("D2"), 1.6, 0.5, 0.15, rng=m.rng), 0.0, 0.8)
    line = [("A3", 0.0, 0.35), ("G3", 0.4, 0.35), ("F3", 0.8, 0.35), ("E3", 1.2, 0.5)]
    under = ["F3", "E3", "D3", "C#3"]
    for (note, at, held), third in zip(line, under):
        m.add(brass(hz(note), held, vel=0.75, bright=0.6, rng=m.rng), at, 0.9, -0.15)
        m.add(brass(hz(third), held, vel=0.55, bright=0.5, rng=m.rng), at, 0.5, 0.2)
    m.chord(brass, ["D3", "A3", "D4"], 1.8, 0.85, held=1.4, vel=0.7, bright=0.6)
    m.add(timpani(hz("D2"), 0.6, decay=1.8, rng=m.rng), 1.8)
    return m


@piece("hellhound_round_start", "Hellhound Round start: a timpani roll and cymbal swell into a tritone brass stab, strings grinding on a minor second, a growl beneath", max_secs=4.6)
def hellhound_round_start():
    m = Mix(4.4, 3)
    m.add(growl(4.0, m.rng), 0.0, 0.5)
    m.add(timpani_roll(hz("D2"), 1.55, 0.2, 1.0, rate=16, rng=m.rng), 0.0, 1.0)
    m.add(cymbal_swell(1.6, m.rng), 0.0, 0.8)
    m.chord(strings, ["D2", "Eb2", "D3", "Eb3"], 0.0, 1.0, held=3.0, vel=0.9, tremolo=13, swell=0.8, a=0.2)
    m.add(bass_drum(1.0, m.rng), 1.6, 1.0)
    m.add(timpani(hz("D2"), 1.0, decay=1.8, rng=m.rng), 1.6)
    m.chord(brass, ["D2", "Ab2", "D3", "Ab3", "Eb4"], 1.6, 1.1, held=1.3, vel=1.0, spread=0.7)
    m.chord(brass, ["D3", "Ab3"], 3.0, 0.5, held=0.4, vel=0.6, muted=True)
    return m


@piece("twist_round_start", "Twist Round start: strings sliding up a tritone in tremolo, into a diminished brass stab", max_secs=4.0)
def twist_round_start():
    m = Mix(3.4, 4)
    m.chord(strings, ["D3", "Ab3"], 0.0, 0.9, held=1.3, vel=0.8, tremolo=12, glide=6, glide_t=1.2, swell=0.7, a=0.08)
    m.chord(strings, ["D2"], 0.0, 0.7, held=1.3, vel=0.7, swell=0.7)
    m.add(cymbal_swell(1.3, m.rng), 0.0, 0.6)
    m.add(timpani(hz("B1"), 1.0, rng=m.rng), 1.3)
    m.chord(brass, ["B2", "D3", "F3", "Ab3", "B3"], 1.3, 1.0, held=0.9, vel=1.0, spread=0.6)
    m.chord(strings, ["B1", "F2"], 1.3, 0.8, held=1.2, vel=0.8, a=0.02)
    return m


@piece("boss_warning", "Boss warning: three low tritone brass hits on the drum, then a climbing brass call over string tremolo", max_secs=4.6)
def boss_warning():
    m = Mix(4.4, 5)
    for k, at in enumerate((0.0, 0.55, 1.1)):
        m.add(bass_drum(0.9, m.rng), at, 0.9)
        m.add(timpani(hz("D2"), 0.8 + 0.1 * k, rng=m.rng), at, 0.8)
        m.chord(brass, ["D2", "Ab2", "D3"], at, 0.9, held=0.22, vel=0.9 + 0.05 * k)
    m.chord(strings, ["D3", "D4"], 1.6, 0.8, held=2.0, vel=0.8, tremolo=14, a=0.05)
    m.add(brass(hz("D3"), 0.5, vel=0.9, rng=m.rng), 1.65, 0.9, -0.2)
    m.add(brass(hz("Eb3"), 0.5, vel=0.95, rng=m.rng), 2.2, 0.9, 0.0)
    m.add(brass(hz("Ab3"), 1.0, vel=1.0, rng=m.rng), 2.75, 1.0, 0.2)
    m.chord(brass, ["D2", "Ab2"], 2.75, 0.8, held=1.0, vel=0.9)
    m.add(timpani_roll(hz("D2"), 1.0, 0.3, 0.9, rng=m.rng), 2.75, 0.7)
    return m


@piece("game_over", "Game Over: a low string lament D-C-Bb-A over a D pedal, horns on the last D minor chord, the tape running down", tape_stop=(4.0, 1.8), max_secs=6.0)
def game_over():
    m = Mix(6.2, 6)
    m.add(timpani(hz("D2"), 0.7, decay=1.6, rng=m.rng), 0.0)
    m.chord(strings, ["D2"], 0.0, 0.9, held=5.0, vel=0.8, a=0.3)
    line = [("D3", 0.0, 0.95), ("C3", 1.0, 0.75), ("Bb2", 1.8, 0.75), ("A2", 2.6, 1.0)]
    for note, at, held in line:
        m.add(strings(hz(note), held, vel=0.85, a=0.15, rng=m.rng), at, 1.0, 0.25)
        m.add(strings(hz(note) * 2, held, vel=0.5, a=0.15, rng=m.rng), at, 0.6, -0.25)
    m.chord(brass, ["G2", "Bb2", "D3"], 1.8, 0.45, held=0.75, vel=0.5, bright=0.5, a=0.15)
    m.add(timpani(hz("D2"), 0.6, decay=2.0, rng=m.rng), 3.6)
    m.chord(brass, ["D2", "A2", "D3", "F3"], 3.6, 0.8, held=1.8, vel=0.6, bright=0.5, a=0.2)
    m.chord(strings, ["A2", "D3", "F3"], 3.6, 0.6, held=1.8, vel=0.6, a=0.3)
    return m


@piece("gauntlet_win", "Gauntlet win: a snare roll, a bugle-like brass call on A, then a worn D major (the Picardy turn) with timpani", max_secs=5.2)
def gauntlet_win():
    m = Mix(5.0, 7)
    m.add(snare_roll(0.9, 0.2, 0.9, m.rng), 0.0, 0.9)
    m.chord(strings, ["D2", "A2"], 0.0, 0.6, held=0.9, vel=0.6, swell=0.8)
    call = [("A3", 0.9, 0.12), ("A3", 1.05, 0.12), ("A3", 1.2, 0.12), ("D4", 1.35, 0.35), ("F#4", 1.75, 0.3), ("A4", 2.1, 1.3)]
    for note, at, held in call:
        m.add(brass(hz(note), held, vel=0.95, rng=m.rng, ens=(-4, 0, 5)), at, 0.9, 0.0)
    m.add(timpani(hz("D2"), 0.9, rng=m.rng), 1.35)
    m.add(timpani(hz("A1"), 0.7, rng=m.rng), 1.75)
    m.add(bass_drum(1.0, m.rng), 2.1)
    m.add(timpani(hz("D2"), 1.0, decay=1.8, rng=m.rng), 2.1)
    m.chord(brass, ["D3", "F#3", "A3", "D4"], 2.1, 0.9, held=1.7, vel=0.9)
    m.chord(strings, ["D2", "A2", "D3", "F#3"], 2.1, 0.8, held=1.9, vel=0.85, a=0.05)
    m.add(snare_roll(1.2, 0.5, 0.2, m.rng), 2.1, 0.6)
    return m


@piece("set_bonus_tier_two", "Set Bonus tier two: one short brass fifth on D with a celesta D")
def set_bonus_tier_two():
    m = Mix(1.3, 8)
    m.chord(brass, ["D3", "A3"], 0.0, 0.9, held=0.35, vel=0.85)
    m.add(timpani(hz("D2"), 0.6, decay=0.8, rng=m.rng), 0.0, 0.8)
    m.add(tine(hz("D5"), 0.8, decay=0.6, rng=m.rng, partials=CELESTA), 0.0, 0.7, 0.3)
    return m


@piece("set_bonus_tier_four", "Set Bonus tier four: two brass stabs rising D5th to F5th, celesta above")
def set_bonus_tier_four():
    m = Mix(1.6, 9)
    m.chord(brass, ["D3", "A3"], 0.0, 0.85, held=0.14, vel=0.85)
    m.add(timpani(hz("D2"), 0.6, decay=0.8, rng=m.rng), 0.0, 0.7)
    m.chord(brass, ["F3", "C4"], 0.25, 0.95, held=0.45, vel=0.95)
    m.add(timpani(hz("F2"), 0.75, decay=0.9, rng=m.rng), 0.25, 0.8)
    m.add(tine(hz("F5"), 0.8, decay=0.6, rng=m.rng, partials=CELESTA), 0.25, 0.7, 0.3)
    return m


@piece("set_bonus_tier_six", "Set Bonus tier six: three rising stabs D, F, A into a held A major chord with timpani and celesta", max_secs=3.0)
def set_bonus_tier_six():
    m = Mix(2.2, 10)
    for k, (notes, tim) in enumerate(((["D3", "A3"], "D2"), (["F3", "C4"], "F2"), (["A3", "C#4", "E4"], "A1"))):
        at = 0.22 * k
        held = 0.12 if k < 2 else 0.95
        m.chord(brass, notes, at, 0.85 + 0.08 * k, held=held, vel=0.85 + 0.07 * k)
        m.add(timpani(hz(tim), 0.6 + 0.15 * k, decay=0.9 + 0.3 * k, rng=m.rng), at, 0.8)
    m.chord(strings, ["A2", "E3"], 0.44, 0.7, held=0.95, vel=0.8, a=0.03)
    for k, note in enumerate(("A5", "C#6", "E6")):
        m.add(tine(hz(note), 0.7, decay=0.7, rng=m.rng, partials=CELESTA), 0.44 + 0.08 * k, 0.6, 0.35)
    return m


@piece("commendation", "Commendation: a short bugle-style call on a D major arpeggio, with a bell", max_secs=2.8)
def commendation():
    m = Mix(2.4, 11)
    call = [("A3", 0.0, 0.1), ("A3", 0.13, 0.1), ("D4", 0.26, 0.22), ("F#4", 0.52, 0.22), ("A4", 0.78, 0.8)]
    for note, at, held in call:
        m.add(brass(hz(note), held, vel=0.9, rng=m.rng, ens=(-3, 0, 4), vib=5), at, 0.9, -0.1)
    m.chord(brass, ["D3", "A3"], 0.78, 0.5, held=0.8, vel=0.6)
    m.add(tine(hz("D5"), 0.8, decay=1.2, rng=m.rng, partials=BELL), 0.78, 0.6, 0.3)
    return m


@piece("challenge_met", "Challenge met: a brass fourth up, A to D, with a bell on the D")
def challenge_met():
    m = Mix(1.5, 12)
    m.chord(brass, ["A3", "E3"], 0.0, 0.8, held=0.13, vel=0.85)
    m.chord(brass, ["D4", "A3", "F#3"], 0.18, 0.9, held=0.55, vel=0.9)
    m.add(timpani(hz("D2"), 0.6, decay=0.8, rng=m.rng), 0.18, 0.7)
    m.add(tine(hz("D5"), 0.7, decay=0.9, rng=m.rng, partials=BELL), 0.18, 0.5, 0.25)
    return m


@piece("challenge_breach", "Challenge broken: muted low brass falling a minor second Eb to D, over a low D")
def challenge_breach():
    m = Mix(1.6, 13)
    m.chord(brass, ["Eb3", "A3"], 0.0, 0.9, held=0.2, vel=0.85, muted=True)
    m.chord(brass, ["D3", "Ab3"], 0.27, 1.0, held=0.6, vel=0.8, muted=True)
    m.chord(strings, ["D2"], 0.27, 0.8, held=0.6, vel=0.8, a=0.02)
    m.add(timpani(hz("D2"), 0.7, decay=0.9, rng=m.rng), 0.27, 0.8)
    return m


@piece("duo_taken", "Duo taken: two horns from either side meeting on one chord, F major over D")
def duo_taken():
    m = Mix(1.9, 14)
    m.add(brass(hz("D4"), 0.16, vel=0.85, rng=m.rng), 0.0, 0.8, -0.6)
    m.add(brass(hz("A3"), 0.16, vel=0.85, rng=m.rng), 0.12, 0.8, 0.6)
    m.chord(brass, ["D3", "F3", "A3", "C4"], 0.3, 0.9, held=0.8, vel=0.9, spread=0.6)
    m.add(timpani(hz("D2"), 0.8, decay=1.0, rng=m.rng), 0.3, 0.8)
    m.chord(strings, ["D2", "A2"], 0.3, 0.6, held=0.8, vel=0.7, a=0.03)
    return m


@piece("boon_offer", "Boon offer: a soft string swell on F major seven, a celesta climbing through it", max_secs=2.8)
def boon_offer():
    m = Mix(2.6, 15)
    m.chord(strings, ["F2", "C3", "A3", "E4"], 0.0, 0.9, held=1.5, vel=0.7, a=0.35, swell=0.4)
    for k, note in enumerate(("F5", "A5", "C6", "E6", "A6")):
        m.add(tine(hz(note), 0.6, decay=0.8, rng=m.rng, partials=CELESTA), 0.15 + 0.13 * k, 0.6, -0.3 + 0.15 * k)
    return m


@piece("pact_offer", "Pact offer: low strings swelling on D, A and Eb, muted brass on Ab, a dull bell out of tune", max_secs=3.2)
def pact_offer():
    m = Mix(2.9, 16)
    m.chord(strings, ["D2", "A2", "Eb3"], 0.0, 1.0, held=1.8, vel=0.85, a=0.4, swell=0.6, bright=3.5)
    m.add(brass(hz("Ab2"), 1.2, vel=0.7, muted=True, a=0.3, rng=m.rng), 0.5, 0.7, 0.2)
    m.add(tine(hz("D4") * 2 ** (-0.3 / 12), 0.8, decay=1.4, rng=m.rng, partials=BELL), 0.0, 0.5, -0.3)
    m.add(timpani(hz("D2"), 0.4, decay=1.6, rng=m.rng), 0.0, 0.6)
    return m


@piece("box_spin", "The Box spin tune: a music-box waltz in D minor, two bars, ending as the spin does (2.85 s)", wow=2.0, max_secs=3.1)
def box_spin():
    m = Mix(3.0, 17)
    tune = ["D5", "F5", "A5", "G5", "F5", "E5", "F5", "D5", "A4", "Bb4", "C5", "A4", "D5", "E5", "F5", "E5", "C#5", "D5"]
    step = 0.145
    for k, note in enumerate(tune):
        m.add(tine(hz(note), 0.9 if k % 3 == 0 else 0.7, decay=0.5 if k < len(tune) - 1 else 0.6, rng=m.rng), k * step, 1.0)
    for k, note in enumerate(("D4", "A3", "Bb3", "A3", "D4", "A3")):
        m.add(tine(hz(note), 0.6, decay=0.6, rng=m.rng), k * step * 3, 0.8)
    return m


@piece("box_teddy", "The Teddy: a music box winding down on a falling minor line, the tape dragging to a stop", tape_stop=(1.2, 1.6), wow=2.0, max_secs=3.2)
def box_teddy():
    m = Mix(2.8, 18)
    tune = [("A5", 0.0), ("G5", 0.26), ("F5", 0.55), ("E5", 0.9), ("D5", 1.3), ("C#5", 1.75)]
    for note, at in tune:
        m.add(tine(hz(note), 0.8, decay=0.9, rng=m.rng), at, 1.0)
    for note, at in (("D4", 0.0), ("Bb3", 0.55), ("A3", 1.3)):
        m.add(tine(hz(note), 0.5, decay=0.9, rng=m.rng), at, 0.8)
    return m


# The Perk jingles: period radio adverts, one per machine, played through the machine's speaker.


@piece("perk_jingle_juggernog", "Juggernog: a heavy oompah march in D minor, tuba and brass, 107 bpm", radio=True)
def perk_jingle_juggernog():
    m = Mix(5.2, 19)
    beat = 0.56
    bass = ["D2", "A1", "D2", "A1", "Bb1", "F1", "A1", "E1"]
    chords = [["D3", "F3", "A3"], ["D3", "F3", "A3"], ["D3", "F3", "A3"], ["C#3", "E3", "A3"], ["D3", "F3", "Bb3"], ["D3", "F3", "Bb3"], ["C#3", "E3", "A3"], ["C#3", "E3", "G3"]]
    for k in range(8):
        at = k * beat
        m.add(brass(hz(bass[k]), 0.25, vel=0.9, bright=0.5, rng=m.rng), at, 1.1)
        m.add(bass_drum(0.6, m.rng), at, 0.5)
        m.chord(brass, chords[k], at + beat / 2, 0.45, held=0.12, vel=0.7, bright=0.6)
    tune = [("A4", 0, 1), ("A4", 1, 0.5), ("Bb4", 1.5, 0.5), ("A4", 2, 1), ("F4", 3, 1), ("G4", 4, 1), ("F4", 5, 0.5), ("E4", 5.5, 0.5), ("F4", 6, 0.5), ("E4", 6.5, 0.5), ("D4", 7, 1.2)]
    for note, b, length in tune:
        m.add(brass(hz(note), length * beat * 0.9, vel=0.95, rng=m.rng, ens=(-5, 4)), b * beat, 0.8)
    m.chord(brass, ["D2", "A2", "D3", "F3", "A3"], 8 * beat, 0.8, held=0.3, vel=0.95)
    m.add(timpani(hz("D2"), 0.9, rng=m.rng), 8 * beat)
    return m


@piece("perk_jingle_speed_cola", "Speed Cola: a quick swing on an upright piano and muted trumpet in G minor, 168 bpm", radio=True)
def perk_jingle_speed_cola():
    m = Mix(4.6, 20)
    beat = 60 / 168
    roots = ["G2", "D3", "G2", "D3", "C3", "G2", "D3", "A2", "G2", "D3", "C3", "D3"]
    comp = [["Bb3", "D4", "G4"], ["Bb3", "D4", "G4"], ["C4", "Eb4", "G4"], ["A3", "D4", "F#4"]]
    for k in range(12):
        at = k * beat
        m.add(piano(hz(roots[k]), beat * 0.8, 0.9, m.rng), at, 0.9)
        m.chord(piano, comp[(k // 3) % 4], at + beat * 0.62, 0.5, held=beat * 0.3, vel=0.7)
        m.add(snare(0.3, m.rng, decay=0.04), at + beat * 0.62, 0.5)
    tune = ["D5", "Bb4", "G4", "Bb4", "D5", "F5", "Eb5", "D5", "C5", "D5", "C5", "Bb4", "A4", "Bb4", "C5", "D5", "G5"]
    swing = [0, 0.66, 1, 1.66, 2, 2.66, 3, 3.66, 4, 4.66, 5, 5.66, 6, 6.66, 7, 7.66, 8]
    for k, (note, b) in enumerate(zip(tune, swing)):
        held = beat * (2.5 if k == len(tune) - 1 else 0.45)
        m.add(brass(hz(note), held, vel=0.85, muted=True, rng=m.rng, ens=(-3, 3), scoop=-20, vib=0), b * beat, 1.0)
    m.chord(piano, ["G2", "G3", "Bb3", "D4", "G4"], 8 * beat, 0.8, held=beat * 3, vel=0.9)
    return m


@piece("perk_jingle_double_tap", "Double Tap: a field-drum march in Bb minor, the brass hammering a two-note tap, 124 bpm", radio=True)
def perk_jingle_double_tap():
    m = Mix(5.0, 21)
    beat = 60 / 124
    for k in range(8):
        at = k * beat
        m.add(snare(0.8 if k % 2 == 0 else 0.6, m.rng), at, 0.8)
        m.add(snare(0.45, m.rng), at + beat * 0.75, 0.6)
        if k % 2 == 0:
            m.add(bass_drum(0.8, m.rng), at, 0.7)
    taps = [("F4", 0), ("F4", 0.5), ("Bb4", 1), ("F4", 2), ("F4", 2.5), ("Db5", 3), ("C5", 4), ("C5", 4.5), ("Bb4", 5), ("Ab4", 5.5), ("Bb4", 6)]
    for note, b in taps:
        held = beat * (1.6 if b == 6 else 0.3)
        m.add(brass(hz(note), held, vel=0.95, rng=m.rng, ens=(-5, 0, 5)), b * beat, 0.8)
    for b, notes in ((0, ["Bb2", "F3"]), (2, ["Bb2", "F3"]), (4, ["Gb2", "Db3"]), (6, ["Bb2", "F3", "Bb3", "Db4"])):
        m.chord(brass, notes, b * beat, 0.6, held=beat * (1.6 if b == 6 else 0.9), vel=0.8, bright=0.6)
    m.add(timpani(hz("Bb1"), 0.9, rng=m.rng), 6 * beat)
    return m


@piece("perk_jingle_quick_revive", "Quick Revive: a gentle waltz in F, celesta over strings and a soft upright", radio=True, max_secs=6.0)
def perk_jingle_quick_revive():
    m = Mix(5.4, 22)
    beat = 0.42
    bars = [("F2", ["A3", "C4"]), ("C3", ["G3", "Bb3"]), ("D2", ["F3", "A3"]), ("C2", ["E3", "G3"])]
    for k, (root, comp) in enumerate(bars):
        at = k * 3 * beat
        m.add(piano(hz(root), beat * 0.9, 0.8, m.rng), at, 0.8)
        m.chord(piano, comp, at + beat, 0.45, held=beat * 0.6, vel=0.6)
        m.chord(piano, comp, at + 2 * beat, 0.45, held=beat * 0.6, vel=0.6)
    m.chord(strings, ["F2", "C3", "A3"], 0.0, 0.5, held=2.4, vel=0.6, a=0.4)
    m.chord(strings, ["D2", "A2", "F3"], 2.5, 0.5, held=1.2, vel=0.6, a=0.3)
    tune = [("C5", 0), ("A4", 1), ("C5", 2), ("Bb4", 3), ("G4", 4), ("E4", 5), ("A4", 6), ("F4", 7), ("D4", 8), ("G4", 9), ("E4", 10.5), ("F4", 12)]
    for note, b in tune:
        m.add(tine(hz(note), 0.85, decay=0.9, rng=m.rng, partials=CELESTA), b * beat, 1.0)
    m.chord(strings, ["F2", "C3", "A3", "C4"], 12 * beat, 0.6, held=0.5, vel=0.6, a=0.08)
    m.add(piano(hz("F2"), 0.6, 0.8, m.rng), 12 * beat, 0.8)
    return m


# --- render ----------------------------------------------------------------------------------


def impulse(secs, decay, predelay, wet, seed, radio):
    """A stereo room: the direct sound, then decorrelated decaying noise, darker as it fades."""
    rng = random.Random(seed)
    n = int(secs * SR)
    pre = int(predelay * SR)
    chans = []
    for c in range(2):
        tail = noise(rng, n)
        tail = lowpass(tail, 3200 if radio else 5000, poles=1)
        ch = [0.0] * n
        ch[0] = 1.0
        for i in range(pre, n):
            t = (i - pre) / SR
            ch[i] = tail[i] * wet * math.exp(-6.9 * t / decay)
        chans.append(ch)
    out = array.array("f", [0.0] * (2 * n))
    out[0::2] = array.array("f", chans[0])
    out[1::2] = array.array("f", chans[1])
    return out


def interleave(L, R):
    out = array.array("f", [0.0] * (2 * len(L)))
    out[0::2] = array.array("f", L)
    out[1::2] = array.array("f", R)
    return out


def tape(L, R, rng, wow=1.0, stop=None, sat=1.2, hiss_db=-52.0):
    """Wow and flutter (a slowly varying playback rate), the tape running down from `stop[0]`
    seconds over `stop[1]` seconds, a soft saturation and a little hiss. Returns new L and R."""
    n = len(L)
    peak = max(max(abs(x) for x in L), max(abs(x) for x in R), 1e-9)
    L = [x * 0.8 / peak for x in L]
    R = [x * 0.8 / peak for x in R]
    p_wow, p_flut, p_drift = rng.random() * 6, rng.random() * 6, rng.random() * 6
    pos = [0.0] * n
    p = 0.0
    for i in range(n):
        t = i / SR
        rate = 1 + wow * (0.0014 * math.sin(2 * math.pi * 0.55 * t + p_wow) + 0.0006 * math.sin(2 * math.pi * 0.13 * t + p_drift) + 0.00035 * math.sin(2 * math.pi * 7.3 * t + p_flut))
        if stop and t > stop[0]:
            k = min(1.0, (t - stop[0]) / stop[1])
            rate *= 1 - 0.7 * k * k
        pos[i] = p
        p += rate
    norm = math.tanh(sat)
    hiss = lowpass(noise(rng, n), 6000, poles=1)
    hl = 10 ** (hiss_db / 20) * 1.7
    out = []
    for src in (L, R):
        ch = [0.0] * n
        for i in range(n):
            q = pos[i]
            j = int(q)
            if j + 1 >= n:
                break
            s = src[j] + (src[j + 1] - src[j]) * (q - j)
            ch[i] = math.tanh(sat * s) / norm
        out.append(ch)
    # Hiss under the tail too, shaped so it doesn't start or stop with a click.
    fade = int(0.08 * SR)
    for ch in out:
        for i in range(n):
            g = min(1.0, i / fade, (n - 1 - i) / fade)
            ch[i] = ch[i] * (min(1.0, (n - 1 - i) / fade)) + hiss[i] * hl * g
    return out


def trim(L, R, floor_db=-28.0, cap=None):
    """Cut the tail once its loudness (RMS over 20 ms) has fallen `floor_db` under the loudest
    moment, or at `cap` seconds, with a fade, so a stinger doesn't trail off into hiss."""
    win = int(0.02 * SR)
    rms = []
    for b in range(0, len(L), win):
        seg = L[b : b + win] + R[b : b + win]
        rms.append(math.sqrt(sum(x * x for x in seg) / max(1, len(seg))))
    floor = max(rms) * 10 ** (floor_db / 20)
    last = max(i for i, r in enumerate(rms) if r >= floor)
    end = min(len(L), (last + 1) * win)
    fade = int(0.25 * SR)
    if cap is not None and end > int(cap * SR):
        end = int(cap * SR)
        fade = int(0.4 * SR)
    L, R = L[:end], R[:end]
    for i in range(max(0, end - fade), end):
        g = ((end - i) / fade) ** 2
        L[i] *= g
        R[i] *= g
    return L, R


def render(slot, workdir, keep=None):
    fn, desc, fx = PIECES[slot]
    rng = random.Random(hash_seed(slot))
    mix = fn()
    radio = fx.get("radio", False)
    dry = workdir / f"{slot}.dry.f32"
    ir = workdir / f"{slot}.ir.f32"
    wet = workdir / f"{slot}.wet.f32"
    mix.pad(mix.n + int((0.6 if radio else 1.5) * SR))
    dry.write_bytes(interleave(mix.L, mix.R).tobytes())
    if radio:
        ir.write_bytes(impulse(0.5, 0.35, 0.008, 0.008, hash_seed(slot) + 1, True).tobytes())
        band = "highpass=f=320,highpass=f=320,lowpass=f=3600,lowpass=f=3600,equalizer=f=1400:t=q:w=1.2:g=4"
    else:
        ir.write_bytes(impulse(1.4, 1.1, 0.018, 0.009, hash_seed(slot) + 1, False).tobytes())
        band = "highpass=f=55,lowpass=f=10000,equalizer=f=120:t=q:w=1:g=-2"
    raw = ["-f", "f32le", "-ar", str(SR), "-ac", "2"]
    cmd = ["ffmpeg", "-hide_banner", "-nostats", "-loglevel", "error", "-y", *raw, "-i", str(dry), *raw, "-i", str(ir),
           "-filter_complex", f"[0:a][1:a]afir=irnorm=-1:gtype=none:dry=1:wet=1,{band}[o]", "-map", "[o]", *raw, str(wet)]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        sys.exit(f"{slot}: ffmpeg failed\n{result.stderr}")
    data = array.array("f")
    data.frombytes(wet.read_bytes())
    L, R = list(data[0::2]), list(data[1::2])
    L, R = tape(L, R, rng, wow=fx.get("wow", 1.0), stop=fx.get("tape_stop"), sat=2.0 if radio else 1.2, hiss_db=-46.0 if radio else -52.0)
    L, R = trim(L, R, cap=fx.get("max_secs"))
    final = workdir / f"{slot}.wav"
    with wave.open(str(final), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(SR)
        pcm = array.array("h", [0] * (2 * len(L)))
        pcm[0::2] = array.array("h", (int(max(-1.0, min(1.0, x)) * 32000) for x in L))
        pcm[1::2] = array.array("h", (int(max(-1.0, min(1.0, x)) * 32000) for x in R))
        w.writeframes(pcm.tobytes())
    if keep:
        shutil.copy(final, keep / final.name)
    return final, desc, radio


def hash_seed(slot):
    """A stable seed from the slot's name (Python's own hash() is salted per run)."""
    return sum((i + 1) * ord(c) for i, c in enumerate(slot))


def existing(slot):
    """How the slot's files are laid out now: ('plain', None) or ('numbered', 1)."""
    if (AUDIO / f"{slot}.ogg").exists():
        return ["--plain"]
    numbered = sorted(AUDIO.glob(f"{slot}_[0-9]*.ogg"))
    if numbered:
        # One composed take replaces every recorded variant.
        for extra in numbered[1:]:
            extra.unlink()
        return ["--variant", "1"]
    return ["--plain"]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("slots", nargs="*", help="slots to compose (default: all)")
    parser.add_argument("--list", action="store_true")
    parser.add_argument("--keep", type=pathlib.Path, help="also copy each rendered WAV here")
    parser.add_argument("--render-only", action="store_true", help="render the WAVs (needs --keep) without touching assets/")
    args = parser.parse_args()
    if args.list:
        for slot, (_, desc, _) in PIECES.items():
            print(f"{slot:28} {desc}")
        return
    slots = args.slots or list(PIECES)
    unknown = [s for s in slots if s not in PIECES]
    if unknown:
        sys.exit(f"no piece for {', '.join(unknown)}; see --list")
    if args.keep:
        args.keep.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        for slot in slots:
            final, desc, radio = render(slot, pathlib.Path(tmp), args.keep)
            if args.render_only:
                print(f"rendered {slot}")
                continue
            notes = (
                f"{desc}. An original piece composed and synthesised for this project and released as CC0 "
                f"(no samples); rendered by tools/audio/compose.py ({'radio-speaker band' if radio else 'room and tape'}), "
                "normalised to -20 LUFS (music) and encoded OGG q4 by tools/audio/process.py"
            )
            cmd = [sys.executable, str(HERE / "process.py"), str(final), slot, "--group", "music", "--source", SOURCE,
                   "--licence", LICENCE, "--name", f"{slot}, composed for this project", "--notes", notes, *existing(slot)]
            result = subprocess.run(cmd, capture_output=True, text=True)
            if result.returncode != 0:
                sys.exit(f"{slot}: process.py failed\n{result.stdout}{result.stderr}")
            print(result.stdout.strip())


main()
