import Foundation

/// Lo que el receptor sabe del juego cargado en RetroArch (mensaje `game`,
/// PROTOCOL.md §3): con `console` (un id de `RetroLayouts.consoleIds`, o nil
/// si no la conoce) el móvil elige la plantilla de mando, y `path` es la clave
/// para recordar la elegida a mano por juego. Solo cuenta en modo RetroArch;
/// se conserva al cambiar de modo y se pierde al desconectar.
struct RetroGame: Equatable {
    let console: String?
    let system: String
    let core: String
    let title: String
    let path: String

    /// `{"m":"game",...}` → ficha; nil cuando el receptor no tiene juego (todo vacío).
    static func parse(_ obj: [String: Any]) -> RetroGame? {
        let console = (obj["console"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        let game = RetroGame(
            console: console,
            system: obj["system"] as? String ?? "",
            core: obj["core"] as? String ?? "",
            title: obj["title"] as? String ?? "",
            path: obj["path"] as? String ?? ""
        )
        return (game.title.isEmpty && game.path.isEmpty && game.console == nil) ? nil : game
    }
}

/// Mando de consola elegido a mano en RetroArch: `global` («auto» o un id de
/// plantilla) y, por juego (ruta del contenido que anuncia el receptor), la
/// plantilla que se prefiere (tope `cap`, el más viejo fuera). Puro; se guarda
/// como JSON en las preferencias (mismo formato que Android).
struct RetroLayoutChoice: Equatable {
    struct Entry: Equatable {
        let path: String
        let id: String
    }

    static let auto = "auto"
    static let cap = 50

    var global: String = RetroLayoutChoice.auto
    var byPath: [Entry] = []

    /// La elegida a mano para ese juego (o la global), o nil = automático.
    func choiceFor(_ path: String?) -> String? {
        if let path, let e = byPath.first(where: { $0.path == path }) { return e.id }
        return global == Self.auto ? nil : global
    }

    /// Elegir `id` (nil = automático) para el juego `path` (nil = sin juego
    /// cargado: vale para todos hasta que RetroArch cargue uno). «Automático»
    /// con un juego deja ese juego siguiendo al PC, y la global también.
    func pick(path: String?, id: String?) -> RetroLayoutChoice {
        guard let path else { return RetroLayoutChoice(global: id ?? Self.auto, byPath: byPath) }
        let rest = byPath.filter { $0.path != path }
        guard let id else { return RetroLayoutChoice(global: Self.auto, byPath: rest) }
        return RetroLayoutChoice(global: global, byPath: Array((rest + [Entry(path: path, id: id)]).suffix(Self.cap)))
    }

    func encode() -> String {
        let obj: [String: Any] = ["global": global, "byPath": byPath.map { [$0.path, $0.id] }]
        guard let data = try? JSONSerialization.data(withJSONObject: obj), let s = String(data: data, encoding: .utf8) else { return "" }
        return s
    }

    static func decode(_ text: String?) -> RetroLayoutChoice {
        guard let text, !text.isEmpty, let data = text.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return RetroLayoutChoice() }
        let pairs = (obj["byPath"] as? [[String]] ?? []).compactMap { $0.count >= 2 ? Entry(path: $0[0], id: $0[1]) : nil }
        let global = (obj["global"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? auto
        return RetroLayoutChoice(global: global, byPath: Array(pairs.suffix(cap)))
    }
}
