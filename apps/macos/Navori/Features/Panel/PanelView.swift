import SwiftUI

/// Content of the menubar panel.
struct PanelView: View {
    let store: CoreStore
    let openSettings: (SettingsTab) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider().padding(.horizontal, 12)
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    banners
                    if store.hasSnapshot {
                        ModulesSection(modules: store.moduleRows)
                        ActivitySection(events: store.eventRows)
                    }
                }
                .padding(14)
            }
            .frame(maxHeight: 440)
            Divider().padding(.horizontal, 12)
            footer
        }
        .frame(width: 360)
    }

    private var header: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text("Navori").font(.headline)
            if AppEnvironment.isDev {
                Text("DEV")
                    .font(.caption2.weight(.bold))
                    .padding(.horizontal, 5)
                    .padding(.vertical, 1)
                    .background(.orange.opacity(0.25), in: Capsule())
            }
            Spacer()
            Text("v\(AppEnvironment.version)")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
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

    private var footer: some View {
        HStack {
            Button("Settings…") { openSettings(.general) }
                .keyboardShortcut(",", modifiers: .command)
            Spacer()
            Button("Quit") { NSApp.terminate(nil) }
                .keyboardShortcut("q", modifiers: .command)
        }
        .buttonStyle(.borderless)
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
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
