import SwiftUI

/// Aviso de versión nueva en el inicio: «PepoMote X disponible», el enlace a
/// la release de GitHub y «Ocultar» (esa versión no se vuelve a anunciar).
struct UpdateCard: View {
    let version: AppVersion
    let onDismiss: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(tr("update_available", version.description)).pepoTitle()
            HStack(spacing: 16) {
                Link(tr("update_download"), destination: UpdateCheck.releaseURL(version))
                    .font(PepoFont.labelLarge())
                    .foregroundColor(Pepo.blue)
                TextLink(title: tr("update_dismiss"), action: onDismiss)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .pepoCard(border: Pepo.blue, radius: 14)
    }
}
