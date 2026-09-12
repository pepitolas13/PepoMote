import SwiftUI

/// Conectar: tus PCs guardados (toca uno para conectar; mantén pulsado para
/// olvidarlo), el escáner del QR del PC y los receptores nuevos vistos en la
/// red. `pairReason` (no nil) = se llega aquí porque el PC no responde o ya
/// no reconoce el emparejamiento: se explica arriba, en rojo.
struct PairScreen: View {
    @EnvironmentObject var model: AppModel
    @State private var receivers: [ReceiverInfo] = []
    @State private var scanning = true
    @State private var forgetting: Pairing?
    @State private var saved: [Pairing] = PairStore.all()
    @State private var currentToken: String? = PairStore.current()?.token

    var body: some View {
        let reason = model.pairReason
        let unknown = PairList.unknown(receivers, saved)
        return VStack(alignment: .leading, spacing: 0) {
            Spacer().frame(height: 24)
            Text(reason != nil ? tr("pair_title_other") : tr("channel_connect"))
                .font(PepoFont.headline())
                .foregroundColor(Pepo.text)
            Text(saved.isEmpty ? tr("pair_sub_scan") : tr("pair_sub_saved")).pepoBody()
            if let reason {
                Spacer().frame(height: 14)
                Text(reason)
                    .font(PepoFont.bodyMedium())
                    .foregroundColor(Pepo.text)
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .pepoCard(border: Pepo.error)
            }
            Spacer().frame(height: 20)
            ScrollView(showsIndicators: false) {
                LazyVStack(alignment: .leading, spacing: 10) {
                    if !saved.isEmpty {
                        Text(tr("your_pcs")).pepoTitle()
                        ForEach(saved, id: \.token) { p in
                            SavedPcCard(
                                p: p,
                                current: p.token == currentToken,
                                online: PairList.isOnline(p, receivers),
                                onTap: { model.onSavedPcChosen(p) },
                                onLongPress: { forgetting = p }
                            )
                        }
                        Spacer().frame(height: 4)
                    }
                    PrimaryButton(title: saved.isEmpty ? tr("scan_qr") : tr("scan_qr_other"), height: 64) {
                        model.scanning = true
                    }
                    Spacer().frame(height: 10)
                    Text(
                        scanning && receivers.isEmpty ? tr("searching")
                            : (unknown.isEmpty && saved.isEmpty) ? tr("no_receivers")
                            : unknown.isEmpty ? tr("no_new_pcs")
                            : tr("on_your_network")
                    )
                    .pepoBody()
                    ForEach(unknown, id: \.host) { r in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(r.name).pepoTitle()
                            Text(tr("receiver_hint", "\(r.host):\(r.tcpPort)")).pepoBody()
                        }
                        .padding(16)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .pepoCard()
                    }
                    Text(tr("local_network_hint"))
                        .font(PepoFont.nunito(12, .regular))
                        .foregroundColor(Pepo.textDim)
                        .padding(.top, 8)
                }
            }
            TextLink(title: tr("back")) {
                LinkState.shared.pendingMode = nil // lo pedido antes del QR ya no va
                model.screen = .home
            }
            .frame(maxWidth: .infinity)
            Spacer().frame(height: 16)
        }
        .padding(.horizontal, 20)
        .background(Pepo.background.ignoresSafeArea())
        .task {
            while !Task.isCancelled {
                scanning = true
                let found = await Discovery.scan()
                if Task.isCancelled { break }
                receivers = found
                scanning = false
                try? await Task.sleep(nanoseconds: 2_500_000_000)
            }
        }
        .onChange(of: model.pairVersion) { _ in
            saved = PairStore.all()
            currentToken = PairStore.current()?.token
        }
        .confirmationDialog(
            forgetting.map { tr("forget_title", $0.pcName) } ?? "",
            isPresented: Binding(get: { forgetting != nil }, set: { if !$0 { forgetting = nil } }),
            titleVisibility: .visible
        ) {
            Button(tr("forget"), role: .destructive) {
                if let p = forgetting { model.onForgetPc(p) }
                forgetting = nil
            }
            Button(tr("cancel"), role: .cancel) { forgetting = nil }
        } message: {
            Text(tr("forget_body"))
        }
    }
}

/// Un PC guardado: punto verde si se ve en la red, borde azul si es el actual.
private struct SavedPcCard: View {
    let p: Pairing
    let current: Bool
    let online: Bool
    let onTap: () -> Void
    let onLongPress: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Circle()
                .fill(online ? Pepo.ok : Pepo.textDim)
                .frame(width: 10, height: 10)
            VStack(alignment: .leading, spacing: 2) {
                Text(p.pcName).pepoTitle()
                Text(online ? tr("pc_online", "\(p.host):\(p.port)") : "\(p.host):\(p.port)").pepoBody()
            }
            Spacer()
            if current {
                Text(tr("current"))
                    .font(PepoFont.bodyMedium())
                    .foregroundColor(Pepo.blue)
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity)
        .pepoCard(border: current ? Pepo.blue : Pepo.cardBorder)
        .contentShape(Rectangle())
        .onTapGesture { onTap() }
        .onLongPressGesture(minimumDuration: 0.5) { onLongPress() }
    }
}
