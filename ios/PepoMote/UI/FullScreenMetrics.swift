import CoreGraphics
import Foundation

/// Rectángulo (x, y, ancho, alto) en las unidades del contenedor.
struct FitRect: Equatable {
    var x: CGFloat
    var y: CGFloat
    var w: CGFloat
    var h: CGFloat
}

/// Geometría de la «pantalla del GamePad a pantalla completa» (idéntica en
/// Android, `FullScreenMetrics.kt`, con los mismos vectores de test): la
/// imagen ajustada a su proporción real y centrada sobre fondo negro, el
/// táctil solo sobre ella, y el tamaño que se pide al PC.
enum FullScreenMetrics {
    static let nativeW = 854
    static let nativeH = 480

    /// Imagen `image` (sin fotograma aún: 16:9) ajustada y centrada en
    /// `container`; vacío si el contenedor no tiene tamaño.
    static func fitRect(container: CGSize, image: CGSize?) -> FitRect {
        if container.width <= 0 || container.height <= 0 { return FitRect(x: 0, y: 0, w: 0, h: 0) }
        let known = (image?.width ?? 0) > 0 && (image?.height ?? 0) > 0
        let iw: CGFloat = known ? image!.width : 16
        let ih: CGFloat = known ? image!.height : 9
        let scale = Swift.min(container.width / iw, container.height / ih)
        let w = iw * scale
        let h = ih * scale
        return FitRect(x: (container.width - w) / 2, y: (container.height - h) / 2, w: w, h: h)
    }

    /// ¿El punto cae sobre la imagen? Fuera (bandas negras) no es un toque.
    static func contains(_ r: FitRect, _ p: CGPoint) -> Bool {
        r.w > 0 && r.h > 0 && p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h
    }

    /// Fracción 0..65535 del toque dentro de `r`, recortada a sus bordes.
    static func fraction(_ r: FitRect, _ p: CGPoint) -> (Int, Int) {
        if r.w <= 0 || r.h <= 0 { return (0, 0) }
        let fx = Swift.min(Swift.max((p.x - r.x) / r.w, 0), 1)
        let fy = Swift.min(Swift.max((p.y - r.y) / r.h, 0), 1)
        return (Int((fx * 65535).rounded()), Int((fy * 65535).rounded()))
    }

    /// Tamaño pedido al PC en pantalla completa: el del área en píxeles, como
    /// mucho el nativo del GamePad (854×480); el PC conserva la proporción.
    static func streamRequest(containerPxW: Int, containerPxH: Int) -> (Int, Int) {
        if containerPxW < 1 || containerPxH < 1 { return (nativeW, nativeH) }
        return (Swift.min(nativeW, containerPxW), Swift.min(nativeH, containerPxH))
    }
}
