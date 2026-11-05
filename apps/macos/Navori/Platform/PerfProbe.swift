import AppKit
import Darwin
import os

@MainActor
enum PerfProbe {
    private static let log = Logger(subsystem: "dev.navori", category: "perf")
    private static var statusBar: StatusBarController?
    private static var sources: [DispatchSourceSignal] = []
    private static var didMarkStatusItem = false
    private static var processStartUS: UInt64?

    static func start(statusBar: StatusBarController, store _: CoreStore) {
        if AppEnvironment.perfFlagWithoutHome {
            log.fault("NAVORI_PERF=1 requires a non-empty NAVORI_PERF_HOME; set that directory or unset NAVORI_PERF")
            NSApp.terminate(nil)
            return
        }
        guard AppEnvironment.perfMode else { return }
        self.statusBar = statusBar
        installSignalHandlers()
    }

    static func markStatusItemDrawn() {
        guard AppEnvironment.perfMode, !didMarkStatusItem else { return }
        guard let start = processStartMicroseconds() else {
            log.fault("status item clock failed to read the process start")
            return
        }
        guard let now = nowMicroseconds() else {
            log.fault("status item clock read failed")
            return
        }
        guard let ms = milliseconds(from: start, to: now) else {
            log.fault("status item clock went backwards")
            return
        }
        didMarkStatusItem = true
        log.notice("statusItemDrawn ms=\(format(ms), privacy: .public)")
    }

    static func markMenuOpened(since: UInt64, first: Bool, modules: Int) {
        guard AppEnvironment.perfMode else { return }
        guard let now = nowMicroseconds() else {
            log.fault("menu open clock read failed")
            return
        }
        guard let ms = milliseconds(from: since, to: now) else {
            log.fault("menu open clock went backwards")
            return
        }
        let text = format(ms)
        log.notice("menuOpened ms=\(text, privacy: .public) first=\(first, privacy: .public) modules=\(modules, privacy: .public)")
    }

    static func markSubmenuOpened(since: UInt64) {
        guard AppEnvironment.perfMode else { return }
        guard let now = nowMicroseconds() else {
            log.fault("submenu open clock read failed")
            return
        }
        guard let ms = milliseconds(from: since, to: now) else {
            log.fault("submenu open clock went backwards")
            return
        }
        log.notice("submenuOpened ms=\(format(ms), privacy: .public)")
    }

    /// Clock sample taken after a panel close and before the panel is shown.
    static func captureTrigger() -> UInt64? {
        guard let now = nowMicroseconds() else {
            log.fault("menu open clock read failed")
            return nil
        }
        return now
    }

    static func fault(_ message: String) {
        log.fault("\(message, privacy: .public)")
    }

    // Ignore first: the default action would terminate the process before the source runs.
    private static func installSignalHandlers() {
        _ = signal(SIGUSR1, SIG_IGN)
        _ = signal(SIGUSR2, SIG_IGN)
        _ = signal(SIGTERM, SIG_IGN)

        let openMenu = DispatchSource.makeSignalSource(signal: SIGUSR1, queue: .main)
        openMenu.setEventHandler {
            MainActor.assumeIsolated {
                statusBar?.openMenuFresh()
            }
        }
        openMenu.resume()

        let openSubmenu = DispatchSource.makeSignalSource(signal: SIGUSR2, queue: .main)
        openSubmenu.setEventHandler {
            MainActor.assumeIsolated {
                statusBar?.openCoreSubmenuFresh()
            }
        }
        openSubmenu.resume()

        let terminate = DispatchSource.makeSignalSource(signal: SIGTERM, queue: .main)
        terminate.setEventHandler {
            MainActor.assumeIsolated {
                NSApp.terminate(nil)
            }
        }
        terminate.resume()
        sources = [openMenu, openSubmenu, terminate]
    }

    private static func nowMicroseconds() -> UInt64? {
        var tv = timeval()
        guard gettimeofday(&tv, nil) == 0 else { return nil }
        return microseconds(tv)
    }

    private static func processStartMicroseconds() -> UInt64? {
        if let processStartUS { return processStartUS }
        var info = kinfo_proc()
        var size = MemoryLayout<kinfo_proc>.stride
        var mib: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()]
        let rc = mib.withUnsafeMutableBufferPointer { buf in
            sysctl(buf.baseAddress, u_int(buf.count), &info, &size, nil, 0)
        }
        guard rc == 0, let us = microseconds(info.kp_proc.p_un.__p_starttime) else { return nil }
        processStartUS = us
        return us
    }

    private static func microseconds(_ tv: timeval) -> UInt64? {
        guard tv.tv_sec >= 0, tv.tv_usec >= 0 else { return nil }
        return UInt64(tv.tv_sec) * 1_000_000 + UInt64(tv.tv_usec)
    }

    private static func milliseconds(from start: UInt64, to end: UInt64) -> Double? {
        guard end >= start else { return nil }
        return Double(end - start) / 1000
    }

    private static func format(_ ms: Double) -> String {
        String(format: "%.1f", locale: Locale(identifier: "en_US_POSIX"), arguments: [ms])
    }
}
