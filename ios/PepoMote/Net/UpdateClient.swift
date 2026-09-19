import Foundation

/// Stream the bounded public contract. No cookies, identifiers or credentials.
final class UpdateClient: NSObject, URLSessionTaskDelegate {
    static let shared = UpdateClient()

    private func makeSession() -> URLSession {
        let cfg = URLSessionConfiguration.ephemeral
        cfg.timeoutIntervalForRequest = UpdateCheck.timeout
        cfg.timeoutIntervalForResource = UpdateCheck.timeout * 2
        cfg.httpAdditionalHeaders = ["User-Agent": UpdateCheck.userAgent]
        cfg.httpShouldSetCookies = false
        cfg.urlCredentialStorage = nil
        cfg.requestCachePolicy = .reloadIgnoringLocalCacheData
        return URLSession(configuration: cfg, delegate: self, delegateQueue: nil)
    }

    func latest() async throws -> UpdateRelease {
        let session = makeSession()
        // Also stops unread response bytes on status/size/validation failure.
        defer { session.invalidateAndCancel() }
        var req = URLRequest(url: UpdateCheck.latestURL)
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        let (bytes, response) = try await session.bytes(for: req)
        guard let http = response as? HTTPURLResponse, http.statusCode == 200,
              let finalURL = http.url, UpdateCheck.allowsMetadataURL(finalURL) else { throw UpdateError.unavailable }
        guard response.expectedContentLength <= Int64(UpdateCheck.maxManifestBytes) else { throw UpdateError.tooLarge }
        var data = Data()
        for try await byte in bytes {
            try Task.checkCancellation()
            guard data.count < UpdateCheck.maxManifestBytes else { throw UpdateError.tooLarge }
            data.append(byte)
        }
        return try UpdateRelease.decode(data)
    }

    // GitHub release assets redirect to its signed CDN URL. Never follow to an
    // arbitrary domain, insecure URL or another repository. Limit redirect hops.
    func urlSession(
        _ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
        newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void
    ) {
        let hops = Int(task.taskDescription ?? "0") ?? 0
        guard hops < 5, let url = request.url, UpdateCheck.allowsMetadataURL(url) else {
            completionHandler(nil)
            return
        }
        task.taskDescription = String(hops + 1)
        completionHandler(request)
    }
}
