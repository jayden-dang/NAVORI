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
}
