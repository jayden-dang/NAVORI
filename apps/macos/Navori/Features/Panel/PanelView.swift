import NavoriPanel
import SwiftUI

/// Content of the menubar panel.
struct PanelView: View {
    let store: CoreStore
    @Environment(CascadeController.self) private var cascade

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            banners
            MenuRowView(row: coreRow, cascade: cascade)
            Divider()
            MenuRowView(row: settingsRow, cascade: cascade)
            MenuRowView(row: aboutRow, cascade: cascade)
            MenuRowView(row: quitRow, cascade: cascade)
        }
        .padding(5)
        .frame(width: 360)
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

    private var coreRow: MenuRow {
        let state = CoreRowState(status: store.coreStatus)
        return MenuRow(
            id: "core",
            icon: nil,
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

private struct ModulesSection: View {
    let modules: [ModuleRow]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            SectionTitle("Core")
            ForEach(modules, id: \.id) { module in
                HStack(spacing: 8) {
                    Circle().fill(color(module.state)).frame(width: 7, height: 7)
                    Text(module.id).font(.callout)
                    Spacer()
                    Text(label(module)).font(.caption).foregroundStyle(.secondary)
                }
                .help(module.detail ?? module.state)
            }
        }
    }

    private func color(_ state: String) -> Color {
        switch state {
        case "running": .green
        case "starting", "restarting": .yellow
        case "failed": .red
        default: .gray
        }
    }

    private func label(_ m: ModuleRow) -> String {
        m.restarts > 0 ? "\(m.state) · \(m.restarts) restarts" : m.state
    }
}

private struct ActivitySection: View {
    let events: [EventRow]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            SectionTitle("Activity")
            if events.isEmpty {
                Text("Nothing yet.").font(.callout).foregroundStyle(.secondary)
            }
            ForEach(events.prefix(8), id: \.id) { event in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Image(systemName: icon(event.severity))
                        .foregroundStyle(tint(event.severity))
                        .font(.caption)
                        .frame(width: 12)
                    Text(event.summary).font(.callout).lineLimit(2)
                    Spacer(minLength: 4)
                    Text(Self.relative(event.tsMs)).font(.caption2).foregroundStyle(.tertiary)
                }
            }
        }
    }

    private func icon(_ severity: String) -> String {
        switch severity {
        case "error": "xmark.octagon.fill"
        case "warn": "exclamationmark.triangle.fill"
        default: "circle.fill"
        }
    }

    private func tint(_ severity: String) -> Color {
        switch severity {
        case "error": .red
        case "warn": .orange
        default: .secondary
        }
    }

    private static func relative(_ ms: Int64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(ms) / 1000)
        return date.formatted(.relative(presentation: .numeric, unitsStyle: .narrow))
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
                .foregroundStyle(.secondary)
                .lineLimit(4)
                .textSelection(.enabled)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(10)
        .background((style == .error ? Color.red : Color.orange).opacity(0.10), in: RoundedRectangle(cornerRadius: 8))
    }
}
