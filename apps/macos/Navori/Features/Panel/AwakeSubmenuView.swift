import AppKit
import NavoriPanel
import SwiftUI

struct AwakeSubmenuView: View {
    let store: CoreStore
    @Environment(CascadeController.self) private var cascade

    var body: some View {
        let state = store.awakeSubmenu
        VStack(alignment: .leading, spacing: 0) {
            MenuRowView(
                row: MenuRow(
                    id: "awake.keep",
                    icon: "timer",
                    title: "Keep awake",
                    value: state.keepValue,
                    dot: nil,
                    opensSubmenu: true,
                    shortcut: nil
                ),
                cascade: cascade
            )
            MenuRowView(
                row: MenuRow(
                    id: "awake.tracking",
                    icon: "sparkles",
                    title: "Tracking agents",
                    value: state.trackingValue,
                    dot: state.trackingDot,
                    opensSubmenu: false,
                    shortcut: nil
                ),
                cascade: cascade
            )
            if let problem = state.problem {
                MenuSeparator()
                AwakeProblemView(problem: problem)
            }
            if state.holdingGroups.isEmpty == false {
                MenuSeparator()
                HoldingList(groups: state.holdingGroups)
            }
        }
        .padding(5)
        .frame(width: 360, alignment: .topLeading)
        .fixedSize(horizontal: true, vertical: true)
    }
}

/// The third level: how long to keep the Mac awake.
struct KeepAwakeSubmenuView: View {
    let store: CoreStore
    @Environment(CascadeController.self) private var cascade

    var body: some View {
        let state = store.awakeSubmenu
        VStack(alignment: .leading, spacing: 0) {
            ForEach(KeepAwakeChoice.all, id: \.id) { choice in
                MenuRowView(
                    row: MenuRow(
                        id: choice.id,
                        icon: nil,
                        title: choice.title,
                        value: nil,
                        dot: nil,
                        opensSubmenu: false,
                        shortcut: nil,
                        isChecked: state.isChecked(choice)
                    ),
                    cascade: cascade
                )
            }
        }
        .padding(5)
        .frame(width: 180, alignment: .topLeading)
        .fixedSize(horizontal: true, vertical: true)
    }
}

private struct MenuSeparator: View {
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

/// Fixed heights make the list's size known without measuring. It scrolls only when it overflows.
private struct HoldingList: View {
    private static let titleHeight: CGFloat = 26
    private static let rowHeight: CGFloat = 26
    private static let maxHeight: CGFloat = 344

    let groups: [HoldingGroup]

    private var height: CGFloat {
        let rows = groups.reduce(0) { $0 + $1.rows.count }
        return CGFloat(groups.count) * Self.titleHeight + CGFloat(rows) * Self.rowHeight
    }

    var body: some View {
        if height > Self.maxHeight {
            ScrollView { content }
                .frame(height: Self.maxHeight)
        } else {
            content
        }
    }

    private var content: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(groups.enumerated()), id: \.offset) { _, group in
                Text(group.title)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                    .lineLimit(1)
                    .padding(.horizontal, 8)
                    .padding(.bottom, 3)
                    .frame(maxWidth: .infinity, minHeight: Self.titleHeight, maxHeight: Self.titleHeight, alignment: .bottomLeading)
                    .accessibilityAddTraits(.isHeader)
                ForEach(Array(group.rows.enumerated()), id: \.offset) { _, row in
                    HoldingLine(row: row, height: Self.rowHeight)
                }
            }
        }
    }
}

private struct HoldingLine: View {
    let row: HoldingRow
    let height: CGFloat

    private var ink: Color {
        Color(nsColor: row.isQuiet ? .secondaryLabelColor : .labelColor)
    }

    var body: some View {
        HStack(spacing: 6) {
            HoldingMarkView(mark: row.mark)
                .foregroundStyle(ink)
                .frame(width: 20, height: 20)
            Text(row.name)
                .font(.system(size: 14))
                .foregroundStyle(ink)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 8)
            if row.showsDot {
                Circle()
                    .fill(statusDotColor(.green))
                    .frame(width: 6, height: 6)
            }
            if row.word.isEmpty == false {
                Text(row.word)
                    .font(.system(size: 12))
                    .foregroundStyle(ink)
                    .lineLimit(1)
            }
        }
        .padding(.horizontal, 8)
        .frame(maxWidth: .infinity, minHeight: height, maxHeight: height, alignment: .leading)
        .help(row.help ?? "")
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(row.accessibilitySentence)
    }
}

private struct HoldingMarkView: View {
    let mark: HoldingMark

    var body: some View {
        switch mark {
        case .claude: harness(HarnessMarkImage.claude)
        case .codex: harness(HarnessMarkImage.codex)
        case .grok: harness(HarnessMarkImage.grok)
        case .terminal: symbol("terminal")
        case .clock: symbol("clock")
        case .infinity: symbol("infinity")
        case .display: symbol("display")
        }
    }

    private func harness(_ image: NSImage) -> some View {
        Image(nsImage: image)
            .renderingMode(.template)
            .resizable()
            .frame(width: 16, height: 16)
    }

    private func symbol(_ name: String) -> some View {
        Image(systemName: name)
            .font(.system(size: 13))
            .symbolRenderingMode(.monochrome)
    }
}
