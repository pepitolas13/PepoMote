import SwiftUI

struct OnboardingScreen: View {
    let onDone: () -> Void

    var body: some View {
        VStack(alignment: .center, spacing: 0) {
            Spacer().frame(height: 48)
            Text("PepoMote").font(PepoFont.display()).foregroundColor(Pepo.text)
            Text(tr("onboarding_sub")).pepoBody()
            Spacer().frame(height: 32)
            Step(n: 1, title: tr("ob1_title"), text: tr("ob1_body"))
            Spacer().frame(height: 14)
            Step(n: 2, title: tr("ob2_title"), text: tr("ob2_body"))
            Spacer().frame(height: 14)
            Step(n: 3, title: tr("ob3_title"), text: tr("ob3_body"))
            Spacer()
            PrimaryButton(title: tr("lets_go"), action: onDone)
            Spacer().frame(height: 24)
        }
        .padding(.horizontal, 24)
        .frame(maxWidth: 560)
        .frame(maxWidth: .infinity)
        .background(Pepo.background.ignoresSafeArea())
    }
}

private struct Step: View {
    let n: Int
    let title: String
    let text: String

    var body: some View {
        HStack(spacing: 14) {
            ZStack {
                Circle().fill(Pepo.blue).frame(width: 38, height: 38)
                Text("\(n)").font(PepoFont.titleMedium()).foregroundColor(Pepo.onAccent)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(title).pepoTitle()
                Text(text).pepoBody()
            }
            Spacer(minLength: 0)
        }
        .padding(16)
        .frame(maxWidth: .infinity)
        .pepoCard()
    }
}
