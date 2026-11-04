import Foundation
import NavoriPanel
import Testing

@Suite
struct StopFlowTests {
    private func result(_ outcome: StopOutcome, reason: String? = nil) -> StopResult {
        StopResult(target: .container(id: "4f"), result: outcome, reason: reason)
    }

    @Test func pressing_asks_and_sends_nothing() {
        var flow = StopFlow.idle
        flow.press(count: 3)
        #expect(flow == .asking(count: 3))
    }

    @Test func confirming_starts_the_send_once() {
        var flow = StopFlow.idle
        flow.press(count: 2)
        let sent = flow.confirm()
        #expect(sent)
        #expect(flow == .running)
    }

    @Test func confirming_without_asking_sends_nothing() {
        var flow = StopFlow.idle
        let sent = flow.confirm()
        #expect(sent == false)
        #expect(flow == .idle)
    }

    @Test func dismissing_returns_to_idle_and_sends_nothing() {
        var flow = StopFlow.idle
        flow.press(count: 2)
        flow.dismiss()
        #expect(flow == .idle)
        let sent = flow.confirm()
        #expect(sent == false)
    }

    @Test func a_second_confirm_while_running_is_ignored() {
        var flow = StopFlow.idle
        flow.press(count: 2)
        let first = flow.confirm()
        let second = flow.confirm()
        #expect(first)
        #expect(second == false)
        #expect(flow == .running)
    }

    @Test func a_second_press_or_dismissal_while_running_is_ignored() {
        var flow = StopFlow.idle
        flow.press(count: 2)
        _ = flow.confirm()
        flow.press(count: 5)
        flow.dismiss()
        #expect(flow == .running)
    }

    @Test func a_stale_result_keeps_the_row_and_carries_the_reason() {
        var flow = StopFlow.idle
        flow.press(count: 1)
        _ = flow.confirm()
        flow.finish([result(.stale, reason: "The port closed.")])
        #expect(flow == .failed(["The port closed."]))
    }

    @Test func a_result_with_no_reason_still_says_why() {
        var flow = StopFlow.idle
        flow.press(count: 1)
        _ = flow.confirm()
        flow.finish([result(.stillStopping)])
        guard case .failed(let reasons) = flow else {
            Issue.record("expected failed, got \(flow)")
            return
        }
        #expect(reasons.count == 1)
        #expect(reasons[0].isEmpty == false)
    }

    @Test func stopped_stopped_alone_and_already_gone_clear_the_row() {
        for outcome in [StopOutcome.stopped, .stoppedAlone, .alreadyGone] {
            var flow = StopFlow.idle
            flow.press(count: 1)
            _ = flow.confirm()
            flow.finish([result(outcome)])
            #expect(flow == .idle)
        }
    }

    @Test func fewer_stops_than_asked_lists_each_reason_for_the_rest() {
        var flow = StopFlow.idle
        flow.press(count: 3)
        _ = flow.confirm()
        flow.finish([
            result(.stopped),
            result(.protected, reason: "Navori needs this process."),
            result(.failed, reason: "Docker is not reachable."),
        ])
        #expect(flow == .failed(["Navori needs this process.", "Docker is not reachable."]))
    }

    @Test func a_failed_flow_can_be_asked_again_or_dismissed() {
        var flow = StopFlow.failed(["x"])
        flow.press(count: 1)
        #expect(flow == .asking(count: 1))
        flow = .failed(["x"])
        flow.dismiss()
        #expect(flow == .idle)
    }

    @Test func a_reply_with_no_results_ends_in_a_visible_failure() {
        var flow = StopFlow.idle
        flow.press(count: 2)
        _ = flow.confirm()
        flow.finish([])
        #expect(flow == .failed(["Navori got no answer to the stop."]))
    }

    @Test func finishing_when_not_running_changes_nothing() {
        var flow = StopFlow.idle
        flow.finish([result(.stale)])
        #expect(flow == .idle)
    }

    @Test func a_kill_reply_decodes_each_target_and_outcome() throws {
        let good = """
        {"results":[
          {"target":{"process":{"pid":412,"start_us":1730000000000000}},"result":"stopped_alone","reason":null},
          {"target":{"container":{"id":"4f"}},"result":"already_gone"},
          {"target":{"process":{"pid":7,"start_us":1}},"result":"outside_selector","reason":"Elsewhere."},
          {"target":{"process":{"pid":8,"start_us":2}},"result":"still_stopping","reason":"Slow."}
        ]}
        """
        let reply = try JSONDecoder().decode(KillReply.self, from: Data(good.utf8))
        #expect(reply.results.map(\.result) == [.stoppedAlone, .alreadyGone, .outsideSelector, .stillStopping])
        #expect(reply.results[0].target == .process(pid: 412, startUs: 1_730_000_000_000_000))
        #expect(reply.results[1].target == .container(id: "4f"))
        #expect(reply.results[2].reason == "Elsewhere.")
        #expect(reply.results[1].reason == nil)
    }

    private func idle(_ name: String, ports: Int, request: String) -> RunningProjectInput {
        RunningProjectInput(
            name: name,
            isOther: false,
            state: .idle,
            sessions: [],
            ports: (0..<ports).map { RunningPortInput(port: 3000 + $0, name: "node", stopRequest: "p\($0)") },
            stoppablePorts: ports,
            rowStopRequest: request,
            childStopRequest: request
        )
    }

    @Test func stop_all_sends_one_request_per_idle_project_after_one_confirmation() throws {
        let input = RunningInput(
            unknownReason: nil,
            projects: [idle("api", ports: 2, request: "kill-api"), idle("web", ports: 1, request: "kill-web")],
            hiddenSystemPorts: 0,
            stopAll: ["kill-api", "kill-web"],
            stopAllPorts: 3,
            inUsePorts: 0,
            otherPortsLeft: 0
        )
        let offer = try #require(RunningSubmenuState(running: input).stopAll)
        var flow = StopFlow.idle
        flow.press(count: offer.ports)
        #expect(flow.confirm(sending: offer.requests, count: offer.ports) == ["kill-api", "kill-web"])
        #expect(flow.confirm(sending: offer.requests, count: offer.ports).isEmpty)
        #expect(flow == .running)
    }

    @Test func an_empty_request_or_a_changed_count_sends_nothing() {
        var flow = StopFlow.idle
        flow.press(count: 2)
        #expect(flow.confirm(sending: ["", "kill-api"], count: 2) == ["kill-api"])

        var empty = StopFlow.idle
        empty.press(count: 2)
        #expect(empty.confirm(sending: [""], count: 2).isEmpty)
        #expect(empty == .idle)

        var moved = StopFlow.idle
        moved.press(count: 2)
        #expect(moved.confirm(sending: ["kill-api"], count: 3).isEmpty)
        #expect(moved == .idle)

        var untouched = StopFlow.idle
        #expect(untouched.confirm(sending: ["kill-api"], count: 2).isEmpty)
    }

    @Test func a_failed_target_keeps_its_row_with_the_reason() {
        let stale = StopResult(target: .process(pid: 412, startUs: 1), result: .stale, reason: "The port closed.")
        let gone = StopResult(target: .process(pid: 413, startUs: 2), result: .stopped, reason: nil)
        var progress = StopProgress()
        progress.record([stale, gone])
        #expect(progress.entry(for: .process(pid: 412, startUs: 1)) == .failed("The port closed."))
        #expect(progress.entry(for: .process(pid: 413, startUs: 2)) == .stopping)
        #expect(progress.failures(among: [.process(pid: 412, startUs: 1), .process(pid: 413, startUs: 2)]) == ["The port closed."])
        #expect(progress.isStopping(among: [.process(pid: 413, startUs: 2)]))

        progress.prune(listed: [.process(pid: 412, startUs: 1)])
        #expect(progress.entry(for: .process(pid: 412, startUs: 1)) == .failed("The port closed."))
        #expect(progress.entry(for: .process(pid: 413, startUs: 2)) == nil)

        progress.prune(listed: [])
        #expect(progress.failures(among: [.process(pid: 412, startUs: 1)]).isEmpty)
    }

    @Test func fewer_stops_than_confirmed_read_as_results_not_as_an_error() {
        var flow = StopFlow.idle
        flow.press(count: 3)
        _ = flow.confirm(sending: ["kill-api"], count: 3)
        flow.finish([
            result(.stopped),
            result(.stopped),
            StopResult(target: .process(pid: 9, startUs: 3), result: .protected, reason: nil),
        ])
        #expect(flow == .failed(["This process is protected."]))
    }

    @Test func a_request_nobody_answered_reads_as_a_failed_result() {
        var flow = StopFlow.idle
        flow.press(count: 2)
        _ = flow.confirm(sending: ["a", "b"], count: 2)
        flow.finish([result(.stopped), .unanswered("Navori is not running.")])
        #expect(flow == .failed(["Navori is not running."]))
    }

    @Test func a_confirmation_text_names_ports_and_the_in_use_and_other_counts() {
        #expect(StopOffer(count: 3, request: "x").confirmation == "Stop 3 ports?")
        #expect(StopOffer(count: 1, request: "x").confirmation == "Stop 1 port?")

        let all = StopAllOffer(requests: ["a", "b"], ports: 5, projects: 2, inUse: 3, otherLeft: 1, systemHidden: 4)
        #expect(all.confirmation == "Stop 5 ports in 2 projects? 3 ports in use and 1 port in Other stay running.")
        #expect(all.systemNote == "4 system ports hidden")

        let plain = StopAllOffer(requests: ["a"], ports: 1, projects: 1, inUse: 0, otherLeft: 0, systemHidden: 0)
        #expect(plain.confirmation == "Stop 1 port in 1 project?")
        #expect(plain.systemNote == nil)

        for text in [all.confirmation, plain.confirmation, StopOffer(count: 2, request: "x").confirmation] {
            #expect(text.localizedCaseInsensitiveContains("process") == false)
        }
    }
}
