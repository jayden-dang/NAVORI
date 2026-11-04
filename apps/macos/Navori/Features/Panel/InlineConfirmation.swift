import SwiftUI

struct InlineConfirmation: View {
    private enum Field { case cancel }

    let text: String
    var note: String?
    var leading: CGFloat = MenuColumn.name
    let stopLabel: String
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
                    .accessibilityLabel(stopLabel)
            }
            .buttonStyle(CompactMenuButtonStyle())
        }
        .padding(.leading, leading)
        .padding(.trailing, 8)
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
    var leading: CGFloat = MenuColumn.name

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(Array(reasons.enumerated()), id: \.offset) { _, reason in
                Text(reason)
                    .font(.system(size: 12))
                    .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.leading, leading)
        .padding(.trailing, 8)
        .padding(.bottom, 4)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

enum MenuColumn {
    /// Where a row's name starts: 8 pt padding, the 14 pt lead and a 6 pt gap.
    static let name: CGFloat = 28
    /// Where a child-menu row's name starts: 8 pt padding, the 20 pt lead and a 6 pt gap.
    static let childName: CGFloat = 34
}

struct CompactMenuButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12))
            .foregroundStyle(Color(nsColor: .labelColor))
            .padding(.horizontal, 10)
            .padding(.vertical, 3)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(Color(nsColor: .labelColor).opacity(configuration.isPressed ? 0.22 : 0.12))
            )
    }
}
