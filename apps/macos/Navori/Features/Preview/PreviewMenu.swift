#if DEBUG
import AppKit
import NavoriPanel
import SwiftUI

@MainActor
enum PreviewMenu {
    static func content(
        setTint: @escaping (NSColor) -> Void,
        setIconSample: @escaping (StatusIconState?) -> Void,
        playBeat: @escaping () -> Void
    ) -> CascadeContent {
        let marker = PreviewSampleRows.marker
        return CascadeContent(
            tree: { tree },
            root: AnyView(PreviewRootView(setTint: setTint, setIconSample: setIconSample, playBeat: playBeat).accessibilityIdentifier(marker)),
            submenu: { id in AnyView(PreviewSubmenuView(id: id)) },
            activate: { _ in },
            caughtUp: { false }
        )
    }

    static var tree: [CascadeNode] {
        [
            CascadeNode(id: "preview.inbox.stop3", isActionable: true),
            CascadeNode(id: "preview.inbox.keep", isActionable: true),
            CascadeNode(
                id: "preview.awake",
                isActionable: true,
                children: [
                    CascadeNode(id: "preview.awake.keep1h", isActionable: true),
                    CascadeNode(id: "preview.awake.keepOn", isActionable: true),
                    CascadeNode(id: "preview.awake.sleep", isActionable: true),
                    CascadeNode(
                        id: "preview.awake.sessions",
                        isActionable: true,
                        children: [
                            CascadeNode(id: "preview.awake.sessions.claude", isActionable: true),
                            CascadeNode(id: "preview.awake.sessions.codex", isActionable: true),
                        ]
                    ),
                ]
            ),
            CascadeNode(
                id: "preview.ports",
                isActionable: true,
                children: [
                    CascadeNode(id: "preview.ports.openAll", isActionable: true),
                    CascadeNode(id: "preview.ports.stopAll", isActionable: true),
                ]
            ),
            CascadeNode(
                id: "preview.prs",
                isActionable: true,
                children: [
                    CascadeNode(id: "preview.prs.open", isActionable: true),
                    CascadeNode(id: "preview.prs.notify", isActionable: true),
                ]
            ),
        ]
    }
}

private struct PreviewRootView: View {
    let setTint: (NSColor) -> Void
    let setIconSample: (StatusIconState?) -> Void
    let playBeat: () -> Void
    @Environment(CascadeController.self) private var cascade
    @State private var ask = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            askField
            Divider()
            needsYou
            Divider()
            MenuRowView(row: awakeRow, cascade: cascade)
            MenuRowView(row: portsRow, cascade: cascade)
            MenuRowView(row: prsRow, cascade: cascade)
            Divider()
            PreviewTintRow(setTint: setTint)
            Divider()
            PreviewIconSampleList(setIconSample: setIconSample, playBeat: playBeat)
        }
        .padding(5)
        .frame(width: 360, alignment: .topLeading)
        .fixedSize(horizontal: true, vertical: true)
        .accessibilityIdentifier(PreviewSampleRows.marker)
    }

    private var askField: some View {
        HStack(spacing: 8) {
            Image(systemName: "sparkle")
                .font(.system(size: 13))
                .symbolRenderingMode(.monochrome)
                .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                .frame(width: 20, height: 20)
            TextField("Ask Navori…", text: $ask)
                .textFieldStyle(.plain)
                .font(.system(size: 14))
            Text("⌥ Space")
                .font(.system(size: 13))
                .foregroundStyle(Color(nsColor: .secondaryLabelColor))
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(
            Color.primary.opacity(0.06),
            in: RoundedRectangle(cornerRadius: 8, style: .continuous)
        )
    }

    private var needsYou: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack {
                Text("Needs you")
                    .font(.system(size: 14))
                    .foregroundStyle(Color(nsColor: .labelColor))
                Spacer(minLength: 8)
                Text("\(PreviewSampleRows.inbox.count)")
                    .font(.system(size: 13))
                    .foregroundStyle(Color(nsColor: .secondaryLabelColor))
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            ForEach(PreviewSampleRows.inbox) { item in
                InboxRowView(item: item, cascade: cascade)
            }
        }
    }

    private var awakeRow: MenuRow {
        MenuRow(
            id: "preview.awake",
            icon: "cup.and.saucer",
            title: "Awake",
            value: "On · Claude, Codex",
            dot: .green,
            opensSubmenu: true,
            shortcut: nil
        )
    }

    private var portsRow: MenuRow {
        MenuRow(
            id: "preview.ports",
            icon: "point.3.connected.trianglepath.dotted",
            title: "Ports",
            value: "17 open",
            dot: nil,
            opensSubmenu: true,
            shortcut: nil
        )
    }

    private var prsRow: MenuRow {
        MenuRow(
            id: "preview.prs",
            icon: "arrow.triangle.pull",
            title: "Pull Requests",
            value: "1 to review · 2 failing",
            dot: .red,
            opensSubmenu: true,
            shortcut: nil
        )
    }
}

private struct PreviewTintRow: View {
    let setTint: (NSColor) -> Void
    @Environment(\.colorScheme) private var colorScheme
    @State private var percent: Double = 69

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text("Tint")
                    .font(.system(size: 14))
                    .foregroundStyle(Color(nsColor: .labelColor))
                Spacer(minLength: 8)
                Text("\(Int(percent.rounded()))%")
                    .font(.system(size: 13))
                    .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                    .monospacedDigit()
            }
            Slider(value: $percent, in: 20...90, step: 1)
                .controlSize(.small)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .onAppear {
            percent = colorScheme == .dark ? 69 : 57
            apply(percent)
        }
        .onChange(of: percent) { _, value in
            apply(value)
        }
    }

    private func apply(_ percent: Double) {
        let alpha = percent / 100
        let color: NSColor = colorScheme == .dark
            ? NSColor.black.withAlphaComponent(alpha)
            : NSColor.white.withAlphaComponent(alpha)
        setTint(color)
    }
}

private struct PreviewSubmenuView: View {
    let id: String
    @Environment(CascadeController.self) private var cascade

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            switch id {
            case "preview.awake":
                awake
            case "preview.awake.sessions":
                sessions
            case "preview.ports":
                ports
            case "preview.prs":
                pullRequests
            default:
                EmptyView()
            }
        }
        .padding(5)
        .frame(width: 360, alignment: .topLeading)
        .fixedSize(horizontal: true, vertical: true)
        .accessibilityIdentifier(PreviewSampleRows.marker)
    }

    private var awake: some View {
        VStack(alignment: .leading, spacing: 0) {
            MenuRowView(row: command("preview.awake.keep1h", "Keep awake for 1 hour"), cascade: cascade)
            MenuRowView(row: command("preview.awake.keepOn", "Keep awake until I turn it off"), cascade: cascade)
            MenuRowView(row: command("preview.awake.sleep", "Let my Mac sleep now"), cascade: cascade)
            VStack(alignment: .leading, spacing: 2) {
                Text("Awake · since 11:38")
                    .font(.system(size: 14))
                    .foregroundStyle(Color(nsColor: .labelColor))
                Text("3 leases · system sleep blocked · display may sleep")
                    .font(.system(size: 12))
                    .foregroundStyle(Color(nsColor: .secondaryLabelColor))
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 8)
            MenuRowView(
                row: MenuRow(
                    id: "preview.awake.sessions",
                    icon: nil,
                    title: "Agent sessions",
                    value: "6",
                    dot: nil,
                    opensSubmenu: true,
                    shortcut: nil
                ),
                cascade: cascade
            )
        }
    }

    private var sessions: some View {
        VStack(alignment: .leading, spacing: 0) {
            session(id: "preview.awake.sessions.claude", title: "Claude · NAVORI/main", detail: "working · pid 23719")
            session(id: "preview.awake.sessions.codex", title: "Codex · cargo-zip", detail: "working · pid 24002")
        }
    }

    private var ports: some View {
        VStack(alignment: .leading, spacing: 0) {
            MenuRowView(row: command("preview.ports.openAll", "Open all web servers", value: "5"), cascade: cascade)
            MenuRowView(row: command("preview.ports.stopAll", "Stop all ports…"), cascade: cascade)
        }
    }

    private var pullRequests: some View {
        VStack(alignment: .leading, spacing: 0) {
            MenuRowView(row: command("preview.prs.open", "Open pull requests on GitHub"), cascade: cascade)
            MenuRowView(row: command("preview.prs.notify", "Open notifications", value: "4"), cascade: cascade)
        }
    }

    private func command(_ id: String, _ title: String, value: String? = nil) -> MenuRow {
        MenuRow(
            id: id,
            icon: nil,
            title: title,
            value: value,
            dot: nil,
            opensSubmenu: false,
            shortcut: nil
        )
    }

    private func session(id: String, title: String, detail: String) -> some View {
        let selected = cascade.selection == id
        return VStack(alignment: .leading, spacing: 1) {
            HStack {
                Text(title)
                    .font(.system(size: 14))
                    .foregroundStyle(selected ? Color.white : Color(nsColor: .labelColor))
                    .lineLimit(1)
                Spacer(minLength: 8)
            }
            Text(detail)
                .font(.system(size: 11.5, design: .monospaced))
                .foregroundStyle(selected ? Color.white.opacity(0.9) : Color(nsColor: .secondaryLabelColor))
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .frame(maxWidth: .infinity, minHeight: 36, alignment: .leading)
        .background {
            if selected {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(Color(nsColor: .selectedContentBackgroundColor))
            }
        }
        .contentShape(Rectangle())
        .onTapGesture { cascade.click(id) }
        .onContinuousHover { phase in
            switch phase {
            case .active:
                cascade.pointer(on: id, at: NSEvent.mouseLocation)
            case .ended:
                cascade.pointerLeft(id)
            }
        }
        .onGeometryChange(for: CGRect.self) { proxy in
            proxy.frame(in: .global)
        } action: { frame in
            cascade.setRowFrame(id, frame)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .accessibilityValue(detail)
        .accessibilityIdentifier(PreviewSampleRows.marker)
    }
}
#endif
