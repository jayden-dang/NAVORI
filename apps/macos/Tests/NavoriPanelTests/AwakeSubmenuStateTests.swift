import NavoriPanel
import Testing

@Suite
struct AwakeSubmenuStateTests {
    @Test func the_menu_hold_is_one_hour() {
        #expect(AwakeSubmenuState.menuHoldSeconds == 3600)
    }

    @Test(arguments: Self.copies)
    func each_status_block_and_holding_row_uses_its_text(_ copy: Copy) {
        let state = AwakeSubmenuState(
            status: copy.status,
            leases: copy.leases,
            displayHeld: copy.displayHeld,
            displayWanted: copy.displayWanted
        )
        #expect(state.statusTitle == copy.title)
        #expect(state.statusDetail == copy.detail)
        #expect(state.displayLabel == copy.displayLabel)
        #expect(state.releaseAllEnabled == copy.releaseAllEnabled)
        #expect(state.holdingRows == copy.rows)
    }

    @Test func a_duration_without_a_label_does_not_claim_one_hour() {
        let state = AwakeSubmenuState(
            status: .on(since: "11:38"),
            leases: [
                AwakeLeaseInput(
                    kind: .duration,
                    harness: nil,
                    label: nil,
                    pid: nil,
                    stopped: false,
                    until: "11:38"
                )
            ],
            displayHeld: false,
            displayWanted: false
        )
        #expect(state.holdingRows == [
            HoldingRow(line1: "Manual", line2: "until 11:38", accessibilitySentence: "Manual, until 11:38")
        ])
    }

    @Test func release_all_is_disabled_with_no_lease() {
        let state = AwakeSubmenuState(
            status: .off,
            leases: [],
            displayHeld: false,
            displayWanted: false
        )
        #expect(state.releaseAllEnabled == false)
        #expect(state.displayLabel == "Display on")
        #expect(state.holdingRows.isEmpty)
    }

    @Test func no_text_says_the_mac_will_sleep_or_names_another_app() {
        let texts = Self.copies.flatMap { copy -> [String] in
            let state = AwakeSubmenuState(
                status: copy.status,
                leases: copy.leases,
                displayHeld: copy.displayHeld,
                displayWanted: copy.displayWanted
            )
            return [state.statusTitle, state.statusDetail, state.displayLabel]
                + state.holdingRows.flatMap { [$0.line1, $0.line2, $0.accessibilitySentence] }
        }
        for text in texts {
            #expect(text.contains("will sleep") == false)
            #expect(text.contains("sleep now") == false)
            #expect(text.contains("Pause") == false)
            #expect(text.localizedCaseInsensitiveContains("another app") == false)
            #expect(text.localizedCaseInsensitiveContains("other app") == false)
        }
    }

    struct Copy: Sendable {
        let status: AwakeBlockStatus
        let leases: [AwakeLeaseInput]
        let displayHeld: Bool
        let displayWanted: Bool
        let title: String
        let detail: String
        let displayLabel: String
        let releaseAllEnabled: Bool
        let rows: [HoldingRow]
    }

    private static let hour = AwakeLeaseInput(
        kind: .duration,
        harness: nil,
        label: "1 hour",
        pid: nil,
        stopped: false,
        until: "11:38"
    )
    private static let untilOff = AwakeLeaseInput(
        kind: .untilOff,
        harness: nil,
        label: "Until off",
        pid: nil,
        stopped: false,
        until: nil
    )
    private static let display = AwakeLeaseInput(
        kind: .display,
        harness: nil,
        label: "Display",
        pid: nil,
        stopped: false,
        until: nil
    )
    private static let command = AwakeLeaseInput(
        kind: .command,
        harness: nil,
        label: "cargo",
        pid: 23719,
        stopped: false,
        until: nil
    )
    private static let working = AwakeLeaseInput(
        kind: .session,
        harness: "Claude Code",
        label: "NAVORI/main",
        pid: 23719,
        stopped: false,
        until: nil
    )
    private static let stopped = AwakeLeaseInput(
        kind: .session,
        harness: "Claude Code",
        label: "NAVORI/main",
        pid: 23719,
        stopped: true,
        until: nil
    )

    private static let copies: [Copy] = [
        Copy(
            status: .off,
            leases: [],
            displayHeld: false,
            displayWanted: false,
            title: "Awake off",
            detail: "No Leases",
            displayLabel: "Display on",
            releaseAllEnabled: false,
            rows: []
        ),
        Copy(
            status: .on(since: "11:38"),
            leases: [hour, working],
            displayHeld: false,
            displayWanted: false,
            title: "Awake · since 11:38",
            detail: "2 Leases · idle sleep blocked",
            displayLabel: "Display on",
            releaseAllEnabled: true,
            rows: [hourRow, workingRow]
        ),
        Copy(
            status: .on(since: "11:38"),
            leases: [hour],
            displayHeld: false,
            displayWanted: false,
            title: "Awake · since 11:38",
            detail: "1 Lease · idle sleep blocked",
            displayLabel: "Display on",
            releaseAllEnabled: true,
            rows: [hourRow]
        ),
        Copy(
            status: .on(since: "11:38"),
            leases: [hour, display],
            displayHeld: true,
            displayWanted: true,
            title: "Awake · since 11:38",
            detail: "2 Leases · idle sleep blocked · display kept on",
            displayLabel: "Display off",
            releaseAllEnabled: true,
            rows: [hourRow, displayRow]
        ),
        Copy(
            status: .notHeld,
            leases: [hour],
            displayHeld: false,
            displayWanted: false,
            title: "Can't hold this Mac awake",
            detail: "macOS refused the request. Navori keeps trying.",
            displayLabel: "Display on",
            releaseAllEnabled: true,
            rows: [hourRow]
        ),
        Copy(
            status: .on(since: "11:38"),
            leases: [hour, display],
            displayHeld: false,
            displayWanted: true,
            title: "Display not held",
            detail: "2 Leases · idle sleep blocked",
            displayLabel: "Display off",
            releaseAllEnabled: true,
            rows: [hourRow, displayRow]
        ),
        Copy(
            status: .on(since: "11:38"),
            leases: [untilOff, command, stopped],
            displayHeld: false,
            displayWanted: false,
            title: "Awake · since 11:38",
            detail: "3 Leases · idle sleep blocked",
            displayLabel: "Display on",
            releaseAllEnabled: true,
            rows: [untilOffRow, commandRow, stoppedRow]
        ),
    ]

    private static let hourRow = HoldingRow(
        line1: "1 hour",
        line2: "until 11:38",
        accessibilitySentence: "1 hour, until 11:38"
    )
    private static let untilOffRow = HoldingRow(
        line1: "Until off",
        line2: "",
        accessibilitySentence: "Until off"
    )
    private static let displayRow = HoldingRow(
        line1: "Display",
        line2: "",
        accessibilitySentence: "Display"
    )
    private static let commandRow = HoldingRow(
        line1: "cargo",
        line2: "running · pid 23719",
        accessibilitySentence: "cargo, running · pid 23719"
    )
    private static let workingRow = HoldingRow(
        line1: "Claude Code · NAVORI/main",
        line2: "working · pid 23719",
        accessibilitySentence: "Claude Code · NAVORI/main, working · pid 23719"
    )
    private static let stoppedRow = HoldingRow(
        line1: "Claude Code · NAVORI/main",
        line2: "stopped · background work · pid 23719",
        accessibilitySentence: "Claude Code · NAVORI/main, stopped · background work · pid 23719"
    )
}
