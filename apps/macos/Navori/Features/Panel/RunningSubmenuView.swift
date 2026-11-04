import AppKit
import NavoriPanel
import SwiftUI

struct RunningSubmenuView: View {
    private static let rowHeight: CGFloat = 26

    let store: CoreStore
    @Environment(CascadeController.self) private var cascade

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let state = store.runningSubmenu {
                ForEach(Array(shownItems(state).enumerated()), id: \.offset) { _, item in
                    row(item, state: state)
                }
            }
        }
        .padding(5)
        .frame(width: 272, alignment: .topLeading)
        .fixedSize(horizontal: true, vertical: true)
    }

    private func shownItems(_ state: RunningSubmenuState) -> [RunningSubmenuItem] {
        var items = state.items.filter { item in
            if case .stopAll = item { return false }
            return true
        }
        if case .separator = items.last { items.removeLast() }
        return items
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
        case .separator:
            RunningSeparator()
        case .nothingRunning:
            Text("Nothing running")
                .font(.system(size: 14))
                .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                .padding(.horizontal, 8)
                .frame(maxWidth: .infinity, minHeight: Self.rowHeight, alignment: .leading)
                .accessibilityAddTraits(.isStaticText)
        case let .project(project):
            ProjectRowView(state: project, cascade: cascade)
        case .stopAll:
            EmptyView()
        }
    }

    @ViewBuilder
    private func note(_ text: String) -> some View {
        if store.running?.unknownReason == text {
            Text(text)
                .font(.system(size: 12))
                .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityLabel(text)
        } else if let problem = store.awakeSubmenu.problem {
            AwakeProblemView(problem: problem)
        }
    }
}

private struct RunningSeparator: View {
    var body: some View {
        Divider()
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
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
