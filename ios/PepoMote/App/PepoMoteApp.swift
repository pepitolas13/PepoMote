import SwiftUI

@main
struct PepoMoteApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @StateObject private var model = AppModel.shared
    @StateObject private var l10n = L10n.shared
    @Environment(\.scenePhase) private var scenePhase

    init() {
        UiSounds.shared.prepare()
        Haptics.prepare()
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(model)
                .environmentObject(l10n)
                // Cambiar de idioma recompone la UI entera (como recrear la actividad en Android)
                .id(l10n.code)
                // Enlace profundo pepomote://pair?… (desde otra app o un QR leído por la cámara del sistema)
                .onOpenURL { url in model.onPairContent(url.absoluteString) }
        }
        .onChange(of: scenePhase) { phase in
            // La doble pantalla del GamePad solo se recibe con la app a la vista
            ScreenLink.shared.setForeground(phase == .active)
        }
    }
}
