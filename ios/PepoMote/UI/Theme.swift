import SwiftUI
import UIKit

/// Paleta de la app: «PepoWhite» (inspiración Wii, cero assets ajenos) y
/// «PepoDark» (el mismo azul y los mismos acentos sobre grafito). Colores
/// dinámicos: siguen al tema del sistema.
enum Pepo {
    private static func dyn(_ light: UInt32, _ dark: UInt32) -> Color {
        Color(UIColor { tc in
            tc.userInterfaceStyle == .dark ? UIColor(rgb: dark) : UIColor(rgb: light)
        })
    }

    static let background = dyn(0xF4F6F7, 0x14181C)
    static let card = dyn(0xFFFFFF, 0x1F252B)
    static let cardBorder = dyn(0xE3E8EB, 0x2C343B)
    static let text = dyn(0x3B4750, 0xE6EBEF)
    static let textDim = dyn(0x7C8A94, 0x8E9BA6)
    static let blue = Color(UIColor(rgb: 0x3FA9F5))
    static let blueHover = Color(UIColor(rgb: 0x2B98E8))
    static let glow = dyn(0xAEE2FF, 0x1D3D52)
    static let ok = Color(UIColor(rgb: 0x7BC94C))
    static let warn = Color(UIColor(rgb: 0xF5A83C))
    static let error = Color(UIColor(rgb: 0xE85C5C))
    /// Texto sobre el azul (o cualquier acento): blanco en los dos temas.
    static let onAccent = Color.white
}

extension UIColor {
    convenience init(rgb: UInt32) {
        self.init(
            red: CGFloat((rgb >> 16) & 0xFF) / 255,
            green: CGFloat((rgb >> 8) & 0xFF) / 255,
            blue: CGFloat(rgb & 0xFF) / 255,
            alpha: 1
        )
    }
}

/// Tipografía: Nunito (variable, con sus instancias Regular/Bold/Black) y, si
/// la fuente no cargara, la redondeada del sistema, que se le parece.
enum PepoFont {
    enum Weight {
        case regular, bold, black
    }

    private static let nunitoAvailable: Bool = UIFont(name: "Nunito-Bold", size: 12) != nil

    static func nunito(_ size: CGFloat, _ weight: Weight) -> Font {
        if nunitoAvailable {
            switch weight {
            case .regular: return .custom("Nunito-Regular", size: size)
            case .bold: return .custom("Nunito-Bold", size: size)
            case .black: return .custom("Nunito-Black", size: size)
            }
        }
        switch weight {
        case .regular: return .system(size: size, weight: .regular, design: .rounded)
        case .bold: return .system(size: size, weight: .bold, design: .rounded)
        case .black: return .system(size: size, weight: .black, design: .rounded)
        }
    }

    static func display() -> Font { nunito(40, .black) }
    static func headline() -> Font { nunito(26, .black) }
    static func titleLarge() -> Font { nunito(20, .bold) }
    static func titleMedium() -> Font { nunito(17, .bold) }
    static func bodyLarge() -> Font { nunito(16, .regular) }
    static func bodyMedium() -> Font { nunito(14, .regular) }
    static func labelLarge() -> Font { nunito(15, .bold) }
}

extension Text {
    /// bodyMedium: 14, texto tenue.
    func pepoBody() -> Text { font(PepoFont.bodyMedium()).foregroundColor(Pepo.textDim) }
    /// titleMedium: 17 negrita, texto.
    func pepoTitle() -> Text { font(PepoFont.titleMedium()).foregroundColor(Pepo.text) }
}

/// Tarjeta: fondo, borde fino y esquinas redondeadas (shape medium).
struct CardStyle: ViewModifier {
    var border: Color = Pepo.cardBorder
    var radius: CGFloat = 24

    func body(content: Content) -> some View {
        content
            .background(Pepo.card)
            .clipShape(RoundedRectangle(cornerRadius: radius, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: radius, style: .continuous).stroke(border, lineWidth: 1.5))
    }
}

extension View {
    func pepoCard(border: Color = Pepo.cardBorder, radius: CGFloat = 24) -> some View {
        modifier(CardStyle(border: border, radius: radius))
    }
}

/// Botón azul grande (Conectar, ¡Vamos!).
struct PrimaryButton: View {
    let title: String
    var height: CGFloat = 60
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(PepoFont.titleMedium())
                .foregroundColor(Pepo.onAccent)
                .frame(maxWidth: .infinity)
                .frame(height: height)
                .background(Pepo.blue)
                .clipShape(RoundedRectangle(cornerRadius: 24, style: .continuous))
        }
        .buttonStyle(.plain)
    }
}

/// Botón de texto plano (Salir, Volver…).
struct TextLink: View {
    let title: String
    var color: Color = Pepo.textDim
    var font: Font = PepoFont.bodyMedium()
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(font)
                .foregroundColor(color)
                .lineLimit(1)
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
        }
        .buttonStyle(.plain)
    }
}
