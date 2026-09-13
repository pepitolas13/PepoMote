import CoreGraphics

/// Escala de la interfaz en pantallas grandes (iPad). El factor SOLO agranda:
/// la base es el iPhone más grande (16 Pro Max, 440×956 pt), así cualquier
/// iPhone queda exactamente como hasta ahora (factor 1) y un iPad crece con
/// tope. Misma regla y mismos números que en Android (UiScale.kt).
enum UiScale {
    static let phonePortrait = CGSize(width: 440, height: 956)
    static let phoneLandscape = CGSize(width: 956, height: 440)

    /// max(1, min(ancho/base.ancho, alto/base.alto, max)).
    static func factor(_ size: CGSize, base: CGSize, max: CGFloat) -> CGFloat {
        guard size.width > 0, size.height > 0, base.width > 0, base.height > 0 else { return 1 }
        let f = Swift.min(size.width / base.width, size.height / base.height, max)
        return Swift.max(1, f)
    }

    /// Solo por ancho (GamePad apaisado): clamp(ancho/base, 1, max).
    static func factor(width: CGFloat, base: CGFloat, max: CGFloat) -> CGFloat {
        guard width > 0, base > 0 else { return 1 }
        return Swift.max(1, Swift.min(width / base, max))
    }
}
