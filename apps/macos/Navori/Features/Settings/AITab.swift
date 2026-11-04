import SwiftUI

struct AITab: View {
    @State private var model: AIKeyModel
    @State private var key = ""

    init(store: CoreStore) {
        _model = State(initialValue: AIKeyModel(
            save: { try await store.saveProviderKey($0) },
            remove: { try await store.removeProviderKey() }
        ))
    }

    var body: some View {
        Form {
            Section("Provider key") {
                SecureField("Provider API key", text: $key)
                HStack {
                    Button("Save") {
                        let pasted = key
                        Task { if await model.save(pasted) { key = "" } }
                    }
                    Button("Remove") { Task { await model.remove() } }
                    Spacer()
                }
                .disabled(model.isBusy)
                if let status = model.status {
                    if model.failed {
                        Text(status).foregroundStyle(.red).font(.caption)
                    } else {
                        Text(status).foregroundStyle(.secondary).font(.caption)
                    }
                }
            }
        }
        .formStyle(.grouped)
    }
}
