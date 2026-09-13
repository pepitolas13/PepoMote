import Foundation

/// Un HEAD a `releases/latest` de GitHub SIN seguir la redirección: la
/// etiqueta de la última versión va en la cabecera `Location`. Cualquier
/// fallo (sin red, GitHub caído) es silencioso: nil.
final class UpdateClient: NSObject, URLSessionTaskDelegate {
    static let shared = UpdateClient()

    private lazy var session: URLSession = {
        let cfg = URLSessionConfiguration.ephemeral
        cfg.timeoutIntervalForRequest = UpdateCheck.timeout
        cfg.timeoutIntervalForResource = UpdateCheck.timeout * 2
        cfg.httpAdditionalHeaders = ["User-Agent": UpdateCheck.userAgent]
        return URLSession(configuration: cfg, delegate: self, delegateQueue: nil)
    }()

    func latest() async -> AppVersion? {
        var req = URLRequest(url: UpdateCheck.latestURL)
        req.httpMethod = "HEAD"
        do {
            let (_, response) = try await session.data(for: req)
            guard let http = response as? HTTPURLResponse, (300...399).contains(http.statusCode) else { return nil }
            return UpdateCheck.version(fromLocation: http.value(forHTTPHeaderField: "Location"))
        } catch {
            return nil
        }
    }

    // La redirección no se sigue: queremos el 302 con su `Location`.
    func urlSession(
        _ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
        newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void
    ) {
        completionHandler(nil)
    }
}
