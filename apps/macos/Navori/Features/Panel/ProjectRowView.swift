import AppKit
import NavoriPanel
import SwiftUI

struct ProjectRowView: View {
    let state: ProjectRowState
    let cascade: CascadeController

    var body: some View {
        let selected = cascade.selection == state.id
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
        .onContinuousHover { phase in
            switch phase {
            case .active:
                cascade.pointer(on: state.id, at: NSEvent.mouseLocation)
            case .ended:
                cascade.pointerLeft(state.id)
            }
        }
        .onGeometryChange(for: CGRect.self) { proxy in
            proxy.frame(in: .global)
        } action: { frame in
            cascade.setRowFrame(state.id, frame)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(state.spoken)
        .accessibilityAddTraits(.isButton)
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
