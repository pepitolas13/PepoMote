import SwiftUI

/// Pregunta de la primera vez del mando universal: mover el móvil mueve el
/// stick derecho, y eso en unos juegos es riqueza y en otros un estorbo (en
/// The Binding of Isaac disparas hacia donde muevas el mando; en Until Dawn
/// hay que dejarlo completamente quieto). «Sin giro» y «Con giro» se guardan
/// para siempre (`AppPrefs.padAim`) y Ajustes lo cambia. Mientras la tarjeta
/// está en pantalla el giro va apagado, así que el juego no hace nada raro
/// mientras se lee. La misma que en Android.
struct PadAimAskCard: View {
    let onPick: (Bool) -> Void

    var body: some View {
        VStack(spacing: 6) {
            Text(tr("pad_aim_ask_title")).pepoTitle().multilineTextAlignment(.center)
            Text(tr("pad_aim_ask_body")).pepoBody().multilineTextAlignment(.center)
            HStack(spacing: 10) {
                ModeChip(label: tr("pad_aim_no"), selected: false, action: { onPick(false) })
                ModeChip(label: tr("pad_aim_yes"), selected: true, action: { onPick(true) })
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .frame(maxWidth: 360)
        .pepoCard(border: Pepo.warn, radius: 14)
    }
}
