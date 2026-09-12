import Foundation

/// «Teclado» del modo Wii U: el teclado en pantalla de Cemu no acepta toques,
/// solo teclas del PC, así que el móvil manda texto por el canal de control
/// (`{"m":"text","text":"…"}`, PROTOCOL.md §3) y el receptor lo teclea. Aquí va
/// la lógica pura del diálogo (qué se manda, qué queda en el campo y si se
/// cierra) y la codificación del mensaje.
enum TextInput {
    /// Intro: acepta / OK en el teclado de Cemu.
    static let enter = "\n"

    /// Borra un carácter en Cemu (U+0008).
    static let backspace = "\u{8}"

    /// Resultado de un botón del diálogo.
    struct Action: Equatable {
        /// Lo que se manda al PC (nil = nada).
        let send: String?
        /// Contenido del campo después.
        let field: String
        /// Si el diálogo se cierra.
        let close: Bool
    }

    /// «Borrar»: un carácter menos en Cemu; el campo no cambia y el diálogo sigue.
    static func delete(_ field: String) -> Action { Action(send: backspace, field: field, close: false) }

    /// «Escribir»: manda el campo tal cual (nada si está vacío) y lo vacía; el diálogo sigue.
    static func write(_ field: String) -> Action { Action(send: field.isEmpty ? nil : field, field: "", close: false) }

    /// «Aceptar»: manda el campo + Intro (solo Intro con el campo vacío: confirma) y cierra.
    static func accept(_ field: String) -> Action { Action(send: field + enter, field: "", close: true) }

    /// «Cerrar»: no se manda nada.
    static func close(_ field: String) -> Action { Action(send: nil, field: field, close: true) }

    /// Línea `{"m":"text","text":"…"}` con el texto escapado como JSON.
    static func encode(_ text: String) -> String {
        "{\"m\":\"text\",\"text\":\"" + escape(text) + "\"}"
    }

    /// Escapado JSON de una cadena, el mismo que hace org.json en Android.
    static func escape(_ text: String) -> String {
        var out = ""
        out.reserveCapacity(text.count + 8)
        for scalar in text.unicodeScalars {
            switch scalar {
            case "\"", "\\", "/":
                out.append("\\")
                out.unicodeScalars.append(scalar)
            case "\t": out += "\\t"
            case "\u{8}": out += "\\b"
            case "\n": out += "\\n"
            case "\r": out += "\\r"
            case "\u{c}": out += "\\f"
            default:
                if scalar.value <= 0x1f {
                    out += String(format: "\\u%04x", scalar.value)
                } else {
                    out.unicodeScalars.append(scalar)
                }
            }
        }
        return out
    }
}
