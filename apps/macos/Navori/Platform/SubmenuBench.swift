import CoreGraphics
import Foundation
import os
import QuartzCore

/// Perf harness only, never reachable outside `NAVORI_PERF=1`. Injects a pointer entry into the Core row, lets
/// the real intent timer fire and records the open path that follows, with (`warm`) and without (`cold`) the
/// preparation done during the intent window. The two modes alternate to cancel drift.
@MainActor
enum SubmenuBench {
    private static let log = Logger(subsystem: "dev.navori", category: "perf")
    private static let row = "core"
    private static var running = false

    static func run(cascade: CascadeController, iterations: Int) {
        guard AppEnvironment.perfMode, !running else { return }
        running = true
        Task { @MainActor in
            defer { running = false }
            var lines = ["mode,prepareMs,setRootMs,frameMs,showMs,presentMs,enterToFrameMs,timerToFrameMs"]
            let trace = SubmenuPresentTrace()
            cascade.presentTrace = trace
            for index in 0..<(iterations * 2) {
                let warm = index % 2 == 1
                cascade.closeSubmenus()
                try? await Task.sleep(for: .milliseconds(30))
                trace.reset()
                cascade.prepareEnabled = warm
                let entered = CACurrentMediaTime()
                var frameAt = 0.0
                cascade.pointer(on: row, at: CGPoint(x: 100 + Double(index), y: 200)) {
                    frameAt = CACurrentMediaTime()
                }
                try? await Task.sleep(for: .milliseconds(150))
                guard frameAt > 0, trace.timerFiredAt > 0 else {
                    log.fault("submenuBench iteration \(index, privacy: .public) never reached a first frame")
                    continue
                }
                lines.append(String(
                    format: "%@,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f",
                    warm ? "warm" : "cold",
                    trace.prepare * 1000, trace.setRoot * 1000, trace.frame * 1000, trace.show * 1000,
                    trace.presentTotal * 1000, (frameAt - entered) * 1000, (frameAt - trace.timerFiredAt) * 1000
                ))
            }
            cascade.prepareEnabled = true
            cascade.presentTrace = nil
            cascade.closeSubmenus()
            let path = "/tmp/navori-submenu-bench.csv"
            try? lines.joined(separator: "\n").write(toFile: path, atomically: true, encoding: .utf8)
            log.notice("submenuBench done n=\(iterations, privacy: .public) path=\(path, privacy: .public)")
        }
    }
}
