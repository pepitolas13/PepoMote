import SwiftUI

struct RootView: View {
    @EnvironmentObject var model: AppModel
    @ObservedObject var link = LinkState.shared

    private var wantLandscape: Bool {
        model.screen == .controller && Route.route(link.link, link.intent) == .gamePad
    }

    var body: some View {
        ZStack {
            Pepo.background.ignoresSafeArea()
            screenView
                .id(model.screen)
                .transition(.opacity)
            if let t = model.toast {
                ToastView(text: t)
                    .transition(.opacity)
            }
        }
        .animation(.easeInOut(duration: 0.18), value: model.screen)
        .animation(.easeInOut(duration: 0.2), value: model.toast)
        // Error de conexión: al escáner si el PC ya no reconoce el
        // emparejamiento, o aviso y vuelta al inicio
        .onReceive(link.$link) { l in
            if case .failed(let code, let msg) = l { model.onLinkFailed(code: code, msg: msg) }
        }
        // Apaisado fijo mientras el GamePad esté en pantalla; al salir, como estaba
        .onChange(of: wantLandscape) { v in OrientationLock.set(v ? .landscape : .all) }
        .onAppear { OrientationLock.set(wantLandscape ? .landscape : .all) }
        .fullScreenCover(isPresented: $model.scanning) {
            QRScannerScreen { contents in
                model.scanning = false
                if let contents { model.onPairContent(contents) }
            }
        }
    }

    @ViewBuilder
    private var screenView: some View {
        switch model.screen {
        case .onboarding:
            OnboardingScreen {
                AppPrefs.onboarded = true
                model.screen = .home
            }
        case .home:
            HomeScreen()
        case .pair:
            PairScreen()
        case .settings:
            SettingsScreen()
        case .nunchuk:
            NunchukScreen(onDisconnect: { model.disconnectNunchuk() })
        case .controller:
            ControllerRoute()
        }
    }
}

/// El mando, según `Route`: GamePad de Wii U (confirmado o pedido y
/// pendiente), Nunchuk, o los layouts Wii de siempre según la orientación.
struct ControllerRoute: View {
    @EnvironmentObject var model: AppModel
    @ObservedObject var link = LinkState.shared

    var body: some View {
        GeometryReader { geo in
            let landscape = geo.size.width > geo.size.height
            Group {
                switch Route.route(link.link, link.intent) {
                case .gamePad:
                    GamePadScreen(onDisconnect: { model.disconnect() })
                case .nunchuk:
                    NunchukScreen(onDisconnect: { model.disconnect() })
                case .wii:
                    // Selector Puntero/Dolphin/Wii U: entrando por Conectar/Mando/Wii U
                    // con el ajuste activo (y siempre dentro de Wii U). Por Dolphin:
                    // pantalla solo-Dolphin.
                    let showChips = !model.controllerDolphinOnly && AppPrefs.showDolphinChips
                    if landscape {
                        ControllerLandscapeScreen(showChips: showChips, onDisconnect: { model.disconnect() })
                    } else {
                        ControllerScreen(showChips: showChips, onDisconnect: { model.disconnect() })
                    }
                }
            }
            .frame(width: geo.size.width, height: geo.size.height)
        }
    }
}

/// Aviso corto abajo (el Toast de Android).
struct ToastView: View {
    let text: String

    var body: some View {
        VStack {
            Spacer()
            Text(text)
                .font(PepoFont.bodyMedium())
                .foregroundColor(Pepo.text)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                .background(Pepo.card)
                .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).stroke(Pepo.cardBorder, lineWidth: 1.5))
                .padding(.horizontal, 24)
                .padding(.bottom, 40)
        }
        .allowsHitTesting(false)
    }
}
