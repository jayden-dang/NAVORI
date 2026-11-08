import AppKit
import NavoriPanel
import SwiftUI

struct NeedsYouSubmenuView: View {
    let store: CoreStore
    @Environment(CascadeController.self) private var cascade

    @State private var updateFrames: [Int64: CGRect] = [:]
    @State private var listViewport: CGRect = .zero
    @State private var striking: [Int64: NeedsYouTodoRow] = [:]

    private var snapshot: NeedsYouSnapshot? { store.needsYou }

    private var tab: NeedsYouTab { store.needsYouTab }

    private var today: String { snapshot?.today ?? "" }

    private var openTodos: [NeedsYouTodoRow] {
        snapshot?.todos ?? []
    }

    private var updates: [NeedsYouUpdateRow] {
        snapshot?.updates ?? []
    }

    private var listedTodos: [NeedsYouTodoRow] {
        let filtered = NeedsYouList.filter(tab, todos: openTodos, updates: updates).todos
        guard tab != .updates else { return filtered }
        let liveIds = Set(openTodos.map(\.id))
        let held = striking.values.filter { !liveIds.contains($0.id) }
        return filtered + held.sorted { $0.id < $1.id }
    }

    private var listedUpdates: [NeedsYouUpdateRow] {
        NeedsYouList.filter(tab, todos: openTodos, updates: updates).updates
    }

    private var listEmpty: Bool {
        listedTodos.isEmpty && listedUpdates.isEmpty
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            filter
            AddTodoField(store: store, cascade: cascade, today: today)
            MenuDivider()
            list
            MenuDivider()
            if tab == .updates {
                clearUpdatesRow
            }
            historyRow
        }
        .padding(MenuMetrics.pad)
        .frame(width: 350, alignment: .topLeading)
        .fixedSize(horizontal: true, vertical: true)
        .onChange(of: updateFrames) { _, _ in publishDrawn() }
        .onChange(of: listViewport) { _, _ in publishDrawn() }
        .onChange(of: store.needsYouTab) { _, _ in
            updateFrames = [:]
            publishDrawn()
        }
    }

    private var header: some View {
        HStack(spacing: 6) {
            Text("Needs you")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Color(nsColor: .secondaryLabelColor))
            Spacer(minLength: 8)
            Text("\(snapshot?.needsCount ?? 0)")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Color(nsColor: .secondaryLabelColor))
        }
        .padding(.horizontal, 8)
        .frame(
            maxWidth: .infinity,
            minHeight: MenuMetrics.rowHeight,
            maxHeight: MenuMetrics.rowHeight,
            alignment: .leading
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Needs you")
        .accessibilityValue("\(snapshot?.needsCount ?? 0)")
        .accessibilityAddTraits(.isHeader)
    }

    private var filter: some View {
        NeedsYouFilterControl(
            tab: Binding(
                get: { store.needsYouTab },
                set: { store.needsYouTab = $0 }
            ),
            todoCount: openTodos.count,
            updateCount: updates.count
        )
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Filter")
        .accessibilityAddTraits(.isTabBar)
    }

    private var list: some View {
        CappedScroll {
            VStack(alignment: .leading, spacing: 0) {
                if listEmpty {
                    Text("✓ You're all caught up")
                        .font(.system(size: 14))
                        .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                        .padding(.horizontal, 8)
                        .frame(
                            maxWidth: .infinity,
                            minHeight: MenuMetrics.rowHeight,
                            maxHeight: MenuMetrics.rowHeight,
                            alignment: .leading
                        )
                        .accessibilityLabel("You're all caught up")
                } else {
                    ForEach(listedTodos) { row in
                        TodoRowView(
                            row: row,
                            today: today,
                            struck: striking[row.id] != nil,
                            cascade: cascade,
                            onTick: { tick(row) }
                        )
                    }
                    ForEach(listedUpdates) { row in
                        UpdateRowView(row: row, cascade: cascade) { id, frame in
                            updateFrames[id] = frame
                        }
                    }
                }
            }
        }
        .onGeometryChange(for: CGRect.self) { proxy in
            proxy.frame(in: .global)
        } action: { frame in
            listViewport = frame
        }
    }

    private var clearUpdatesRow: some View {
        let selected = cascade.selection == "clear_updates"
        return Text("Clear updates")
            .font(.system(size: 14))
            .foregroundStyle(selected ? Color.white : Color(nsColor: .labelColor))
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
            .onTapGesture { cascade.click("clear_updates") }
            .onContinuousHover { phase in
                switch phase {
                case .active:
                    cascade.pointer(on: "clear_updates", at: NSEvent.mouseLocation)
                case .ended:
                    cascade.pointerLeft("clear_updates")
                }
            }
            .onGeometryChange(for: CGRect.self) { proxy in
                proxy.frame(in: .global)
            } action: { frame in
                cascade.setRowFrame("clear_updates", frame)
            }
            .accessibilityLabel("Clear updates")
            .accessibilityAddTraits(.isButton)
    }

    private var historyRow: some View {
        MenuRowView(
            row: MenuRow(
                id: "needs.history",
                icon: nil,
                title: "History",
                value: nil,
                dot: nil,
                opensSubmenu: true,
                shortcut: nil
            ),
            cascade: cascade
        )
    }

    private func tick(_ row: NeedsYouTodoRow) {
        striking[row.id] = row
        Task { @MainActor in
            await store.tick(row.tickRequest)
            try? await Task.sleep(for: .milliseconds(450))
            striking.removeValue(forKey: row.id)
        }
    }

    private func publishDrawn() {
        let visibleUpdates = Set(listedUpdates.map(\.id))
        let frames = updateFrames.filter { visibleUpdates.contains($0.key) }
        store.drawnUpdateIds = NeedsYouList.drawnIds(rowFrames: frames, viewport: listViewport)
    }
}

private struct NeedsYouFilterControl: NSViewRepresentable {
    @Binding var tab: NeedsYouTab
    var todoCount: Int
    var updateCount: Int

    func makeCoordinator() -> Coordinator {
        Coordinator(tab: $tab)
    }

    func makeNSView(context: Context) -> NSSegmentedControl {
        let control = NSSegmentedControl()
        control.segmentCount = 3
        control.trackingMode = .selectOne
        control.segmentStyle = .rounded
        control.controlSize = .small
        control.font = .systemFont(ofSize: 12, weight: .medium)
        control.target = context.coordinator
        control.action = #selector(Coordinator.changed(_:))
        control.setAccessibilityLabel("Filter")
        control.setAccessibilityRole(.tabGroup)
        for (i, label) in labels.enumerated() {
            control.setLabel(label, forSegment: i)
        }
        control.selectedSegment = 0
        control.setContentHuggingPriority(.required, for: .vertical)
        control.setContentCompressionResistancePriority(.required, for: .vertical)
        control.setContentHuggingPriority(.defaultLow, for: .horizontal)
        return control
    }

    func updateNSView(_ control: NSSegmentedControl, context: Context) {
        context.coordinator.tab = $tab
        for (i, label) in labels.enumerated() {
            if control.label(forSegment: i) != label {
                control.setLabel(label, forSegment: i)
            }
        }
        let idx: Int
        switch tab {
        case .all: idx = 0
        case .todos: idx = 1
        case .updates: idx = 2
        }
        if control.selectedSegment != idx {
            control.selectedSegment = idx
        }
    }

    private var labels: [String] {
        ["All", "To-dos \(todoCount)", "Updates \(updateCount)"]
    }

    @MainActor
    final class Coordinator: NSObject {
        var tab: Binding<NeedsYouTab>
        init(tab: Binding<NeedsYouTab>) { self.tab = tab }

        @objc func changed(_ sender: NSSegmentedControl) {
            switch sender.selectedSegment {
            case 1: tab.wrappedValue = .todos
            case 2: tab.wrappedValue = .updates
            default: tab.wrappedValue = .all
            }
        }
    }
}
