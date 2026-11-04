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
                ForEach(Array(state.items.enumerated()), id: \.offset) { _, item in
                    row(item, state: state)
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
            RunningSeparator()
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
            StopAllRowView(offer: offer, store: store, cascade: cascade)
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

private struct StopAllRowView: View {
    let offer: StopAllOffer
    let store: CoreStore
    let cascade: CascadeController

    var body: some View {
        let id = CoreStore.stopAllId
        let selected = cascade.selection == id
        let flow = store.stopFlow(id)
        VStack(alignment: .leading, spacing: 0) {
            switch flow {
            case .asking:
                InlineConfirmation(
                    text: offer.confirmation,
                    note: offer.systemNote,
                    stopLabel: offer.spoken,
                    onConfirm: { store.confirmStop(id, offer: offer) },
                    onCancel: { store.dismissStop(id) }
                )
            case .running:
                label("Stopping\u{2026}", selected: selected)
            case let .failed(reasons):
                label("Stop all ports\u{2026}", selected: selected)
                StopFailureLines(reasons: reasons)
            case .idle:
                label("Stop all ports\u{2026}", selected: selected)
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

    private func label(_ title: String, selected: Bool) -> some View {
        Button(action: { cascade.click(CoreStore.stopAllId) }) {
            HStack(spacing: 6) {
                Image(systemName: "stop.fill")
                    .font(.system(size: 9))
                    .foregroundStyle(selected ? Color.white : Color(nsColor: .secondaryLabelColor))
                    .frame(width: 14, height: 20)
                Text(title)
                    .font(.system(size: 14))
                    .foregroundStyle(selected ? Color.white : Color(nsColor: .labelColor))
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
            cascade.setRowFrame(CoreStore.stopAllId, frame)
        }
        .accessibilityLabel(title)
        .accessibilityAddTraits(.isButton)
    }
}
