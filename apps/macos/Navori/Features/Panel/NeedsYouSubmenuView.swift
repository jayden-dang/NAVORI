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

    private var listed: (todos: [NeedsYouTodoRow], updates: [NeedsYouUpdateRow]) {
        let filtered = NeedsYouList.filter(tab, todos: openTodos, updates: updates)
        let todos: [NeedsYouTodoRow] = {
            guard tab != .updates else { return filtered.todos }
            let liveIds = Set(openTodos.map(\.id))
            let held = striking.values.filter { !liveIds.contains($0.id) }
            return filtered.todos + held.sorted { $0.id < $1.id }
        }()
        return (todos, filtered.updates)
    }

    private var listEmpty: Bool {
        listed.todos.isEmpty && listed.updates.isEmpty
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            filter
            AddTodoField(store: store, cascade: cascade, today: today)
            MenuDivider()
            list
            MenuDivider()
            if tab == .updates {
                clearUpdatesRow
            }
            if let error = store.needsYouActionError {
                actionErrorLine(error)
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

    private var filter: some View {
        NeedsYouFilterControl(
            tab: Binding(
                get: { store.needsYouTab },
                set: { store.needsYouTab = $0 }
            ),
            todoCount: openTodos.count,
            updateCount: updates.count
        )
        .padding(.horizontal, 4)
        .padding(.top, 2)
        .padding(.bottom, 4)
    }

    private var list: some View {
        CappedScroll {
            VStack(alignment: .leading, spacing: 0) {
                if listEmpty {
                    caughtUpLine
                } else {
                    ForEach(listed.todos) { row in
                        TodoRowView(
                            row: row,
                            today: today,
                            struck: striking[row.id] != nil,
                            cascade: cascade,
                            onTick: { tick(row) }
                        )
                    }
                    ForEach(listed.updates) { row in
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

    private var caughtUpLine: some View {
        HStack(spacing: 6) {
            Image(systemName: "checkmark.circle")
                .font(.system(size: 13))
                .frame(width: 20, height: 20)
            Text("You're all caught up")
                .font(.system(size: 14))
        }
        .foregroundStyle(Color(nsColor: .secondaryLabelColor))
        .padding(.horizontal, 8)
        .frame(
            maxWidth: .infinity,
            minHeight: MenuMetrics.rowHeight,
            maxHeight: MenuMetrics.rowHeight,
            alignment: .leading
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("You're all caught up")
    }

    private var clearUpdatesRow: some View {
        MenuRowView(
            row: MenuRow(
                id: "clear_updates",
                icon: "clear",
                title: "Clear updates",
                value: nil,
                dot: nil,
                opensSubmenu: false,
                shortcut: nil
            ),
            cascade: cascade
        )
    }

    private func actionErrorLine(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 14))
            .foregroundStyle(Color(nsColor: .secondaryLabelColor))
            .padding(.horizontal, 8)
            .frame(
                maxWidth: .infinity,
                minHeight: MenuMetrics.rowHeight,
                maxHeight: MenuMetrics.rowHeight,
                alignment: .leading
            )
            .accessibilityAddTraits(.isStaticText)
    }

    private var historyRow: some View {
        MenuRowView(
            row: MenuRow(
                id: "needs.history",
                icon: "clock.arrow.circlepath",
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
        let visibleUpdates = Set(listed.updates.map(\.id))
        let frames = updateFrames.filter { visibleUpdates.contains($0.key) }
        store.drawnUpdateIds = NeedsYouList.drawnIds(rowFrames: frames, viewport: listViewport)
    }
}

private struct NeedsYouFilterControl: View {
    @Binding var tab: NeedsYouTab
    var todoCount: Int
    var updateCount: Int

    var body: some View {
        let labels = NeedsYouText.filterLabels(todoCount: todoCount, updateCount: updateCount)
        HStack(spacing: 2) {
            FilterTab(title: labels[0], selected: tab == .all) { tab = .all }
            FilterTab(title: labels[1], selected: tab == .todos) { tab = .todos }
            FilterTab(title: labels[2], selected: tab == .updates) { tab = .updates }
            Spacer(minLength: 0)
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Filter")
        .accessibilityAddTraits(.isTabBar)
    }
}

private struct FilterTab: View {
    let title: String
    let selected: Bool
    let action: () -> Void

    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 12, weight: selected ? .semibold : .medium))
                .foregroundStyle(Color(nsColor: selected ? .labelColor : .secondaryLabelColor))
                .padding(.horizontal, 10)
                .frame(height: 22)
                .background(Capsule().fill(fill))
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .focusEffectDisabled()
        .onHover { hovered = $0 }
        .accessibilityAddTraits(selected ? [.isButton, .isSelected] : .isButton)
    }

    private var fill: Color {
        if selected { return Color.primary.opacity(0.12) }
        return hovered ? Color.primary.opacity(0.05) : .clear
    }
}
