import Foundation
import NavoriPanel
import Testing

@Suite
struct LeaseLabelFixtureTests {
    private struct Case: Decodable {
        let project: String?
        let name: String?
    }

    private static func cases() throws -> [Case] {
        let root = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let url = root.appendingPathComponent("crates/ffi/tests/fixtures/lease_labels.json")
        return try JSONDecoder().decode([Case].self, from: Data(contentsOf: url))
    }

    @Test func the_menu_groups_each_label_under_the_project_the_core_reads() throws {
        let cases = try Self.cases()
        #expect(cases.isEmpty == false)
        for entry in cases {
            let lease = AwakeLeaseInput(
                kind: .session, harness: "Claude Code", label: entry.name, pid: 1, stopped: false, until: nil,
                project: entry.project
            )
            let state = AwakeSubmenuState(
                status: .on(sources: []), leases: [lease], displayHeld: true, tracking: true
            )
            #expect(
                state.holdingGroups.map(\.title) == [entry.project ?? "Other"],
                "project \(entry.project ?? "nil") name \(entry.name ?? "nil")"
            )
        }
    }
}
