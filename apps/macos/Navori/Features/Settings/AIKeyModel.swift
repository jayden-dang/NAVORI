import Foundation
import NavoriBanner
import Observation

@MainActor
@Observable
final class AIKeyModel {
    private(set) var status: String?
    private(set) var isBusy = false
    private(set) var failed = false

    @ObservationIgnored private let saveKey: (String) async throws -> Void
    @ObservationIgnored private let removeKey: () async throws -> Bool

    init(
        save: @escaping (String) async throws -> Void,
        remove: @escaping () async throws -> Bool
    ) {
        saveKey = save
        removeKey = remove
    }

    /// Returns true when the key was stored, so the caller can clear the field.
    func save(_ key: String) async -> Bool {
        guard !isBusy else { return false }
        begin()
        defer { isBusy = false }
        do {
            try await saveKey(key)
            status = "Saved"
            return true
        } catch {
            fail(error)
            return false
        }
    }

    func remove() async {
        guard !isBusy else { return }
        begin()
        defer { isBusy = false }
        do {
            status = try await removeKey() ? "Removed" : "No key was stored"
        } catch {
            fail(error)
        }
    }

    private func begin() {
        isBusy = true
        failed = false
        status = "Waiting for the Keychain…"
    }

    private func fail(_ error: Error) {
        failed = true
        status = CoreStartupBanner.menuFailure(for: error)
    }
}
