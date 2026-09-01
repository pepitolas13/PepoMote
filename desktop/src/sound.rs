//! Chimes de conexión/desconexión — sintetizados con rodio, cero assets.

use rodio::source::{SineWave, Source};
use rodio::{OutputStream, Sink};
use std::time::Duration;

fn play_notes(notes: &'static [(f32, u64)]) {
    // Hilo propio: abrir el dispositivo de audio tarda; jamás en el hot path.
    std::thread::spawn(move || {
        let Ok((_stream, handle)) = OutputStream::try_default() else {
            return;
        };
        let Ok(sink) = Sink::try_new(&handle) else {
            return;
        };
        for &(freq, ms) in notes {
            sink.append(
                SineWave::new(freq)
                    .take_duration(Duration::from_millis(ms))
                    .amplify(0.18)
                    .fade_in(Duration::from_millis(4)),
            );
        }
        sink.sleep_until_end();
    });
}

pub fn connect_chime() {
    play_notes(&[(659.0, 90), (784.0, 90), (1046.5, 160)]);
}

pub fn disconnect_chime() {
    play_notes(&[(1046.5, 90), (784.0, 90), (659.0, 160)]);
}
