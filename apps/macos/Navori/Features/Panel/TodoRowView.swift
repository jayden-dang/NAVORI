import AppKit
import NavoriPanel
import SwiftUI

struct TodoRowView: View {
    let row: NeedsYouTodoRow
    let today: String
    let struck: Bool
    let cascade: CascadeController
    let onTick: () -> Void

    private var rowId: String { "todo.\(row.id)" }

    private var selected: Bool {
        cascade.selection == rowId
    }

    var body: some View {
        HStack(spacing: 6) {
            TodoCheckbox(struck: struck, action: onTick)
                .fixedSize()
                .frame(width: 20, height: 20)
                .accessibilityLabel(NeedsYouText.checkboxSpoken)
            Text(row.title)
                .font(.system(size: 14))
                .foregroundStyle(selected ? Color.white : Color(nsColor: .labelColor))
                .strikethrough(struck)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 8)
            DuePill(dueOn: row.dueOn, status: row.status, today: today, selected: selected)
        }
        .padding(.horizontal, 8)
        .frame(
            maxWidth: .infinity,
            minHeight: MenuMetrics.rowHeight,
            maxHeight: MenuMetrics.rowHeight,
            alignment: .leading
        )
        .background {
            if selected {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(Color(nsColor: .selectedContentBackgroundColor))
            }
        }
        .contentShape(Rectangle())
        .onTapGesture { cascade.click(rowId) }
        .onContinuousHover { phase in
            switch phase {
            case .active:
                cascade.pointer(on: rowId, at: NSEvent.mouseLocation)
            case .ended:
                cascade.pointerLeft(rowId)
            }
        }
        .onGeometryChange(for: CGRect.self) { proxy in
            proxy.frame(in: .global)
        } action: { frame in
            cascade.setRowFrame(rowId, frame)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(row.title)
        .accessibilityValue(NeedsYouText.pillSpoken(status: row.status, dueOn: row.dueOn))
    }
}

struct DuePill: View {
    let dueOn: String?
    let status: DueStatus
    let today: String
    var selected = false

    private var text: String {
        NeedsYouText.pill(dueOn: dueOn, today: today)
    }

    var body: some View {
        Text(text)
            .font(.system(size: 11.5, weight: .semibold))
            .foregroundStyle(ink)
            .padding(.horizontal, 7)
            .padding(.vertical, 2)
            .background(Capsule().fill(fill))
            .accessibilityHidden(true)
    }

    private var ink: Color {
        if selected { return .white }
        switch status {
        case .overdue: return Color(nsColor: .systemRed)
        case .today: return Color(nsColor: .systemOrange)
        case .soon: return soonInk
        case .later: return Color(nsColor: .systemBlue)
        case .none: return Color(nsColor: .secondaryLabelColor)
        }
    }

    private var fill: Color {
        if selected { return Color.white.opacity(0.22) }
        switch status {
        case .overdue: return Color(nsColor: .systemRed).opacity(0.18)
        case .today: return Color(nsColor: .systemOrange).opacity(0.18)
        case .soon: return Color(nsColor: .systemYellow).opacity(0.18)
        case .later: return Color(nsColor: .systemBlue).opacity(0.18)
        case .none: return Color.primary.opacity(0.08)
        }
    }

    private var soonInk: Color {
        let dark = NSApp.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        if dark { return Color(nsColor: .systemYellow) }
        return Color(nsColor: NSColor.systemYellow.shadow(withLevel: 0.35) ?? .systemYellow)
    }
}

private struct TodoCheckbox: NSViewRepresentable {
    var struck: Bool
    var action: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(action: action)
    }

    func makeNSView(context: Context) -> NSButton {
        let button = NSButton(checkboxWithTitle: "", target: context.coordinator, action: #selector(Coordinator.tapped(_:)))
        button.setButtonType(.switch)
        button.state = struck ? .on : .off
        button.imagePosition = .imageOnly
        button.isBordered = false
        button.focusRingType = .default
        return button
    }

    func updateNSView(_ button: NSButton, context: Context) {
        context.coordinator.action = action
        let next: NSControl.StateValue = struck ? .on : .off
        if button.state != next {
            button.state = next
        }
    }

    @MainActor
    final class Coordinator: NSObject {
        var action: () -> Void
        init(action: @escaping () -> Void) { self.action = action }
        @objc func tapped(_ sender: NSButton) {
            sender.state = .on
            action()
        }
    }
}
