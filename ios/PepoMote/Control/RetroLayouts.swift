import Foundation

/// Plantillas de mando de RetroArch por consola: qué botones enseña el móvil,
/// con qué etiqueta y color, y qué bit del INPUT emite cada uno. El receptor
/// traduce los bits al RetroPad como siempre (A→A, B→B, X→X, Y→Y, L→L, R→R,
/// ZL→L2, ZR→R2, clics→L3/R3, +→Start, −→Select); cada botón en pantalla emite
/// el bit cuyo id del RetroPad espera el mapeo por defecto del núcleo. Fijos
/// en toda plantilla y fuera de la tabla: la cruceta, «Menú» (HOME) y «Rápido»
/// (SCREEN). Copia a mano de `pmp::retro` (la fuente de verdad, en Rust); el
/// test de paridad la compara con `protocol/retro-layouts.json`.
enum RetroShape: String {
    /// Un botón: Atari 2600.
    case single
    /// Dos en fila (`left`, `right`), con `stagger`: NES, Game Boy, Master System, PC Engine, N64.
    case two
    /// Rombo `top`/`left`/`right`/`bottom`: RetroPad, SNES, Neo Geo, PlayStation.
    case diamond
    /// Dos filas de tres `tl tm tr` / `bl bm br`: Mega Drive de 6 botones, arcade.
    case grid2x3
    /// Tres en fila `l m r`: Mega Drive de 3 botones.
    case row3

    var slots: [String] {
        switch self {
        case .single: return ["center"]
        case .two: return ["left", "right"]
        case .diamond: return ["top", "left", "right", "bottom"]
        case .grid2x3: return ["tl", "tm", "tr", "bl", "bm", "br"]
        case .row3: return ["l", "m", "r"]
        }
    }
}

/// Qué va en el hueco del stick derecho.
enum RightStick: String {
    case none
    case analog
    /// N64: cuatro botones C que emiten los ejes del stick derecho a ±127.
    case cbuttons
}

/// Color del botón; cada app lo lleva a su tema (`card` = el de siempre).
enum RetroColor: String {
    case card, red, yellow, green, blue, pink, purple
}

/// Un botón frontal. `label` es el texto tal cual salvo `$coin` y `$fire`, que
/// se traducen («Moneda», «Disparo»). `primary`: el azul destacado de siempre
/// (con «pop»). `size`: relativo al botón normal.
struct RetroFace: Equatable {
    let slot: String
    let label: String
    let bit: UInt32
    var color: RetroColor = .card
    var primary = false
    var size: Double = 1
}

/// Un botón de la fila central (Select/Start y sus nombres en cada consola).
struct RetroCenter: Equatable {
    let label: String
    let bit: UInt32
}

/// Etiquetas de los hombros; nil = ese hombro no existe en la consola.
struct RetroShoulders: Equatable {
    let l: String?
    let r: String?
    let l2: String?
    let r2: String?
}

struct RetroLayout: Equatable {
    let id: String
    /// Nombre propio, igual en los dos idiomas.
    let name: String
    let shape: RetroShape
    /// Solo `.two`: −1 el hueco izquierdo más alto (N64), 0 a nivel, 1 más bajo (Game Boy).
    let stagger: Int
    let face: [RetroFace]
    let leftStick: Bool
    let rightStick: RightStick
    /// L3/R3 (solo con sticks analógicos).
    let stickClicks: Bool
    let shoulders: RetroShoulders
    let center: [RetroCenter]
}

enum RetroLayouts {
    private static let noShoulders = RetroShoulders(l: nil, r: nil, l2: nil, r2: nil)
    private static let lr = RetroShoulders(l: "L", r: "R", l2: nil, r2: nil)
    private static let selectStart = [RetroCenter(label: "Select", bit: Btn.minus), RetroCenter(label: "Start", bit: Btn.plus)]
    private static let modeStart = [RetroCenter(label: "Mode", bit: Btn.minus), RetroCenter(label: "Start", bit: Btn.plus)]
    private static let coinStart = [RetroCenter(label: "$coin", bit: Btn.minus), RetroCenter(label: "Start", bit: Btn.plus)]

    /// El RetroPad completo: lo de hoy, y lo que se enseña cuando no se sabe la consola.
    static let retroPad = RetroLayout(
        id: "retropad", name: "RetroPad", shape: .diamond, stagger: 0,
        face: [
            RetroFace(slot: "top", label: "X", bit: Btn.x),
            RetroFace(slot: "left", label: "Y", bit: Btn.y),
            RetroFace(slot: "right", label: "A", bit: Btn.a, primary: true),
            RetroFace(slot: "bottom", label: "B", bit: Btn.b)
        ],
        leftStick: true, rightStick: .analog, stickClicks: true,
        shoulders: RetroShoulders(l: "L", r: "R", l2: "L2", r2: "R2"), center: selectStart
    )

    /// Mando universal: el rombo de Xbox, con cada letra DONDE el jugador
    /// espera verla (A abajo, B derecha, X izquierda, Y arriba). El receptor
    /// traduce cada bit a su botón de XInput sin cruzar nada, así que lo que
    /// se lee en pantalla es lo que recibe el juego.
    ///
    /// Fuera de `all` a propósito: no es una consola de RetroArch, solo
    /// reutiliza el mismo formato de plantilla, y así el test de paridad con
    /// `protocol/retro-layouts.json` sigue valiendo tal cual.
    static let xbox = RetroLayout(
        id: "xbox", name: "Xbox", shape: .diamond, stagger: 0,
        face: [
            RetroFace(slot: "top", label: "Y", bit: Btn.y),
            RetroFace(slot: "left", label: "X", bit: Btn.x),
            RetroFace(slot: "right", label: "B", bit: Btn.b),
            RetroFace(slot: "bottom", label: "A", bit: Btn.a, primary: true)
        ],
        leftStick: true, rightStick: .analog, stickClicks: true,
        shoulders: RetroShoulders(l: "LB", r: "RB", l2: "LT", r2: "RT"),
        center: [RetroCenter(label: "Back", bit: Btn.minus), RetroCenter(label: "Start", bit: Btn.plus)]
    )

    /// Todas las plantillas, con `retropad` la primera (mismo orden que el JSON).
    static let all: [RetroLayout] = [
        retroPad,
        // FCEUmm / Nestopia: A→A, B→B
        RetroLayout(id: "nes", name: "NES", shape: .two, stagger: 0,
                    face: [RetroFace(slot: "left", label: "B", bit: Btn.b, color: .red), RetroFace(slot: "right", label: "A", bit: Btn.a, color: .red, primary: true)],
                    leftStick: false, rightStick: .none, stickClicks: false, shoulders: noShoulders, center: selectStart),
        // Gambatte / SameBoy: A→A, B→B
        RetroLayout(id: "gb", name: "Game Boy", shape: .two, stagger: 1,
                    face: [RetroFace(slot: "left", label: "B", bit: Btn.b, color: .purple), RetroFace(slot: "right", label: "A", bit: Btn.a, color: .purple, primary: true)],
                    leftStick: false, rightStick: .none, stickClicks: false, shoulders: noShoulders, center: selectStart),
        // mGBA: A→A, B→B, L→L, R→R
        RetroLayout(id: "gba", name: "Game Boy Advance", shape: .two, stagger: 1,
                    face: [RetroFace(slot: "left", label: "B", bit: Btn.b), RetroFace(slot: "right", label: "A", bit: Btn.a, primary: true)],
                    leftStick: false, rightStick: .none, stickClicks: false, shoulders: lr, center: selectStart),
        // Snes9x: identidad (colores del mando PAL/Super Famicom)
        RetroLayout(id: "snes", name: "SNES", shape: .diamond, stagger: 0,
                    face: [
                        RetroFace(slot: "top", label: "X", bit: Btn.x, color: .blue),
                        RetroFace(slot: "left", label: "Y", bit: Btn.y, color: .green),
                        RetroFace(slot: "right", label: "A", bit: Btn.a, color: .red, primary: true),
                        RetroFace(slot: "bottom", label: "B", bit: Btn.b, color: .yellow)
                    ],
                    leftStick: false, rightStick: .none, stickClicks: false, shoulders: lr, center: selectStart),
        // Genesis Plus GX / PicoDrive (Master System, Game Gear): 1→B, 2→A, Pause→Start
        RetroLayout(id: "ms", name: "Master System", shape: .two, stagger: 0,
                    face: [RetroFace(slot: "left", label: "1", bit: Btn.b, primary: true), RetroFace(slot: "right", label: "2", bit: Btn.a)],
                    leftStick: false, rightStick: .none, stickClicks: false, shoulders: noShoulders, center: [RetroCenter(label: "Pause", bit: Btn.plus)]),
        // Genesis Plus GX / PicoDrive: A→Y, B→B, C→A, X→L, Y→X, Z→R, Mode→Select
        RetroLayout(id: "md", name: "Mega Drive", shape: .grid2x3, stagger: 0,
                    face: [
                        RetroFace(slot: "tl", label: "X", bit: Btn.l, size: 0.8),
                        RetroFace(slot: "tm", label: "Y", bit: Btn.x, size: 0.8),
                        RetroFace(slot: "tr", label: "Z", bit: Btn.r, size: 0.8),
                        RetroFace(slot: "bl", label: "A", bit: Btn.y),
                        RetroFace(slot: "bm", label: "B", bit: Btn.b),
                        RetroFace(slot: "br", label: "C", bit: Btn.a, primary: true)
                    ],
                    leftStick: false, rightStick: .none, stickClicks: false, shoulders: noShoulders, center: modeStart),
        // El mando de 3 botones, solo a mano
        RetroLayout(id: "md3", name: "Mega Drive (3)", shape: .row3, stagger: 0,
                    face: [RetroFace(slot: "l", label: "A", bit: Btn.y), RetroFace(slot: "m", label: "B", bit: Btn.b), RetroFace(slot: "r", label: "C", bit: Btn.a, primary: true)],
                    leftStick: false, rightStick: .none, stickClicks: false, shoulders: noShoulders, center: modeStart),
        // Beetle PCE: I→A, II→B, Run→Start
        RetroLayout(id: "pce", name: "PC Engine", shape: .two, stagger: 0,
                    face: [RetroFace(slot: "left", label: "II", bit: Btn.b), RetroFace(slot: "right", label: "I", bit: Btn.a, primary: true)],
                    leftStick: false, rightStick: .none, stickClicks: false, shoulders: noShoulders,
                    center: [RetroCenter(label: "Select", bit: Btn.minus), RetroCenter(label: "Run", bit: Btn.plus)]),
        // FBNeo («Classic») y MAME: 1..6 → B A Y X R L; en lucha LP MP HP = Y X L, LK MK HK = B A R
        RetroLayout(id: "arcade", name: "Arcade", shape: .grid2x3, stagger: 0,
                    face: [
                        RetroFace(slot: "tl", label: "Y", bit: Btn.y), RetroFace(slot: "tm", label: "X", bit: Btn.x), RetroFace(slot: "tr", label: "L", bit: Btn.l),
                        RetroFace(slot: "bl", label: "B", bit: Btn.b, primary: true), RetroFace(slot: "bm", label: "A", bit: Btn.a), RetroFace(slot: "br", label: "R", bit: Btn.r)
                    ],
                    leftStick: false, rightStick: .none, stickClicks: false, shoulders: noShoulders, center: coinStart),
        // FBNeo Neo Geo («Classic»): A→B, B→A, C→Y, D→X, en el rombo del mando de Neo Geo CD
        RetroLayout(id: "neogeo", name: "Neo Geo", shape: .diamond, stagger: 0,
                    face: [
                        RetroFace(slot: "top", label: "D", bit: Btn.x, color: .blue),
                        RetroFace(slot: "left", label: "C", bit: Btn.y, color: .green),
                        RetroFace(slot: "right", label: "B", bit: Btn.a, color: .yellow),
                        RetroFace(slot: "bottom", label: "A", bit: Btn.b, color: .red, primary: true)
                    ],
                    leftStick: false, rightStick: .none, stickClicks: false, shoulders: noShoulders, center: coinStart),
        // Stella: disparo→B, Select→Select, Reset→Start
        RetroLayout(id: "atari2600", name: "Atari 2600", shape: .single, stagger: 0,
                    face: [RetroFace(slot: "center", label: "$fire", bit: Btn.b, color: .red, primary: true)],
                    leftStick: false, rightStick: .none, stickClicks: false, shoulders: noShoulders,
                    center: [RetroCenter(label: "Select", bit: Btn.minus), RetroCenter(label: "Reset", bit: Btn.plus)]),
        // Mupen64Plus: A→B, B→Y, Z→L2, L→L, R→R; botones C = stick derecho
        RetroLayout(id: "n64", name: "Nintendo 64", shape: .two, stagger: -1,
                    face: [RetroFace(slot: "left", label: "B", bit: Btn.y, color: .green, size: 0.9), RetroFace(slot: "right", label: "A", bit: Btn.b, color: .blue, primary: true, size: 1.2)],
                    leftStick: true, rightStick: .cbuttons, stickClicks: false, shoulders: RetroShoulders(l: "L", r: "R", l2: "Z", r2: nil),
                    center: [RetroCenter(label: "Start", bit: Btn.plus)]),
        // PCSX-ReARMed / Beetle PSX: ✕→B, ○→A, □→Y, △→X, L1/R1/L2/R2, L3/R3
        RetroLayout(id: "psx", name: "PlayStation", shape: .diamond, stagger: 0,
                    face: [
                        RetroFace(slot: "top", label: "△", bit: Btn.x, color: .green),
                        RetroFace(slot: "left", label: "□", bit: Btn.y, color: .pink),
                        RetroFace(slot: "right", label: "○", bit: Btn.a, color: .red),
                        RetroFace(slot: "bottom", label: "✕", bit: Btn.b, color: .blue, primary: true)
                    ],
                    leftStick: true, rightStick: .analog, stickClicks: true, shoulders: RetroShoulders(l: "L1", r: "R1", l2: "L2", r2: "R2"), center: selectStart)
    ]

    /// Las consolas que el receptor puede anunciar en `game.console`.
    static let consoleIds = ["nes", "gb", "gba", "snes", "ms", "md", "pce", "arcade", "neogeo", "atari2600", "n64", "psx"]

    /// Ids válidos en `pad.layout` (todas las plantillas).
    static var layoutIds: [String] { all.map(\.id) }

    static func byId(_ id: String?) -> RetroLayout? { all.first { $0.id == id } }

    static func isConsole(_ id: String?) -> Bool { id.map { consoleIds.contains($0) } ?? false }

    /// Qué plantilla se enseña: la elegida a mano si es válida, si no la
    /// consola que anunció el receptor, si no el RetroPad completo (igual que en Rust).
    static func effective(console: String?, chosen: String?) -> String {
        if let chosen, layoutIds.contains(chosen) { return chosen }
        if let console, isConsole(console) { return console }
        return retroPad.id
    }

    /// Nombre del bit para el JSON de paridad.
    static let bitNames: [(String, UInt32)] = [
        ("A", Btn.a), ("B", Btn.b), ("X", Btn.x), ("Y", Btn.y), ("L", Btn.l), ("R", Btn.r),
        ("ZL", Btn.zl), ("ZR", Btn.zr), ("STICK_L", Btn.stickL), ("STICK_R", Btn.stickR),
        ("PLUS", Btn.plus), ("MINUS", Btn.minus)
    ]

    static func bitName(_ bit: UInt32) -> String? { bitNames.first { $0.1 == bit }?.0 }
}
