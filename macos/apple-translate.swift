import AppKit
import Foundation
import SwiftUI
import Translation

/// CLI：`apple-translate <src> <tgt> [infile] [outfile] [errfile]`
/// 无文件参数时从 stdin 读、写 stdout。
/// 语言包已安装时不弹窗、不抢焦点；仅下载语言包时才显示窗口。

enum IOFiles {
    static var outPath: String?
    static var errPath: String?
}

struct Payload {
    let text: String
    let src: Locale.Language
    let tgt: Locale.Language
    let srcId: String
    let tgtId: String

    static func read() -> Payload {
        let args = CommandLine.arguments
        guard args.count >= 3 else {
            fail("usage: apple-translate <src> <tgt> [infile] [outfile] [errfile]", code: 1)
        }
        if args.count >= 5 {
            IOFiles.outPath = args[4]
        }
        if args.count >= 6 {
            IOFiles.errPath = args[5]
        }
        let text: String
        if args.count >= 4 {
            text = (try? String(contentsOfFile: args[3], encoding: .utf8))?
                .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        } else {
            let data = FileHandle.standardInput.readDataToEndOfFile()
            text = String(data: data, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        }
        if text.isEmpty {
            fail("empty input", code: 1)
        }
        return Payload(
            text: text,
            src: Locale.Language(identifier: args[1]),
            tgt: Locale.Language(identifier: args[2]),
            srcId: args[1],
            tgtId: args[2]
        )
    }
}

struct TranslateHost: View {
    let payload: Payload
    @State private var configuration: TranslationSession.Configuration?

    init(payload: Payload) {
        self.payload = payload
        _configuration = State(
            initialValue: TranslationSession.Configuration(
                source: payload.src,
                target: payload.tgt
            )
        )
    }

    var body: some View {
        VStack(spacing: 8) {
            ProgressView()
            Text("正在下载 Apple 翻译语言包…")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
        }
        .padding(16)
        .frame(minWidth: 240, minHeight: 72)
        .translationTask(configuration) { session in
            do {
                let resp = try await session.translate(payload.text)
                writeOk(resp.targetText)
            } catch {
                let ns = error as NSError
                if ns.domain == NSCocoaErrorDomain && ns.code == NSUserCancelledError {
                    fail(
                        "已取消语言包下载。请在系统弹窗中允许下载，或打开「翻译」App 安装 \(payload.srcId) → \(payload.tgtId)",
                        code: 3
                    )
                }
                fail("\(error)", code: 2)
            }
        }
    }
}

func writeOk(_ text: String) -> Never {
    if let path = IOFiles.outPath {
        try? (text + "\n").write(toFile: path, atomically: true, encoding: .utf8)
    } else {
        FileHandle.standardOutput.write((text + "\n").data(using: .utf8) ?? Data())
        fflush(stdout)
    }
    exit(0)
}

func fail(_ message: String, code: Int32) -> Never {
    if let path = IOFiles.errPath {
        try? message.write(toFile: path, atomically: true, encoding: .utf8)
    }
    fputs(message + "\n", stderr)
    fflush(stderr)
    exit(code)
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    let payload: Payload
    var window: NSWindow?

    init(payload: Payload) {
        self.payload = payload
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        DispatchQueue.main.asyncAfter(deadline: .now() + 90) {
            fail("Apple 翻译超时（语言包下载未完成或系统未弹出下载提示）", code: 5)
        }
        Task { @MainActor in
            await self.start()
        }
    }

    @MainActor
    private func start() async {
        let availability = LanguageAvailability()
        let status = await availability.status(from: payload.src, to: payload.tgt)
        fputs("apple-translate status=\(status) \(payload.srcId)->\(payload.tgtId)\n", stderr)
        fflush(stderr)
        switch status {
        case .installed, .supported:
            showTaskWindow(shouldDisplay: status == .supported)
        default:
            fail("Apple 翻译不支持 \(payload.srcId) → \(payload.tgtId)", code: 4)
        }
    }

    @MainActor
    private func showTaskWindow(shouldDisplay: Bool) {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 260, height: 88),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        window.title = "Apple 翻译"
        window.isReleasedWhenClosed = false
        window.level = .floating
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        window.contentView = NSHostingView(rootView: TranslateHost(payload: payload))
        window.center()
        if shouldDisplay {
            // 不调用 activate / makeKey，避免抢焦点把 Transel 浮窗关掉
            window.orderFrontRegardless()
        }
        self.window = window
    }
}

let payload = Payload.read()
fputs("apple-translate start\n", stderr)
fflush(stderr)

let app = NSApplication.shared
let delegate = AppDelegate(payload: payload)
app.setActivationPolicy(.accessory)
app.delegate = delegate
app.run()
