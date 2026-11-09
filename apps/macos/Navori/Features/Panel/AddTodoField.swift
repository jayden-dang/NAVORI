import AppKit
import NavoriPanel
import SwiftUI

struct AddTodoField: View {
    let store: CoreStore
    let cascade: CascadeController
    let today: String

    @State private var focused = false

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "plus")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                .frame(width: 20, height: 20)
                .accessibilityHidden(true)
            AddTodoTextField(
                text: Binding(
                    get: { store.todoDraftText },
                    set: { store.todoDraftText = $0 }
                ),
                focused: $focused,
                onSubmit: submit
            )
            .frame(minWidth: 140, maxWidth: .infinity, minHeight: 18, maxHeight: 18)
            dateChip
        }
        .padding(.leading, 4)
        .padding(.trailing, 3)
        .frame(height: MenuMetrics.rowHeight + 4)
        .background {
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(Color.primary.opacity(0.06))
        }
        .overlay {
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .strokeBorder(
                    focused ? Color(nsColor: .controlAccentColor) : Color.primary.opacity(0.08),
                    lineWidth: focused ? 1.5 : 0.5
                )
        }
        .padding(.horizontal, 4)
        .padding(.vertical, 4)
    }

    private var chipTitle: String {
        NeedsYouText.pill(dueOn: store.todoDraftDueOn, today: today)
    }

    private var dateChip: some View {
        CapsuleButton(
            title: chipTitle,
            prominent: store.todoDraftDueOn != nil,
            selected: cascade.selection == "needs.date",
            action: { cascade.click("needs.date") },
            fontSize: 11.5,
            minHeight: 22,
            systemImage: "calendar"
        )
        .onGeometryChange(for: CGRect.self) { proxy in
            proxy.frame(in: .global)
        } action: { frame in
            cascade.setRowFrame("needs.date", frame)
        }
        .onContinuousHover { phase in
            switch phase {
            case .active:
                cascade.pointer(on: "needs.date", at: NSEvent.mouseLocation)
            case .ended:
                cascade.pointerLeft("needs.date")
            }
        }
        .accessibilityLabel("Due date")
        .accessibilityValue(chipTitle)
    }

    private func submit() {
        let title = store.todoDraftText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return }
        let dueOn = store.todoDraftDueOn
        Task { await store.addTodo(title: title, dueOn: dueOn) }
    }
}

private struct AddTodoTextField: NSViewRepresentable {
    @Binding var text: String
    @Binding var focused: Bool
    var onSubmit: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text, focused: $focused, onSubmit: onSubmit)
    }

    func makeNSView(context: Context) -> KeyPanelTextField {
        let field = KeyPanelTextField(string: text)
        field.placeholderString = "Add a to-do…"
        field.isBordered = false
        field.isBezeled = false
        field.drawsBackground = false
        field.font = .systemFont(ofSize: 13)
        // The rounded container draws the focus state (accent border).
        field.focusRingType = .none
        field.delegate = context.coordinator
        field.target = context.coordinator
        field.action = #selector(Coordinator.submitted(_:))
        field.cell?.isScrollable = true
        field.cell?.wraps = false
        field.cell?.usesSingleLineMode = true
        field.setAccessibilityLabel("Add a to-do")
        let coordinator = context.coordinator
        field.onFocus = { coordinator.focused.wrappedValue = true }
        return field
    }

    func updateNSView(_ field: KeyPanelTextField, context: Context) {
        context.coordinator.text = $text
        context.coordinator.focused = $focused
        context.coordinator.onSubmit = onSubmit
        if field.stringValue != text {
            field.stringValue = text
        }
    }

    @MainActor
    final class Coordinator: NSObject, NSTextFieldDelegate {
        var text: Binding<String>
        var focused: Binding<Bool>
        var onSubmit: () -> Void

        init(text: Binding<String>, focused: Binding<Bool>, onSubmit: @escaping () -> Void) {
            self.text = text
            self.focused = focused
            self.onSubmit = onSubmit
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            focused.wrappedValue = false
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSTextField else { return }
            text.wrappedValue = field.stringValue
        }

        @objc func submitted(_ sender: NSTextField) {
            text.wrappedValue = sender.stringValue
            onSubmit()
        }

        func control(
            _ control: NSControl,
            textView: NSTextView,
            doCommandBy commandSelector: Selector
        ) -> Bool {
            if commandSelector == #selector(NSResponder.insertNewline(_:))
                || commandSelector == #selector(NSResponder.insertNewlineIgnoringFieldEditor(_:))
            {
                text.wrappedValue = textView.string
                onSubmit()
                return true
            }
            return false
        }
    }
}

private final class KeyPanelTextField: NSTextField {
    var onFocus: () -> Void = {}

    override var needsPanelToBecomeKey: Bool { true }

    // Editing starts on focus, before the first keystroke; end is reported by controlTextDidEndEditing.
    override func becomeFirstResponder() -> Bool {
        let became = super.becomeFirstResponder()
        if became { onFocus() }
        return became
    }
}
