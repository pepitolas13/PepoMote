import CoreGraphics

/// Escala de la interfaz en pantallas grandes (iPad). El factor SOLO agranda:
/// cualquier iPhone queda exactamente como hasta ahora (factor 1) y un iPad
/// crece con la pantalla hasta `maxFactor` (que el mando vertical encoja en
/// un iPhone bajo es cosa aparte: RemoteMetrics). Cada pantalla pasa su base: la
/// anchura del iPhone más grande (16 Pro Max, 440 pt) o la anchura natural
/// de su trazado, y la altura de lo que escala descontando lo fijo (cabecera,
/// chips, selector), que no crece. Misma regla y mismos números que en
/// Android (UiScale.kt).
enum UiScale {
    static let maxFactor: CGFloat = 2
    /// GamePad apaisado: el iPhone más grande de lado.
    static let phoneLandscape = CGSize(width: 956, height: 440)
    /// Mando vertical y Nunchuk: 632 pt escalan (huecos mínimos 10+16+16+14+10,
    /// cruceta 168, fila 64, A 148, 1/2 52, multimedia 46, B 88) y 176 pt son
    /// fijos (10 + cabecera 44 + chips 42 + selector «En Cemu soy» 64 + 4 + 12).
    static let remoteBase = CGSize(width: 440, height: 632)
    static let remoteFixed: CGFloat = 176
    /// Mando apaisado (NES): anchura natural del trazado a escala 1
    /// (34+190 | 122 | 232+30 = 578) más dos huecos.
    static let landscapeBase = CGSize(width: 700, height: 440)

    /// max(1, min(ancho/base.ancho, (alto − fixed)/base.alto, max)).
    static func factor(_ size: CGSize, base: CGSize, fixed: CGFloat = 0, max: CGFloat = UiScale.maxFactor) -> CGFloat {
        guard size.width > 0, size.height > 0, base.width > 0, base.height > 0 else { return 1 }
        let f = Swift.min(size.width / base.width, (size.height - fixed) / base.height, max)
        return Swift.max(1, f)
    }
}
