import Darwin
import Foundation

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("perf-probe: \(message)\n".utf8))
    exit(1)
}

func cError(_ prefix: String) -> Never {
    let err = errno
    let detail = String(cString: strerror(err))
    fail("\(prefix): \(detail)")
}

guard CommandLine.arguments.count == 2 else {
    fail("usage: perf-probe <pid>")
}
guard let pidArg = Int64(CommandLine.arguments[1]), pidArg > 0, pidArg <= Int64(Int32.max) else {
    fail("pid is not a positive 32-bit integer")
}
let pid = Int32(pidArg)

var usage = rusage_info_v4()
let usageRc = withUnsafeMutablePointer(to: &usage) { pointer -> Int32 in
    pointer.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) { rebound in
        proc_pid_rusage(pid, RUSAGE_INFO_V4, rebound)
    }
}
if usageRc != 0 {
    cError("proc_pid_rusage")
}

var task = proc_taskinfo()
let taskSize = Int32(MemoryLayout<proc_taskinfo>.size)
let taskBytes = withUnsafeMutablePointer(to: &task) { pointer in
    proc_pidinfo(pid, PROC_PIDTASKINFO, 0, pointer, taskSize)
}
if taskBytes != taskSize {
    cError("proc_pidinfo")
}

var timebase = mach_timebase_info_data_t()
if mach_timebase_info(&timebase) != KERN_SUCCESS {
    fail("mach_timebase_info failed")
}
let numer = UInt64(timebase.numer)
let denom = UInt64(timebase.denom)
if numer == 0 || denom == 0 {
    fail("mach_timebase_info returned a zero scale")
}

func machTicksToNanos(_ ticks: UInt64) -> UInt64? {
    let (quot, rem) = ticks.quotientAndRemainder(dividingBy: denom)
    let (hi, hiOverflow) = quot.multipliedReportingOverflow(by: numer)
    let (remProduct, remOverflow) = rem.multipliedReportingOverflow(by: numer)
    if hiOverflow || remOverflow {
        return nil
    }
    let (sum, sumOverflow) = hi.addingReportingOverflow(remProduct / denom)
    return sumOverflow ? nil : sum
}

guard let userNs = machTicksToNanos(usage.ri_user_time),
      let systemNs = machTicksToNanos(usage.ri_system_time) else {
    fail("cpu time overflowed converting mach ticks")
}
let (cpuNs, cpuOverflow) = userNs.addingReportingOverflow(systemNs)
if cpuOverflow {
    fail("cpu time overflowed adding user and system")
}

guard task.pti_threadnum >= 0 else {
    fail("negative thread count")
}
let threads = UInt32(task.pti_threadnum)
let tNs = clock_gettime_nsec_np(CLOCK_UPTIME_RAW)
if tNs == 0 {
    fail("clock_gettime_nsec_np failed")
}

print(
    "{\"t_ns\":\(tNs),\"footprint\":\(usage.ri_phys_footprint),\"peak\":\(usage.ri_lifetime_max_phys_footprint),\"logical_writes\":\(usage.ri_logical_writes),\"cpu_ns\":\(cpuNs),\"idle_wakeups\":\(usage.ri_pkg_idle_wkups),\"threads\":\(threads)}"
)
