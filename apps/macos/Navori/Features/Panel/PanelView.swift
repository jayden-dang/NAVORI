import AppKit
import NavoriPanel
import SwiftUI

struct PanelView: View {
    let store: CoreStore
    @Environment(CascadeController.self) private var cascade

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            banners
            decideBlock
            MenuRowView(row: needsYouRow, cascade: cascade)
            Divider()
            MenuRowView(row: runningRow, cascade: cascade)
            if store.coreRowVisible {
                MenuRowView(row: coreRow, cascade: cascade)
            }
            Divider()
            MenuRowView(row: settingsRow, cascade: cascade)
            MenuRowView(row: aboutRow, cascade: cascade)
            MenuRowView(row: quitRow, cascade: cascade)
        }
        .padding(MenuMetrics.pad)
        .frame(width: MenuMetrics.level0Width, alignment: .topLeading)
        .fixedSize(horizontal: true, vertical: true)
    }

    @ViewBuilder
    private var decideBlock: some View {
        let needs = store.needsYou
        let down = needs?.down == true
        let decisions = needs?.decisions ?? []
        if !decisions.isEmpty {
            DecideHeaderView(count: decisions.count, cascade: cascade)
            ForEach(decisions, id: \.id) { row in
                DecideRowView(row: row, cascade: cascade, store: store, isDimmed: down)
            }
        } else if needs?.allCaughtUp == true, !down {
            Text("✓ You're all caught up")
                .font(.system(size: 14))
                .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                .padding(.horizontal, 8)
                .frame(
                    maxWidth: .infinity,
                    minHeight: MenuMetrics.rowHeight,
                    maxHeight: MenuMetrics.rowHeight,
                    alignment: .leading
                )
                .accessibilityLabel("You're all caught up")
        }
    }

    private var needsYouRow: MenuRow {
        let needs = store.needsYou
        let count = needs?.needsCount ?? 0
        let down = needs?.down == true
        return MenuRow(
            id: "needs",
            icon: "sparkle",
            title: "Needs you",
            value: count == 0 ? nil : "\(count)",
            dot: needs?.anyOverdue == true ? .red : nil,
            opensSubmenu: true,
            shortcut: nil,
            isDimmed: down
        )
    }

    @ViewBuilder
    private var banners: some View {
        if let error = store.startupError {
            Banner(style: .error, title: "The core did not start", message: error)
        }
        if let unclean = store.uncleanExit {
            Banner(style: .warning, title: "Navori quit unexpectedly last time", message: unclean)
        }
        if let problem = store.configProblem {
            Banner(style: .warning, title: problem.title, message: problem.message)
        }
    }

    private var runningRow: MenuRow {
        let state = RunningRowState(running: store.running)
        return MenuRow(
            id: "running",
            icon: "cup.and.saucer",
            title: state.label,
            value: nil,
            dot: state.dot,
            opensSubmenu: state.hasSubmenu,
            shortcut: nil,
            spokenLabel: state.spoken
        )
    }

    private var coreRow: MenuRow {
        let state = CoreRowState(status: store.coreStatus)
        return MenuRow(
            id: "core",
            icon: "sparkle",
            title: "Core",
            value: state.label,
            dot: state.dot,
            opensSubmenu: state.hasSubmenu,
            shortcut: nil
        )
    }

    private var settingsRow: MenuRow {
        MenuRow(
            id: "settings",
            icon: "gearshape",
            title: "Settings…",
            value: nil,
            dot: nil,
            opensSubmenu: false,
            shortcut: ","
        )
    }

    private var aboutRow: MenuRow {
        MenuRow(
            id: "about",
            icon: nil,
            title: "About Navori",
            value: nil,
            dot: nil,
            opensSubmenu: false,
            shortcut: nil
        )
    }

    private var quitRow: MenuRow {
        MenuRow(
            id: "quit",
            icon: nil,
            title: "Quit Navori",
            value: nil,
            dot: nil,
            opensSubmenu: false,
            shortcut: "q"
        )
    }
}

struct SectionTitle: View {
    let text: String
    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text.uppercased())
            .font(.caption2.weight(.semibold))
            .foregroundStyle(.secondary)
            .tracking(0.6)
    }
}

struct Banner: View {
    enum Style { case warning, error }
    let style: Style
    let title: String
    let message: String

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Label(title, systemImage: style == .error ? "xmark.octagon.fill" : "exclamationmark.triangle.fill")
                .font(.callout.weight(.semibold))
                .foregroundStyle(style == .error ? .red : .orange)
            Text(message)
                .font(.caption)
                .foregroundStyle(Color(nsColor: .labelColor))
                .lineLimit(4)
                .textSelection(.enabled)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(10)
        .background((style == .error ? Color.red : Color.orange).opacity(0.10), in: RoundedRectangle(cornerRadius: 8))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .accessibilityValue(message)
    }
}
