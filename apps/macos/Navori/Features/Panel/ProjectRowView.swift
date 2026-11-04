import AppKit
import NavoriPanel
import SwiftUI

struct ProjectRowView: View {
    let state: ProjectRowState
    let targets: [StopTarget]
    let store: CoreStore
    let cascade: CascadeController

    var body: some View {
        let selected = cascade.selection == state.id
        let flow = store.stopFlow(state.id)
        let stopping = flow == .running || store.stopProgress.isStopping(among: targets)
        VStack(alignment: .leading, spacing: 0) {
            ZStack(alignment: .trailing) {
                Button(action: { cascade.click(state.id) }) {
                    HStack(spacing: 6) {
                        ProjectDotView(dot: state.dot)
                            .frame(width: 14, height: 20)
                        Text(state.name)
                            .font(.system(size: 14))
                            .foregroundStyle(selected ? Color.white : Color(nsColor: .labelColor))
                            .lineLimit(1)
                            .truncationMode(.middle)
                        Spacer(minLength: 8)
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
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(state.spoken)
                .accessibilityAddTraits(.isButton)
                trailing(selected: selected, flow: flow, stopping: stopping)
            }
            .onGeometryChange(for: CGRect.self) { proxy in
                proxy.frame(in: .global)
            } action: { frame in
                cascade.setRowFrame(state.id, frame)
            }
            if case .asking(let count) = flow, let stop = state.stop {
                InlineConfirmation(
                    text: stop.confirmation,
                    onConfirm: { store.confirmStop(state.id, requests: [stop.request], count: count) },
                    onCancel: { store.dismissStop(state.id) }
                )
            }
            if case .failed(let reasons) = flow {
                StopFailureLines(reasons: reasons)
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
            if isSelected == false { store.dismissStop(state.id) }
        }
    }

    @ViewBuilder
    private func trailing(selected: Bool, flow: StopFlow, stopping: Bool) -> some View {
        if stopping {
            Text("Stopping\u{2026}")
                .font(.system(size: 13))
                .foregroundStyle(selected ? Color.white : Color(nsColor: .secondaryLabelColor))
                .padding(.trailing, 8)
                .accessibilityLabel("\(state.name), stopping")
        } else if selected, let stop = state.stop, isAsking(flow) == false {
            Button(action: { store.askStop(state.id, count: stop.count) }) {
                Text("Stop \(stop.count)")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(Color.white)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 2)
                    .background(Capsule().fill(Color.white.opacity(0.22)))
            }
            .buttonStyle(.plain)
            .padding(.trailing, 6)
            .accessibilityLabel(stop.spoken)
        }
    }

    private func isAsking(_ flow: StopFlow) -> Bool {
        if case .asking = flow { true } else { false }
    }
}

struct ProjectDotView: View {
    let dot: ProjectDot

    var body: some View {
        switch dot {
        case .filled:
            Circle()
                .fill(statusDotColor(.green))
                .frame(width: 7, height: 7)
        case .ring:
            Circle()
                .strokeBorder(statusDotColor(.orange), lineWidth: 1.5)
                .frame(width: 7, height: 7)
        case .none:
            Color.clear
        }
    }
}
