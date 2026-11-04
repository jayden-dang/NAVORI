import SwiftUI

struct InlineConfirmation: View {
    private enum Field { case cancel }

    let text: String
    var note: String?
    let onConfirm: () -> Void
    let onCancel: () -> Void
    @FocusState private var focus: Field?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(text)
                .font(.system(size: 13))
                .foregroundStyle(Color(nsColor: .labelColor))
                .fixedSize(horizontal: false, vertical: true)
            if let note {
                Text(note)
                    .font(.system(size: 12))
                    .foregroundStyle(Color(nsColor: .secondaryLabelColor))
            }
            HStack(spacing: 8) {
                Spacer(minLength: 0)
                Button("Cancel", action: onCancel)
                    .focused($focus, equals: .cancel)
                Button("Stop", role: .destructive, action: onConfirm)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(text)
        .onExitCommand(perform: onCancel)
        .onAppear { focus = .cancel }
    }
}

struct StopFailureLines: View {
    let reasons: [String]

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(Array(reasons.enumerated()), id: \.offset) { _, reason in
                Text(reason)
                    .font(.system(size: 12))
                    .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, 8)
        .padding(.bottom, 4)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
