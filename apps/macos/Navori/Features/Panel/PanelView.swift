import AppKit
import NavoriPanel
import SwiftUI

/// Content of the menubar panel.
struct PanelView: View {
    let store: CoreStore
    @Environment(CascadeController.self) private var cascade

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            banners
            MenuRowView(row: awakeRow, cascade: cascade)
            MenuRowView(row: coreRow, cascade: cascade)
            Divider()
            MenuRowView(row: settingsRow, cascade: cascade)
            MenuRowView(row: aboutRow, cascade: cascade)
            MenuRowView(row: quitRow, cascade: cascade)
        }
        .padding(5)
        .frame(width: 360, alignment: .topLeading)
        .fixedSize(horizontal: true, vertical: true)
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

    private var awakeRow: MenuRow {
        let state = AwakeRowState(status: store.awakeStatus)
        return MenuRow(
            id: "awake",
            icon: "cup.and.saucer",
            title: "Awake",
            value: state.label,
            dot: state.dot,
            opensSubmenu: state.hasSubmenu,
            shortcut: nil
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
