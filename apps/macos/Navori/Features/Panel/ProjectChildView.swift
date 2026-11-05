import AppKit
import NavoriPanel
import SwiftUI

/// The third level: one project's header, its sessions and ports, and the Stop controls.
struct ProjectChildView: View {
    static let width: CGFloat = 330

    let projectId: String
    let store: CoreStore
    @Environment(CascadeController.self) private var cascade

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let state = store.projectChild(id: projectId) {
                CappedScroll {
                    content(state)
                }
            }
        }
        .padding(5)
        .frame(width: Self.width, alignment: .topLeading)
        .fixedSize(horizontal: true, vertical: true)
        .onDisappear { store.dismissAskingStops() }
    }

    private func content(_ state: ProjectChildState) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            header(state.header)
            ForEach(state.sections, id: \.self) { section in
                MenuDivider()
                switch section {
                case .sessions:
                    ChildSectionTitle("Sessions")
                    ForEach(Array(state.sessions.enumerated()), id: \.offset) { _, session in
                        SessionRowView(state: session)
                    }
                case .ports:
                    ChildSectionTitle("Ports")
                    ForEach(state.ports, id: \.id) { port in
                        PortRowView(state: port, store: store, cascade: cascade)
                    }
                }
            }
            if let stop = state.stop {
                MenuDivider()
                StopLineView(
                    id: state.stopId,
                    title: stop.spoken,
                    confirmation: stop.confirmation,
                    detail: nil,
                    stopLabel: stop.spoken,
                    store: store,
                    cascade: cascade,
                    onConfirm: { store.confirmStop(state.stopId, offer: stop) }
                )
            }
            if let note = state.systemNote {
                Text(note)
                    .font(.system(size: 12))
                    .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    private func header(_ header: ProjectChildHeader) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(header.name)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(Color(nsColor: .labelColor))
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                if let up = header.up {
                    Text(up)
                        .font(.system(size: 12))
                        .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                        .lineLimit(1)
                        .fixedSize()
                        .layoutPriority(1)
                }
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel([header.name, header.up].compactMap { $0 }.joined(separator: ", "))
            .accessibilityAddTraits(.isHeader)
            if let path = header.path {
                pathLine(path, reveal: header.revealPath)
            }
        }
        .padding(.horizontal, 8)
        .padding(.top, 5)
        .padding(.bottom, 4)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
    }

    @ViewBuilder
    private func pathLine(_ path: String, reveal: String?) -> some View {
        let text = Text(path)
            .font(.system(size: 11.5, design: .monospaced))
            .foregroundStyle(Color(nsColor: .secondaryLabelColor))
            .lineLimit(1)
            .truncationMode(.middle)
        if let reveal {
            Button(action: {
                store.reveal(reveal)
            }) {
                text
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("Reveal in Finder")
            .accessibilityLabel("Reveal \(path) in Finder")
        } else {
            text
        }
    }
}

/// Fits its content, and scrolls only once the content is taller than `cap`.
/// The scroll view stays in place either way, so a row grown by a confirmation keeps its hover.
struct CappedScroll<Content: View>: View {
    static var cap: CGFloat { 440 }

    @ViewBuilder let content: Content
    @State private var height: CGFloat?

    var body: some View {
        ScrollView {
            content
                .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { height = $0 }
        }
        .scrollBounceBehavior(.basedOnSize)
        .frame(height: height.map { min($0, Self.cap) })
    }
}

struct MenuDivider: View {
    var body: some View {
        Divider()
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
    }
}

private struct ChildSectionTitle: View {
    let title: String

    init(_ title: String) {
        self.title = title
    }

    var body: some View {
        Text(title)
            .font(.system(size: 12, weight: .semibold))
            .foregroundStyle(Color(nsColor: .secondaryLabelColor))
            .padding(.horizontal, 8)
            .frame(maxWidth: .infinity, minHeight: 22, maxHeight: 22, alignment: .leading)
            .accessibilityAddTraits(.isHeader)
    }
}

private struct SessionRowView: View {
    let state: SessionRowState

    var body: some View {
        HStack(spacing: 6) {
            HarnessLead(mark: state.mark)
                .frame(width: 20, height: 20)
            Text(state.title)
                .font(.system(size: 14))
                .foregroundStyle(Color(nsColor: .labelColor))
                .lineLimit(1)
                .truncationMode(.tail)
                .layoutPriority(1)
            Spacer(minLength: 8)
            if state.showsDot {
                Circle()
                    .fill(statusDotColor(.green))
                    .frame(width: 6, height: 6)
                    .layoutPriority(2)
            }
            // Only the branch may shrink (in the middle); the title, dot and state word keep their room.
            if let branch = state.branch {
                Text(branch)
                    .font(.system(size: 13))
                    .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text("\u{00B7}")
                    .font(.system(size: 13))
                    .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                    .layoutPriority(2)
            }
            Text(state.state)
                .font(.system(size: 13))
                .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
                .layoutPriority(2)
        }
        .opacity(state.dimmed ? 0.45 : 1)
        .padding(.horizontal, 8)
        .frame(maxWidth: .infinity, minHeight: 26, maxHeight: 26, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(state.spoken)
    }
}

private struct HarnessLead: View {
    let mark: HoldingMark

    var body: some View {
        switch mark {
        case .claude: harness(HarnessMarkImage.claude)
        case .codex: harness(HarnessMarkImage.codex)
        case .grok: harness(HarnessMarkImage.grok)
        default: symbol("terminal")
        }
    }

    private func harness(_ image: NSImage) -> some View {
        Image(nsImage: image)
            .renderingMode(.template)
            .resizable()
            .frame(width: 14, height: 14)
            .foregroundStyle(Color(nsColor: .secondaryLabelColor))
    }

    private func symbol(_ name: String) -> some View {
        Image(systemName: name)
            .font(.system(size: 12))
            .symbolRenderingMode(.monochrome)
            .foregroundStyle(Color(nsColor: .secondaryLabelColor))
    }
}

private struct PortRowView: View {
    let state: PortRowState
    let store: CoreStore
    let cascade: CascadeController

    var body: some View {
        let selected = cascade.selection == state.id
        let flow = store.stopFlow(state.id)
        let entry = stopEntry
        let stopping = flow == .running || entry == .stopping
        let asking = isAsking(flow)
        VStack(alignment: .leading, spacing: 0) {
            ZStack(alignment: .trailing) {
                if asking, let stop = state.stop, let question = state.confirmation {
                    InlineConfirmRow(
                        message: question,
                        stopLabel: state.stopHelp ?? stop.spoken,
                        selected: selected,
                        onConfirm: { store.confirmStop(state.id, offer: stop) },
                        onCancel: { store.dismissStop(state.id) }
                    ) {
                        PortLead(mark: state.mark, selected: selected)
                            .frame(width: 20, height: 20)
                    }
                } else {
                    rowContent(selected: selected, hovering: selected, stopping: stopping)
                    trailing(selected: selected, stopping: stopping)
                }
            }
            .onGeometryChange(for: CGRect.self) { proxy in
                proxy.frame(in: .global)
            } action: { frame in
                cascade.setRowFrame(state.id, frame)
            }
            if let sentence = store.openFailures[state.id] {
                StopFailureLines(reasons: [sentence], leading: MenuColumn.childName)
            }
            if stopping == false, let reason = failure(entry: entry, flow: flow) {
                StopFailureLines(reasons: ["Stop failed. \(reason)"], leading: MenuColumn.childName)
            }
        }
        .onContinuousHover { phase in
            switch phase {
            case .active:
                cascade.pointer(on: state.id, at: NSEvent.mouseLocation)
            case .ended:
                cascade.pointerLeft(state.id)
            }
        }
        .onChange(of: selected) { _, isSelected in
            if isSelected == false {
                store.dismissStop(state.id)
                store.dismissOpenFailure(state.id)
            }
        }
    }

    private func failure(entry: StopProgress.Entry?, flow: StopFlow) -> String? {
        if case let .failed(reason) = entry { return reason }
        if case let .failed(reasons) = flow { return reasons.first }
        return nil
    }

    /// The name never moves: on hover the secondary text only fades, and the icons sit over its place.
    private func rowContent(selected: Bool, hovering: Bool, stopping: Bool) -> some View {
        let secondary = hovering || stopping ? 0.0 : 1.0
        return HStack(spacing: 6) {
            PortLead(mark: state.mark, selected: selected)
                .frame(width: 20, height: 20)
            Text(state.name)
                .font(.system(size: 14))
                .foregroundStyle(selected ? Color.white : Color(nsColor: .labelColor))
                .lineLimit(1)
                .truncationMode(.middle)
            if let suffix = state.suffix {
                Text("\u{00B7} \(suffix)")
                    .font(.system(size: 11.5))
                    .foregroundStyle(selected ? Color.white.opacity(0.8) : Color(nsColor: .secondaryLabelColor))
                    .lineLimit(1)
                    .fixedSize()
                    .layoutPriority(1)
                    .opacity(secondary)
            }
            Spacer(minLength: 8)
            Text(state.portText)
                .font(.system(size: 11.5, design: .monospaced))
                .foregroundStyle(selected ? Color.white.opacity(0.85) : Color(nsColor: .secondaryLabelColor))
                .lineLimit(1)
                .fixedSize()
                .layoutPriority(2)
                .opacity(secondary)
        }
        .padding(.horizontal, 8)
        .frame(maxWidth: .infinity, minHeight: 26, maxHeight: 26, alignment: .leading)
        .background {
            if selected {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(Color(nsColor: .selectedContentBackgroundColor))
            }
        }
        .contentShape(Rectangle())
        .help(state.tooltip ?? "")
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(state.spoken(for: stopping ? .stopping : stopEntry))
    }

    private var stopEntry: StopProgress.Entry? {
        state.target.flatMap { store.stopProgress.entry(for: $0) }
    }

    @ViewBuilder
    private func trailing(selected: Bool, stopping: Bool) -> some View {
        if stopping {
            Text("Stopping\u{2026}")
                .font(.system(size: 13))
                .lineLimit(1)
                .fixedSize()
                .foregroundStyle(selected ? Color.white : Color(nsColor: .secondaryLabelColor))
                .padding(.trailing, 8)
                .accessibilityHidden(true)
        } else if selected {
            HStack(spacing: 0) {
                if let help = state.openHelp {
                    RowActionButton(symbol: "arrow.up.right.square", label: help, selected: selected) {
                        Task { await store.openPort(row: state.id, port: state.port) }
                    }
                }
                if let stop = state.stop, let help = state.stopHelp {
                    RowActionButton(symbol: "stop.fill", label: help, selected: selected) {
                        store.askStop(state.id, count: stop.count)
                    }
                }
            }
            .padding(.trailing, 5)
        }
    }

    private func isAsking(_ flow: StopFlow) -> Bool {
        if case .asking = flow { true } else { false }
    }
}

private struct PortLead: View {
    let mark: PortMark
    let selected: Bool

    private var ink: Color {
        selected ? Color.white.opacity(0.85) : Color(nsColor: .tertiaryLabelColor)
    }

    var body: some View {
        switch mark {
        case .process:
            Text("\u{203A}_")
                .font(.system(size: 11, weight: .medium, design: .monospaced))
                .foregroundStyle(ink)
        case .container:
            symbol("square.inset.filled", size: 9)
        case .database:
            symbol("cylinder", size: 10)
        case .system:
            symbol("gearshape", size: 10)
        }
    }

    private func symbol(_ name: String, size: CGFloat) -> some View {
        Image(systemName: name)
            .font(.system(size: size))
            .symbolRenderingMode(.monochrome)
            .foregroundStyle(ink)
    }
}

/// A row that stops a group of ports: the label, then the one-line confirmation, then the failure lines.
struct StopLineView: View {
    let id: String
    let title: String
    let confirmation: String
    /// The longer sentence for the confirmation's tooltip and VoiceOver label.
    var detail: String?
    let stopLabel: String
    let store: CoreStore
    let cascade: CascadeController
    var narrow = false
    let onConfirm: () -> Void

    private var leading: CGFloat { narrow ? MenuColumn.name : MenuColumn.childName }

    var body: some View {
        let selected = cascade.selection == id
        let flow = store.stopFlow(id)
        VStack(alignment: .leading, spacing: 0) {
            switch flow {
            case .asking:
                InlineConfirmRow(
                    message: confirmation,
                    detail: detail,
                    stopLabel: stopLabel,
                    selected: selected,
                    onConfirm: onConfirm,
                    onCancel: { store.dismissStop(id) }
                ) {
                    stopMark(selected: selected)
                }
            case .running:
                label("Stopping\u{2026}", selected: selected)
            case let .failed(reasons):
                label(title, selected: selected)
                StopFailureLines(reasons: reasons, leading: leading)
            case .idle:
                label(title, selected: selected)
            }
        }
        .onContinuousHover { phase in
            switch phase {
            case .active:
                cascade.pointer(on: id, at: NSEvent.mouseLocation)
            case .ended:
                cascade.pointerLeft(id)
            }
        }
        .onChange(of: selected) { _, isSelected in
            if isSelected == false { store.dismissStop(id) }
        }
    }

    private func stopMark(selected: Bool) -> some View {
        Image(systemName: "stop.fill")
            .font(.system(size: 9))
            .foregroundStyle(selected ? Color.white : Color(nsColor: .secondaryLabelColor))
            .frame(width: narrow ? 14 : 20, height: 20)
    }

    private func label(_ text: String, selected: Bool) -> some View {
        Button(action: { cascade.click(id) }) {
            HStack(spacing: 6) {
                stopMark(selected: selected)
                Text(text)
                    .font(.system(size: 14))
                    .foregroundStyle(selected ? Color.white : Color(nsColor: .labelColor))
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            .padding(.horizontal, 8)
            .frame(maxWidth: .infinity, minHeight: 26, maxHeight: 26, alignment: .leading)
            .background {
                if selected {
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .fill(Color(nsColor: .selectedContentBackgroundColor))
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .focusEffectDisabled()
        .onGeometryChange(for: CGRect.self) { proxy in
            proxy.frame(in: .global)
        } action: { frame in
            cascade.setRowFrame(id, frame)
        }
        .accessibilityLabel(text)
        .accessibilityAddTraits(.isButton)
    }
}
