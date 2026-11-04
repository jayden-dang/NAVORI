import AppKit
import NavoriPanel
import SwiftUI

struct RunningSubmenuView: View {
    private static let rowHeight: CGFloat = 26

    let store: CoreStore
    @Environment(CascadeController.self) private var cascade

    var body: some View {
        CappedScroll {
            VStack(alignment: .leading, spacing: 0) {
                if let state = store.runningSubmenu {
                    ForEach(Array(state.items.enumerated()), id: \.offset) { _, item in
                        row(item, state: state)
                    }
                }
            }
        }
        .padding(5)
        .frame(width: 272, alignment: .topLeading)
        .fixedSize(horizontal: true, vertical: true)
        .onDisappear { store.dismissAskingStops() }
    }

    @ViewBuilder
    private func row(_ item: RunningSubmenuItem, state: RunningSubmenuState) -> some View {
        let awake = store.awakeSubmenu
        switch item {
        case .keepAwake:
            MenuRowView(
                row: MenuRow(
                    id: "awake.keep",
                    icon: "timer",
                    title: "Keep awake",
                    value: awake.keepValue,
                    dot: nil,
                    opensSubmenu: true,
                    shortcut: nil
                ),
                cascade: cascade
            )
        case .trackingAgents:
            MenuRowView(
                row: MenuRow(
                    id: "awake.tracking",
                    icon: "sparkles",
                    title: "Tracking agents",
                    value: awake.trackingValue,
                    dot: awake.trackingDot,
                    opensSubmenu: false,
                    shortcut: nil
                ),
                cascade: cascade
            )
        case let .note(text):
            note(text)
        case .problem:
            if let problem = awake.problem {
                AwakeProblemView(problem: problem)
            }
        case .separator:
            MenuDivider()
        case .nothingRunning:
            Text("Nothing running")
                .font(.system(size: 14))
                .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                .padding(.horizontal, 8)
                .frame(maxWidth: .infinity, minHeight: Self.rowHeight, alignment: .leading)
                .accessibilityAddTraits(.isStaticText)
        case let .project(project):
            ProjectRowView(state: project, targets: project.targets, store: store, cascade: cascade)
        case let .stopAll(offer):
            StopLineView(
                id: CoreStore.stopAllId,
                title: "Stop all ports\u{2026}",
                confirmation: offer.confirmation,
                note: offer.systemNote,
                stopLabel: offer.spoken,
                store: store,
                cascade: cascade,
                narrow: true,
                onConfirm: { store.confirmStop(CoreStore.stopAllId, offer: offer) }
            )
        case let .commands(rows):
            ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                CommandRowView(row: row)
            }
        }
    }

    private func note(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 12))
            .foregroundStyle(Color(nsColor: .secondaryLabelColor))
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityLabel(text)
    }
}

private struct AwakeProblemView: View {
    let problem: AwakeProblem

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Circle()
                .fill(statusDotColor(.orange))
                .frame(width: 7, height: 7)
                .alignmentGuide(.firstTextBaseline) { $0[.bottom] - 1 }
            VStack(alignment: .leading, spacing: 2) {
                Text(problem.title)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(Color(nsColor: .labelColor))
                if let detail = problem.detail {
                    Text(detail)
                        .font(.system(size: 12))
                        .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                }
            }
            .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(problem.title)
        .accessibilityValue(problem.detail ?? "")
    }
}

private struct CommandRowView: View {
    let row: HoldingRow

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "terminal")
                .font(.system(size: 12))
                .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                .frame(width: 14, height: 20)
            Text(row.name)
                .font(.system(size: 14))
                .foregroundStyle(Color(nsColor: .labelColor))
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 8)
            Text(row.word)
                .font(.system(size: 12))
                .foregroundStyle(Color(nsColor: .secondaryLabelColor))
        }
        .padding(.horizontal, 8)
        .frame(maxWidth: .infinity, minHeight: 26, maxHeight: 26, alignment: .leading)
        .help(row.help ?? "")
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(row.accessibilitySentence)
    }
}
