import Foundation
import SwiftUI

/// Una zona pulsable de la pantalla del mando: su bit, dónde está (en el
/// espacio de coordenadas común `PressRegistry.padSpace`) y cómo suena.
struct PressZone {
    let bit: UInt32
    /// Marco en reposo dentro del espacio «pad».
    var rect: CGRect
    /// Redonda (`RoundButton`): la zona es el círculo, no su cuadrado, así la
    /// esquina entre dos botones no se la queda ninguno.
    var circular: Bool
    /// Sonido «pop» en vez del bip (A y 2).
    var pop: Bool

    /// ¿Cae el dedo dentro?
    func contains(_ p: CGPoint) -> Bool {
        guard rect.width > 0, rect.height > 0 else { return false }
        if !circular { return rect.contains(p) }
        let r = Swift.min(rect.width, rect.height) / 2
        let dx = p.x - rect.midX
        let dy = p.y - rect.midY
        return dx * dx + dy * dy <= r * r
    }
}

/// Dónde está cada botón de la pantalla y qué tiene cogido cada dedo, para
/// «Pulsar deslizando»: el botón bajo el dedo se pulsa, y deslizar a otro
/// suelta el primero y pulsa el nuevo.
///
/// `zones` NO se publica a propósito: lo llena el `GeometryReader` de cada
/// botón al medirse, y volver a dibujar por eso sería un bucle. Lo que sí se
/// publica es qué zonas están pulsadas, que es lo que pinta cada botón.
final class PressRegistry: ObservableObject {
    /// Nombre del espacio de coordenadas común de cada pantalla del mando.
    static let padSpace = "pad"

    /// Zonas pulsadas ahora mismo (solo con «Pulsar deslizando»).
    @Published private(set) var pressed: Set<UUID> = []

    /// Con la pantalla inerte (el GamePad esperando el modo, o a pantalla
    /// completa) no se pulsa nada deslizando. Publicado: los botones eligen
    /// su gesto con esto y tienen que redibujarse cuando cambia.
    @Published var enabled = true

    private(set) var zones: [UUID: PressZone] = [:]

    /// Qué dedo tiene cogida cada zona pulsada (zona → dedo).
    private var owners: [UUID: UUID] = [:]

    // MARK: zonas

    func place(_ id: UUID, _ zone: PressZone) {
        zones[id] = zone
    }

    /// El botón se va de la pantalla: si estaba pulsado, se suelta (sea de
    /// quien sea: ya no hay botón que soltar después).
    func remove(_ id: UUID) {
        forget(id)
        zones.removeValue(forKey: id)
    }

    /// Bit de esa zona (`nil` si el dedo no tiene ninguna).
    func bit(_ id: UUID?) -> UInt32? {
        guard let id else { return nil }
        return zones[id]?.bit
    }

    /// Zona bajo el dedo, o `nil` si está en el vacío.
    func resolve(_ p: CGPoint) -> UUID? {
        PressRegistry.resolve(zones, p)
    }

    /// Puro (mismos casos que `PressHit.resolve` en Android): de las zonas que
    /// pisa el dedo gana la que tenga el centro más cerca; a igual distancia,
    /// siempre la misma (el diccionario no tiene orden).
    static func resolve(_ zones: [UUID: PressZone], _ p: CGPoint) -> UUID? {
        var best: UUID?
        var bestD = CGFloat.greatestFiniteMagnitude
        for (id, zone) in zones {
            guard zone.contains(p) else { continue }
            let dx = p.x - zone.rect.midX
            let dy = p.y - zone.rect.midY
            let d = dx * dx + dy * dy
            if d < bestD || (d == bestD && best.map({ id.uuidString < $0.uuidString }) == true) {
                best = id
                bestD = d
            }
        }
        return best
    }

    // MARK: pulsar y soltar

    /// Efectos de una pulsación (bit, háptica y sonido), compartidos por el
    /// gesto propio de cada botón y por el deslizamiento.
    static func fire(bit: UInt32, pop: Bool, down: Bool) {
        ButtonState.shared.set(bit, down)
        guard down else { return }
        Haptics.tap()
        if pop { UiSounds.shared.pop() } else { UiSounds.shared.blip() }
    }

    /// Dos dedos en el mismo botón: se lo queda el PRIMERO hasta que lo suelta,
    /// y el segundo ni lo vuelve a pulsar ni lo suelta al levantarse (igual que
    /// `SlideTracker` en Android, `unBotonQueLlevaOtroDedoNoSeLeQuita`). Por
    /// eso hace falta saber de quién es cada zona: `finger` identifica al dedo
    /// que pulsa (un gesto, un dedo).
    func press(_ id: UUID, by finger: UUID) {
        guard enabled, owners[id] == nil, let zone = zones[id] else { return }
        owners[id] = finger
        pressed.insert(id)
        PressRegistry.fire(bit: zone.bit, pop: zone.pop, down: true)
    }

    func release(_ id: UUID, by finger: UUID) {
        guard owners[id] == finger else { return }
        forget(id)
    }

    /// ¿Puede este dedo coger esa zona? Libre, o ya suya.
    func free(_ id: UUID, for finger: UUID) -> Bool {
        let owner = owners[id]
        return owner == nil || owner == finger
    }

    /// Al salir de la pantalla no se queda nada pulsado.
    func releaseAll() {
        guard !pressed.isEmpty else { return }
        for id in pressed {
            guard let zone = zones[id] else { continue }
            PressRegistry.fire(bit: zone.bit, pop: zone.pop, down: false)
        }
        pressed.removeAll()
        owners.removeAll()
    }

    /// Suelta la zona sea de quien sea (se va de la pantalla, se acabó todo).
    private func forget(_ id: UUID) {
        owners.removeValue(forKey: id)
        guard pressed.remove(id) != nil, let zone = zones[id] else { return }
        PressRegistry.fire(bit: zone.bit, pop: zone.pop, down: false)
    }
}

/// Lienzo de fondo de las pantallas del mando: recoge los dedos que nacen en
/// el vacío para que, con «Pulsar deslizando», también pulsen al entrar en un
/// botón. Va el PRIMERO del `ZStack` de la pantalla: cualquier control de
/// encima (stick, cruceta, tiras, chips, pantalla táctil) se queda con su dedo
/// antes de llegar aquí.
///
/// UN SOLO DEDO, a diferencia de Android: `DragGesture` de SwiftUI no es
/// multitáctil, así que del vacío sale un único dedo deslizante (el segundo no
/// pulsa nada). Los dedos que nacen SOBRE un botón son otro gesto cada uno y
/// siguen yendo por su cuenta, que es el caso normal; está contado en
/// `docs/TROUBLESHOOTING.md`.
struct SlideCanvas: View {
    /// Con la pantalla inerte, ni eso (el GamePad sin el modo confirmado).
    var enabled = true
    @EnvironmentObject private var reg: PressRegistry
    @AppStorage(AppPrefs.slidePressKey) private var slidePress = false
    /// Zona que este dedo tiene cogida.
    @State private var held: UUID?
    /// Quién es este dedo para el registro (el lienzo es uno solo).
    @State private var finger = UUID()

    private var active: Bool { enabled && slidePress }
    private var padSpace: CoordinateSpace { .named(PressRegistry.padSpace) }

    var body: some View {
        Color.clear
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0, coordinateSpace: padSpace)
                    .onChanged { v in
                        guard active else { return }
                        // El dedo nace libre y va cogiendo lo que pisa, menos
                        // lo que ya lleve otro dedo (ese no se le quita)
                        let over = reg.resolve(v.location).flatMap { reg.free($0, for: finger) ? $0 : nil }
                        switch PressStep.next(slide: true, sticky: true, held: reg.bit(held), over: reg.bit(over)) {
                        case .keep:
                            break
                        case .to(let bit):
                            if let old = held { reg.release(old, by: finger) }
                            held = bit == nil ? nil : over
                            if let id = held { reg.press(id, by: finger) }
                        }
                    }
                    .onEnded { _ in
                        if let id = held { reg.release(id, by: finger) }
                        held = nil
                    }
            )
            // Sin el ajuste, el lienzo es del todo transparente al tacto
            .allowsHitTesting(active)
            .onAppear { reg.enabled = enabled }
            .onChange(of: enabled) { on in
                reg.enabled = on
                if !on { reg.releaseAll() }
            }
            .onDisappear { reg.releaseAll() }
    }
}
