import AVFoundation
import SwiftUI
import UIKit

/// Escáner del QR del PC (pantalla completa): la cámara, el aviso de qué
/// apuntar y Cancelar. Un solo resultado por apertura.
struct QRScannerScreen: View {
    let onResult: (String?) -> Void
    @State private var denied = false

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            if denied {
                Text(tr("camera_denied"))
                    .font(PepoFont.bodyLarge())
                    .foregroundColor(.white)
                    .multilineTextAlignment(.center)
                    .padding(32)
            } else {
                QRCameraView(onCode: { code in onResult(code) }, onDenied: { denied = true })
                    .ignoresSafeArea()
            }
            VStack {
                Spacer()
                Text(tr("scan_prompt"))
                    .font(PepoFont.titleMedium())
                    .foregroundColor(.white)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 24)
                Button(action: { onResult(nil) }) {
                    Text(tr("cancel"))
                        .font(PepoFont.labelLarge())
                        .foregroundColor(.white)
                        .padding(.horizontal, 28)
                        .padding(.vertical, 12)
                        .background(Color.white.opacity(0.18))
                        .clipShape(Capsule())
                }
                .buttonStyle(.plain)
                .padding(.top, 14)
                .padding(.bottom, 36)
            }
        }
    }
}

/// La cámara con detección de códigos QR (AVFoundation).
struct QRCameraView: UIViewControllerRepresentable {
    let onCode: (String) -> Void
    let onDenied: () -> Void

    func makeUIViewController(context: Context) -> QRCameraController {
        let c = QRCameraController()
        c.onCode = onCode
        c.onDenied = onDenied
        return c
    }

    func updateUIViewController(_ uiViewController: QRCameraController, context: Context) {}
}

final class QRCameraController: UIViewController, AVCaptureMetadataOutputObjectsDelegate {
    var onCode: ((String) -> Void)?
    var onDenied: (() -> Void)?

    private let session = AVCaptureSession()
    private let sessionQueue = DispatchQueue(label: "pepomote.camera")
    private var preview: AVCaptureVideoPreviewLayer?
    private var fired = false

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized:
            configure()
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .video) { [weak self] granted in
                DispatchQueue.main.async {
                    if granted { self?.configure() } else { self?.onDenied?() }
                }
            }
        default:
            onDenied?()
        }
    }

    private func configure() {
        guard let device = AVCaptureDevice.default(for: .video),
              let input = try? AVCaptureDeviceInput(device: device)
        else {
            onDenied?()
            return
        }
        let output = AVCaptureMetadataOutput()
        session.beginConfiguration()
        if session.canAddInput(input) { session.addInput(input) }
        if session.canAddOutput(output) {
            session.addOutput(output)
            output.setMetadataObjectsDelegate(self, queue: .main)
            if output.availableMetadataObjectTypes.contains(.qr) {
                output.metadataObjectTypes = [.qr]
            }
        }
        session.commitConfiguration()
        let layer = AVCaptureVideoPreviewLayer(session: session)
        layer.videoGravity = .resizeAspectFill
        layer.frame = view.bounds
        view.layer.insertSublayer(layer, at: 0)
        preview = layer
        updateOrientation()
        sessionQueue.async { [session] in session.startRunning() }
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        preview?.frame = view.bounds
        updateOrientation()
    }

    private func updateOrientation() {
        guard let conn = preview?.connection, conn.isVideoOrientationSupported else { return }
        let o = view.window?.windowScene?.interfaceOrientation ?? .portrait
        switch o {
        case .landscapeLeft: conn.videoOrientation = .landscapeLeft
        case .landscapeRight: conn.videoOrientation = .landscapeRight
        case .portraitUpsideDown: conn.videoOrientation = .portraitUpsideDown
        default: conn.videoOrientation = .portrait
        }
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        sessionQueue.async { [session] in
            if session.isRunning { session.stopRunning() }
        }
    }

    func metadataOutput(_ output: AVCaptureMetadataOutput, didOutput metadataObjects: [AVMetadataObject], from connection: AVCaptureConnection) {
        guard !fired,
              let obj = metadataObjects.first as? AVMetadataMachineReadableCodeObject,
              obj.type == .qr, let text = obj.stringValue, !text.isEmpty
        else { return }
        fired = true
        Haptics.tap()
        sessionQueue.async { [session] in session.stopRunning() }
        onCode?(text)
    }
}
