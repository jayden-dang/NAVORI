import Darwin
import Foundation

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("soak-probe: \(message)\n".utf8))
    exit(1)
}

func cError(_ prefix: String) -> Never {
    let err = errno
    let detail = String(cString: strerror(err))
    fail("\(prefix): \(detail)")
}

guard CommandLine.arguments.count == 2 else {
    fail("usage: soak-probe <pid>")
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
guard task.pti_threadnum >= 0 else {
    fail("negative thread count")
}
let threads = UInt32(task.pti_threadnum)

let fdStride = MemoryLayout<proc_fdinfo>.stride
let sizeNeeded = proc_pidinfo(pid, PROC_PIDLISTFDS, 0, nil, 0)
if sizeNeeded < 0 {
    cError("proc_pidinfo list fds size")
}
let capacity = Int(sizeNeeded) + 32 * fdStride
let buffer = UnsafeMutableRawPointer.allocate(byteCount: capacity, alignment: MemoryLayout<proc_fdinfo>.alignment)
defer { buffer.deallocate() }
let listed = proc_pidinfo(pid, PROC_PIDLISTFDS, 0, buffer, Int32(capacity))
if listed < 0 {
    cError("proc_pidinfo list fds")
}
let fds = Int(listed) / fdStride

print("{\"footprint\":\(usage.ri_phys_footprint),\"threads\":\(threads),\"fds\":\(fds)}")
