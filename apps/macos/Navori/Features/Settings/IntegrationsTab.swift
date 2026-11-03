import SwiftUI

struct IntegrationsTab: View {
    @State private var model: IntegrationsModel

    init(store: CoreStore) {
        _model = State(initialValue: IntegrationsModel(store: store))
    }

    var body: some View {
        Form {
            Section("Harness hooks") {
                ForEach(model.rows) { row in
                    IntegrationRowView(row: row, busy: model.isBusy) { action in
                        Task { await model.perform(action, harness: row.harness) }
                    }
                }
            }
            if let error = model.error {
                Text(error).foregroundStyle(.red).font(.caption)
            }
        }
        .formStyle(.grouped)
        .task { await model.refresh() }
    }
}

private struct IntegrationRowView: View {
    let row: IntegrationRow
    let busy: Bool
    let perform: (IntegrationsModel.Action) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(row.title)
                Label(row.stateLabel, systemImage: row.symbol)
                    .font(.caption).foregroundStyle(.secondary)
                Spacer()
                buttons
            }
            if let reason = row.reason {
                Text(reason).font(.caption).foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .contain)
    }

    @ViewBuilder
    private var buttons: some View {
        switch row.state {
        case "not_found", "not_installed":
            button("Install", .install)
        case "needs_repair":
            button("Repair", .repair)
            button("Remove", .remove)
        default:
            if row.offers(.remove) { button("Remove", .remove) }
        }
    }

    private func button(_ title: String, _ action: IntegrationsModel.Action) -> some View {
        Button(title) { perform(action) }
            .disabled(busy || !row.offers(action))
            .accessibilityLabel("\(title) hooks for \(row.title)")
    }
}
