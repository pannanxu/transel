import Foundation
import AppKit

class SnipWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}

class FastSnipView: NSView {
    var startPoint: NSPoint?
    var currentPoint: NSPoint?
    var isDragging = false
    var outPath: String = ""
    var primaryScreenHeight: CGFloat = 0

    var currentRect: NSRect {
        guard let s = startPoint, let c = currentPoint else { return .zero }
        let x = min(s.x, c.x)
        let y = min(s.y, c.y)
        let w = abs(s.x - c.x)
        let h = abs(s.y - c.y)
        return NSRect(x: x, y: y, width: w, height: h)
    }

    override func resetCursorRects() {
        super.resetCursorRects()
        addCursorRect(bounds, cursor: .crosshair)
    }

    override func draw(_ dirtyRect: NSRect) {
        let rect = currentRect
        let maskColor = NSColor(white: 0.0, alpha: 0.28)

        if rect.width > 2 && rect.height > 2 {
            // 绘制选区四周的暗色遮罩
            maskColor.setFill()
            // 顶部
            NSRect(x: 0, y: rect.maxY, width: bounds.width, height: bounds.height - rect.maxY).fill()
            // 底部
            NSRect(x: 0, y: 0, width: bounds.width, height: rect.minY).fill()
            // 左侧
            NSRect(x: 0, y: rect.minY, width: rect.minX, height: rect.height).fill()
            // 右侧
            NSRect(x: rect.maxX, y: rect.minY, width: bounds.width - rect.maxX, height: rect.height).fill()

            // 选框外层高亮活力蓝（Apple 风格 #007AFF）
            let blueColor = NSColor(red: 0.0, green: 0.48, blue: 1.0, alpha: 1.0)
            let path = NSBezierPath(rect: rect)
            path.lineWidth = 2.0
            blueColor.setStroke()
            path.stroke()

            // 内层 1pt 白色微光，确保在暗色和亮色背景下均清晰可见
            let innerRect = rect.insetBy(dx: 1.0, dy: 1.0)
            let innerPath = NSBezierPath(rect: innerRect)
            innerPath.lineWidth = 1.0
            NSColor(white: 1.0, alpha: 0.35).setStroke()
            innerPath.stroke()

            // 绘制四角加固角标
            drawCornerBrackets(in: rect)

            // 绘制尺寸指示胶囊
            drawDimensionBadge(for: rect)
        } else {
            // 未选区状态：整体铺设半透明遮罩与顶部操作提示
            maskColor.setFill()
            bounds.fill()
            drawInstructionPill()
        }
    }

    func drawCornerBrackets(in rect: NSRect) {
        let length: CGFloat = min(14.0, min(rect.width, rect.height) / 2.0)
        let whiteColor = NSColor.white
        let bracketPath = NSBezierPath()
        bracketPath.lineWidth = 3.0

        // 左下
        bracketPath.move(to: NSPoint(x: rect.minX, y: rect.minY + length))
        bracketPath.line(to: NSPoint(x: rect.minX, y: rect.minY))
        bracketPath.line(to: NSPoint(x: rect.minX + length, y: rect.minY))

        // 右下
        bracketPath.move(to: NSPoint(x: rect.maxX - length, y: rect.minY))
        bracketPath.line(to: NSPoint(x: rect.maxX, y: rect.minY))
        bracketPath.line(to: NSPoint(x: rect.maxX, y: rect.minY + length))

        // 左上
        bracketPath.move(to: NSPoint(x: rect.minX, y: rect.maxY - length))
        bracketPath.line(to: NSPoint(x: rect.minX, y: rect.maxY))
        bracketPath.line(to: NSPoint(x: rect.minX + length, y: rect.maxY))

        // 右上
        bracketPath.move(to: NSPoint(x: rect.maxX - length, y: rect.maxY))
        bracketPath.line(to: NSPoint(x: rect.maxX, y: rect.maxY))
        bracketPath.line(to: NSPoint(x: rect.maxX, y: rect.maxY - length))

        whiteColor.setStroke()
        bracketPath.stroke()

        // 四边中点高精定位点
        let dotSize: CGFloat = 5.0
        let midPoints = [
            NSPoint(x: rect.midX, y: rect.minY),
            NSPoint(x: rect.midX, y: rect.maxY),
            NSPoint(x: rect.minX, y: rect.midY),
            NSPoint(x: rect.maxX, y: rect.midY),
        ]
        for pt in midPoints {
            let dotRect = NSRect(x: pt.x - dotSize / 2, y: pt.y - dotSize / 2, width: dotSize, height: dotSize)
            let dotPath = NSBezierPath(ovalIn: dotRect)
            NSColor(red: 0.0, green: 0.48, blue: 1.0, alpha: 1.0).setFill()
            dotPath.fill()
            whiteColor.setStroke()
            dotPath.lineWidth = 1.0
            dotPath.stroke()
        }
    }

    func drawDimensionBadge(for rect: NSRect) {
        guard let screen = window?.screen else { return }
        let scale = screen.backingScaleFactor
        let pixelW = Int(round(rect.width * scale))
        let pixelH = Int(round(rect.height * scale))
        let text = "\(pixelW) × \(pixelH)"

        let font = NSFont.monospacedDigitSystemFont(ofSize: 11.5, weight: .semibold)
        let attrs: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: NSColor.white,
        ]
        let textSize = (text as NSString).size(withAttributes: attrs)

        let pillW = textSize.width + 16.0
        let pillH: CGFloat = 22.0
        var pillY = rect.maxY + 6.0
        if pillY + pillH > bounds.height - 10.0 {
            pillY = max(10.0, rect.minY - pillH - 6.0)
        }
        let pillX = max(10.0, min(rect.minX, bounds.width - pillW - 10.0))
        let pillRect = NSRect(x: pillX, y: pillY, width: pillW, height: pillH)

        let pillPath = NSBezierPath(roundedRect: pillRect, xRadius: 5.0, yRadius: 5.0)
        NSColor(white: 0.12, alpha: 0.88).setFill()
        pillPath.fill()
        NSColor(white: 1.0, alpha: 0.18).setStroke()
        pillPath.lineWidth = 0.8
        pillPath.stroke()

        let textPos = NSPoint(
            x: pillRect.midX - textSize.width / 2.0,
            y: pillRect.midY - textSize.height / 2.0 + 0.5
        )
        (text as NSString).draw(at: textPos, withAttributes: attrs)
    }

    func drawInstructionPill() {
        let text = "按住鼠标左键拖拽选区 · ESC 取消"
        let font = NSFont.systemFont(ofSize: 13.0, weight: .medium)
        let attrs: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: NSColor.white,
        ]
        let textSize = (text as NSString).size(withAttributes: attrs)
        let pillW = textSize.width + 28.0
        let pillH: CGFloat = 32.0
        let pillRect = NSRect(x: (bounds.width - pillW) / 2.0, y: bounds.height - 70.0, width: pillW, height: pillH)

        let pillPath = NSBezierPath(roundedRect: pillRect, xRadius: 16.0, yRadius: 16.0)
        NSColor(white: 0.15, alpha: 0.85).setFill()
        pillPath.fill()
        NSColor(white: 1.0, alpha: 0.2).setStroke()
        pillPath.lineWidth = 1.0
        pillPath.stroke()

        let textPos = NSPoint(
            x: pillRect.midX - textSize.width / 2.0,
            y: pillRect.midY - textSize.height / 2.0 + 0.5
        )
        (text as NSString).draw(at: textPos, withAttributes: attrs)
    }

    override func mouseDown(with event: NSEvent) {
        startPoint = event.locationInWindow
        currentPoint = event.locationInWindow
        isDragging = true
        needsDisplay = true
    }

    override func mouseDragged(with event: NSEvent) {
        guard isDragging else { return }
        currentPoint = event.locationInWindow
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        guard isDragging else { return }
        isDragging = false
        currentPoint = event.locationInWindow
        let rect = currentRect

        if rect.width >= 6 && rect.height >= 6 {
            captureRegion(rect: rect)
        } else {
            startPoint = nil
            currentPoint = nil
            needsDisplay = true
        }
    }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 { // ESC
            exit(1)
        }
    }

    func captureRegion(rect: NSRect) {
        guard let window = self.window else {
            exit(2)
        }
        // 瞬间隐藏选区窗口，确保截屏时不包含自身蒙版
        window.orderOut(nil)

        let winOrigin = window.frame.origin
        let globalX = Int(round(winOrigin.x + rect.minX))
        let globalY = Int(round(primaryScreenHeight - (winOrigin.y + rect.minY + rect.height)))
        let globalW = Int(round(rect.width))
        let globalH = Int(round(rect.height))
        let region = "\(globalX),\(globalY),\(globalW),\(globalH)"

        let proc = Process()
        proc.launchPath = "/usr/sbin/screencapture"
        proc.arguments = ["-R", region, "-x", "-t", "png", outPath]
        proc.launch()
        proc.waitUntilExit()

        exit(proc.terminationStatus == 0 ? 0 : 3)
    }
}

func main() {
    let args = CommandLine.arguments
    guard args.count > 1 else {
        fputs("Usage: apple-snip <output.png>\n", stderr)
        exit(1)
    }
    let outPath = args[1]

    let app = NSApplication.shared
    app.setActivationPolicy(.accessory)

    let mouseLoc = NSEvent.mouseLocation
    let targetScreen = NSScreen.screens.first(where: { NSMouseInRect(mouseLoc, $0.frame, false) })
        ?? NSScreen.main
        ?? NSScreen.screens[0]

    let primaryScreen = NSScreen.screens[0]
    let screenFrame = targetScreen.frame

    let window = SnipWindow(
        contentRect: screenFrame,
        styleMask: [.borderless],
        backing: .buffered,
        defer: false
    )
    window.level = .screenSaver
    window.isOpaque = false
    window.backgroundColor = .clear
    window.hasShadow = false
    window.ignoresMouseEvents = false
    window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]

    let snipView = FastSnipView(frame: NSRect(origin: .zero, size: screenFrame.size))
    snipView.outPath = outPath
    snipView.primaryScreenHeight = primaryScreen.frame.height
    window.contentView = snipView

    window.makeKeyAndOrderFront(nil)
    NSCursor.crosshair.set()

    app.run()
}

main()
