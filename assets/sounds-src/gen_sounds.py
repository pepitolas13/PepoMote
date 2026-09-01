# Genera los sonidos UI de PepoMote — 100% sintetizados, cero samples ajenos.
# Python puro (wave + math), sin dependencias. Salida: WAV 44.1 kHz 16-bit mono
# directamente en android/app/src/main/res/raw/.
import math
import os
import struct
import wave

SR = 44100
OUT = os.path.join(os.path.dirname(__file__), "..", "..", "android", "app", "src", "main", "res", "raw")


def synth(notes, vol=0.45):
    """notes: lista de (freq_inicio, freq_fin, ms, decay). Sinusoide con
    barrido lineal y envolvente exponencial + ataque suave anti-click."""
    samples = []
    for f0, f1, ms, decay in notes:
        n = int(SR * ms / 1000)
        phase = 0.0
        for i in range(n):
            t = i / n
            f = f0 + (f1 - f0) * t
            phase += 2 * math.pi * f / SR
            env = math.exp(-decay * t) * min(1.0, i / (SR * 0.002))
            samples.append(vol * env * math.sin(phase))
    return samples


def write(name, samples):
    path = os.path.join(OUT, name)
    with wave.open(path, "w") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(b"".join(
            struct.pack("<h", int(max(-1.0, min(1.0, s)) * 32767)) for s in samples
        ))
    print(f"{name}: {len(samples) / SR * 1000:.0f} ms")


os.makedirs(OUT, exist_ok=True)

# blip: pulsación de botón (corto, agudo, con barrido arriba)
write("ui_blip.wav", synth([(1150, 1350, 45, 6.0)]))
# pop: botón A / selección (más cuerpo)
write("ui_pop.wav", synth([(680, 620, 70, 5.0)], vol=0.5))
# tick: recentrado (seco)
write("ui_tick.wav", synth([(1900, 1900, 30, 9.0)], vol=0.4))
# connect: arpegio ascendente E5-G5-C6
write("ui_connect.wav", synth([(659, 659, 80, 3.5), (784, 784, 80, 3.5), (1047, 1047, 140, 4.0)]))
# disconnect: inverso descendente
write("ui_disconnect.wav", synth([(1047, 1047, 80, 3.5), (784, 784, 80, 3.5), (659, 659, 140, 4.0)]))
