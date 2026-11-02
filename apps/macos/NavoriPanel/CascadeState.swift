import CoreGraphics

public struct CascadeNode: Equatable, Sendable {
    public let id: String
    public let isActionable: Bool
    public let children: [CascadeNode]

    public init(id: String, isActionable: Bool = false, children: [CascadeNode] = []) {
        self.id = id
        self.isActionable = isActionable
        self.children = children
    }
}

public enum CascadeKey: Sendable {
    case up, down, left, right, `return`, escape
}

public enum CascadeEvent: Sendable {
    case pointerOn(row: String, at: CGPoint, submenu: CGRect?)
    case pointerLeft(row: String)
    case click(row: String)
    case hoverTimerFired
    case key(CascadeKey)
    case replaceTree([CascadeNode])
}

public enum CascadeEffect: Equatable, Sendable {
    case open(row: String)
    case close(depth: Int)
    case armHoverTimer
    case cancelHoverTimer
    case activate(row: String)
    case closeAll
}

public struct CascadeState: Sendable {
    public private(set) var openPath: [String] = []
    public private(set) var selection: String?

    private var root: [CascadeNode]
    private let maxDepth: Int
    private var pendingRow: String?
    private var lastPoint: CGPoint?
    private var anchor: CGPoint?

    public init(root: [CascadeNode], maxDepth: Int = 3) {
        self.root = root
        self.maxDepth = maxDepth
    }

    public mutating func handle(_ event: CascadeEvent) -> [CascadeEffect] {
        switch event {
        case let .pointerOn(row, at, submenu):
            return handlePointerOn(row: row, at: at, submenu: submenu)
        case let .pointerLeft(row):
            return handlePointerLeft(row)
        case let .click(row):
            return handleClick(row)
        case .hoverTimerFired:
            return handleHoverTimerFired()
        case let .key(key):
            return handleKey(key)
        case let .replaceTree(tree):
            return handleReplaceTree(tree)
        }
    }

    private mutating func handlePointerOn(
        row: String,
        at point: CGPoint,
        submenu: CGRect?
    ) -> [CascadeEffect] {
        if lastPoint == point {
            return []
        }
        lastPoint = point
        pendingRow = row
        var effects: [CascadeEffect] = []
        if isInsideDiagonal(point, submenu: submenu) == false {
            anchor = point
            effects.append(contentsOf: apply(row, shouldOpen: false))
        }
        effects.append(.armHoverTimer)
        return effects
    }

    private mutating func handlePointerLeft(_ row: String) -> [CascadeEffect] {
        if selection == row, openPath.contains(row) == false {
            selection = nil
        }
        guard pendingRow == row else { return [] }
        pendingRow = nil
        return [.cancelHoverTimer]
    }

    private mutating func handleClick(_ row: String) -> [CascadeEffect] {
        var effects: [CascadeEffect] = []
        if pendingRow != nil {
            pendingRow = nil
            effects.append(.cancelHoverTimer)
        }
        guard let node = node(id: row) else { return effects }
        if node.children.isEmpty == false {
            effects.append(contentsOf: apply(row, shouldOpen: mayOpen(node)))
            return effects
        }
        guard isActionable(node) else { return effects }
        effects.append(contentsOf: apply(row, shouldOpen: false))
        effects.append(.activate(row: row))
        return effects
    }

    private mutating func handleHoverTimerFired() -> [CascadeEffect] {
        guard let pendingRow else { return [] }
        return apply(pendingRow, shouldOpen: true)
    }

    private mutating func handleKey(_ key: CascadeKey) -> [CascadeEffect] {
        var effects: [CascadeEffect] = []
        if pendingRow != nil {
            pendingRow = nil
            effects.append(.cancelHoverTimer)
        }
        switch key {
        case .down:
            effects.append(contentsOf: moveSelection(delta: 1))
        case .up:
            effects.append(contentsOf: moveSelection(delta: -1))
        case .right:
            effects.append(contentsOf: handleKeyRight())
        case .left:
            effects.append(contentsOf: handleKeyLeft())
        case .return:
            guard let selection else { return effects }
            effects.append(contentsOf: handleClick(selection))
        case .escape:
            if openPath.isEmpty {
                effects.append(.closeAll)
            } else {
                effects.append(contentsOf: handleKeyLeft())
            }
        }
        return effects
    }

    private mutating func moveSelection(delta: Int) -> [CascadeEffect] {
        let rows = actionableIds(in: deepestPanel())
        guard rows.isEmpty == false else { return [] }
        let current: Int
        if let selection, let index = rows.firstIndex(of: selection) {
            current = index
        } else {
            current = delta > 0 ? -1 : rows.count
        }
        let next = ((current + delta) % rows.count + rows.count) % rows.count
        selection = rows[next]
        return []
    }

    private mutating func handleKeyRight() -> [CascadeEffect] {
        guard let selection, let node = node(id: selection), mayOpen(node) else {
            return []
        }
        let effects = apply(selection, shouldOpen: true)
        if let first = actionableIds(in: node.children).first {
            self.selection = first
        }
        return effects
    }

    private mutating func handleKeyLeft() -> [CascadeEffect] {
        guard let owner = openPath.last else { return [] }
        openPath.removeLast()
        selection = owner
        return [.close(depth: openPath.count + 1)]
    }

    private mutating func handleReplaceTree(_ tree: [CascadeNode]) -> [CascadeEffect] {
        root = tree
        let keep = validOpenPrefix()
        var effects: [CascadeEffect] = []
        if keep.count < openPath.count {
            effects.append(.close(depth: keep.count + 1))
            openPath = keep
        }
        if let selection, node(id: selection) == nil {
            self.selection = nil
        }
        return effects
    }

    private mutating func apply(_ row: String, shouldOpen: Bool) -> [CascadeEffect] {
        guard let target = node(id: row) else { return [] }
        var effects: [CascadeEffect] = []
        if isActionable(target) {
            selection = row
        }
        let keep = prefixOwned(by: row)
        if keep.count < openPath.count {
            effects.append(.close(depth: keep.count + 1))
            openPath = keep
        }
        if shouldOpen, mayOpen(target), openPath.contains(row) == false,
           let ancestors = ancestors(of: row), openPath == ancestors
        {
            openPath.append(row)
            effects.append(.open(row: row))
        }
        return effects
    }

    private func isInsideDiagonal(_ point: CGPoint, submenu: CGRect?) -> Bool {
        guard let submenu, let anchor, openPath.isEmpty == false else { return false }
        return SafeDiagonal.contains(point, from: anchor, toward: submenu)
    }

    private func isActionable(_ node: CascadeNode) -> Bool {
        node.isActionable || node.children.isEmpty == false
    }

    private func actionableIds(in nodes: [CascadeNode]) -> [String] {
        nodes.filter { isActionable($0) }.map(\.id)
    }

    private func deepestPanel() -> [CascadeNode] {
        var rows = root
        for id in openPath {
            guard let node = rows.first(where: { $0.id == id }) else { return rows }
            rows = node.children
        }
        return rows
    }

    private func mayOpen(_ node: CascadeNode) -> Bool {
        guard node.children.isEmpty == false, let depth = depth(of: node.id) else {
            return false
        }
        return depth + 1 < maxDepth
    }

    private func prefixOwned(by row: String) -> [String] {
        guard let ancestors = ancestors(of: row) else { return openPath }
        var keep: [String] = []
        for (index, id) in openPath.enumerated() {
            if index < ancestors.count {
                if id == ancestors[index] {
                    keep.append(id)
                } else {
                    break
                }
            } else if index == ancestors.count, id == row {
                keep.append(id)
                break
            } else {
                break
            }
        }
        return keep
    }

    private func validOpenPrefix() -> [String] {
        var prefix: [String] = []
        var siblings = root
        for id in openPath {
            guard let node = siblings.first(where: { $0.id == id }),
                  node.children.isEmpty == false
            else { break }
            prefix.append(id)
            siblings = node.children
        }
        return prefix
    }

    private func node(id: String) -> CascadeNode? {
        search(root, id: id)
    }

    private func search(_ nodes: [CascadeNode], id: String) -> CascadeNode? {
        for node in nodes {
            if node.id == id { return node }
            if let found = search(node.children, id: id) { return found }
        }
        return nil
    }

    private func ancestors(of id: String) -> [String]? {
        path(to: id, in: root, parents: [])
    }

    private func path(
        to id: String,
        in nodes: [CascadeNode],
        parents: [String]
    ) -> [String]? {
        for node in nodes {
            if node.id == id { return parents }
            if let found = path(to: id, in: node.children, parents: parents + [node.id]) {
                return found
            }
        }
        return nil
    }

    private func depth(of id: String) -> Int? {
        ancestors(of: id)?.count
    }
}
