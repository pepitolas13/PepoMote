import Foundation

/// Canal de pantalla del GamePad (doble pantalla): una sola instancia viva de
/// `ScreenClient`, y solo mientras coinciden tres cosas: hay enlace (el
/// servicio lo enlaza con host, puerto y sesión del `ok`), la pantalla
/// GamePad la pide con el tamaño de su zona táctil en píxeles, y la app está
/// en primer plano. Cada cambio recalcula: abrir y cerrar son idempotentes.
final class ScreenLink: ObservableObject {
    static let shared = ScreenLink()

    private struct Endpoint: Equatable {
        let host: String
        let port: Int
        let sessionId: UInt32
    }

    private struct Request: Equatable {
        let width: Int
        let height: Int
    }

    private struct Live: Equatable {
        let endpoint: Endpoint
        let request: Request
    }

    /// Cliente vivo (la pantalla GamePad pinta sus flujos), o nil.
    @Published private(set) var client: ScreenClient?

    private var endpoint: Endpoint?
    private var request: Request?
    private var foreground = false
    private var live: Live?

    /// El servicio, con el `ok` de un mando (un Nunchuk no tiene pantalla).
    func bind(host: String, port: Int, sessionId: UInt32) {
        endpoint = Endpoint(host: host, port: port, sessionId: sessionId)
        reconcile()
    }

    /// El servicio, al cerrar el enlace.
    func unbind() {
        endpoint = nil
        reconcile()
    }

    /// La pantalla GamePad operativa y como GamePad: tamaño máximo que quiere (píxeles).
    func request(width: Int, height: Int) {
        request = Request(width: width, height: height)
        reconcile()
    }

    /// La pantalla GamePad se va (o deja de ser GamePad).
    func release() {
        request = nil
        reconcile()
    }

    /// La escena: activa → true; inactiva/segundo plano → false.
    func setForeground(_ value: Bool) {
        foreground = value
        reconcile()
    }

    private func reconcile() {
        var wanted: Live?
        if let ep = endpoint, let req = request, foreground { wanted = Live(endpoint: ep, request: req) }
        if wanted == live { return }
        client?.close()
        client = nil
        live = wanted
        if let w = wanted {
            let c = ScreenClient(
                host: w.endpoint.host, port: w.endpoint.port, sessionId: w.endpoint.sessionId,
                maxWidth: w.request.width, maxHeight: w.request.height
            )
            c.start()
            client = c
        }
    }
}
