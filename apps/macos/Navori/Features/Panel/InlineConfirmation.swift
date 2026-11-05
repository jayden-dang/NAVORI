import SwiftUI

/// One confirmation, on one 26 pt row: a lead, the question, a flat `Cancel` and a flat `Stop`.
/// Shared by a single port's Stop, `Stop N ports` and `Stop all ports…`.
struct InlineConfirmRow<Lead: View>: View {
    private enum Field { case cancel }

    let message: String
    /// The tooltip and the VoiceOver sentence; defaults to the message.
    var detail: String?
    let stopLabel: String
    let selected: Bool
    let onConfirm: () -> Void
    let onCancel: () -> Void
    @ViewBuilder let lead: Lead
    @FocusState private var focus: Field?

    var body: some View {
        HStack(spacing: 6) {
            lead
            Text(message)
                .font(.system(size: 13))
                .foregroundStyle(selected ? Color.white : Color(nsColor: .labelColor))
                .lineLimit(1)
                .truncationMode(.tail)
                .help(detail ?? "")
                .accessibilityLabel(detail ?? message)
            Spacer(minLength: 0)
            ConfirmTextButton(title: "Cancel", weight: .regular, ink: cancelInk, action: onCancel)
                .focused($focus, equals: .cancel)
            ConfirmTextButton(title: "Stop", weight: .semibold, ink: stopInk, action: onConfirm)
                .accessibilityLabel(stopLabel)
        }
        .padding(.horizontal, 8)
        .frame(maxWidth: .infinity, minHeight: 26, maxHeight: 26, alignment: .leading)
        .background {
            if selected {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(Color(nsColor: .selectedContentBackgroundColor))
            }
        }
        .accessibilityElement(children: .contain)
        .onExitCommand(perform: onCancel)
        .onAppear { focus = .cancel }
    }

    private var cancelInk: Color {
        selected ? Color.white.opacity(0.85) : Color(nsColor: .secondaryLabelColor)
    }

    /// The one use of red in the menu: the destructive confirm (running-menu brief, Surface 3).
    private var stopInk: Color {
        selected ? Color.white : Color(nsColor: .systemRed)
    }
}

private struct ConfirmTextButton: View {
    let title: String
    let weight: Font.Weight
    let ink: Color
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 13, weight: weight))
                .foregroundStyle(ink)
                .padding(.horizontal, 5)
                .frame(height: 20)
                .background(
                    RoundedRectangle(cornerRadius: 5, style: .continuous)
                        .fill(Color(nsColor: .labelColor).opacity(hovering ? 0.12 : 0))
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .focusEffectDisabled()
        .onHover { hovering = $0 }
    }
}

/// A small borderless icon action on a row: 22 pt hit target, a rounded highlight on hover.
struct RowActionButton: View {
    let symbol: String
    /// The tooltip and the VoiceOver label.
    let label: String
    let selected: Bool
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 12))
                .symbolRenderingMode(.monochrome)
                .foregroundStyle(selected ? Color.white : Color(nsColor: .secondaryLabelColor))
                .frame(width: 22, height: 22)
                .background(
                    RoundedRectangle(cornerRadius: 5, style: .continuous)
                        .fill(Color(nsColor: .labelColor).opacity(hovering ? 0.2 : 0))
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .focusEffectDisabled()
        .onHover { hovering = $0 }
        .help(label)
        .accessibilityLabel(label)
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
