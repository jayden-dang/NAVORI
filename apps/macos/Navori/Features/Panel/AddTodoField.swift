import AppKit
import NavoriPanel
import SwiftUI

struct AddTodoField: View {
    let store: CoreStore
    let cascade: CascadeController
    let today: String

    var body: some View {
        HStack(spacing: 6) {
            AddTodoTextField(
                text: Binding(
                    get: { store.todoDraftText },
                    set: { store.todoDraftText = $0 }
                ),
                onSubmit: submit
            )
            .frame(minWidth: 160, maxWidth: .infinity, minHeight: 24, maxHeight: 24)
            dateChip
        }
        .padding(.horizontal, 8)
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
            minHeight: 20
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
    var onSubmit: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text, onSubmit: onSubmit)
    }

    func makeNSView(context: Context) -> KeyPanelTextField {
        let field = KeyPanelTextField(string: text)
        field.placeholderString = "Add a to-do…"
        field.isBordered = false
        field.isBezeled = false
        field.drawsBackground = true
        field.backgroundColor = NSColor.labelColor.withAlphaComponent(0.06)
        field.font = .systemFont(ofSize: 13)
        field.focusRingType = .default
        field.delegate = context.coordinator
        field.target = context.coordinator
        field.action = #selector(Coordinator.submitted(_:))
        field.cell?.isScrollable = true
        field.cell?.wraps = false
        field.cell?.usesSingleLineMode = true
        field.wantsLayer = true
        field.layer?.cornerRadius = 6
        return field
    }

    func updateNSView(_ field: KeyPanelTextField, context: Context) {
        context.coordinator.text = $text
        context.coordinator.onSubmit = onSubmit
        if field.stringValue != text {
            field.stringValue = text
        }
    }

    @MainActor
    final class Coordinator: NSObject, NSTextFieldDelegate {
        var text: Binding<String>
        var onSubmit: () -> Void

        init(text: Binding<String>, onSubmit: @escaping () -> Void) {
            self.text = text
            self.onSubmit = onSubmit
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
    override var needsPanelToBecomeKey: Bool { true }
}
