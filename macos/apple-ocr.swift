import Foundation
import Vision
import AppKit

struct BoxItem: Codable {
    let text: String
    let x: Double
    let y: Double
    let w: Double
    let h: Double
    let confidence: Double
}

func loadImage(at path: String) -> CGImage? {
    let url = URL(fileURLWithPath: path)
    if let source = CGImageSourceCreateWithURL(url as CFURL, nil),
       let cgImage = CGImageSourceCreateImageAtIndex(source, 0, nil) {
        return cgImage
    }
    if let nsImage = NSImage(contentsOfFile: path),
       let tiffData = nsImage.tiffRepresentation,
       let rep = NSBitmapImageRep(data: tiffData) {
        return rep.cgImage
    }
    return nil
}

func mapLang(_ hint: String) -> String {
    let lower = hint.lowercased()
    if lower.contains("zh") || lower.contains("cn") || lower.contains("chinese") {
        return "zh-Hans"
    } else if lower.contains("en") {
        return "en-US"
    } else if lower.contains("ja") || lower.contains("jp") {
        return "ja-JP"
    } else if lower.contains("ko") || lower.contains("kr") {
        return "ko-KR"
    } else if lower.contains("fr") {
        return "fr-FR"
    } else if lower.contains("de") {
        return "de-DE"
    } else if lower.contains("es") {
        return "es-ES"
    } else if lower.contains("ru") {
        return "ru-RU"
    }
    return "en-US"
}

func tryRecognize(
    cgImage: CGImage,
    level: VNRequestTextRecognitionLevel,
    langHint: String?,
    cpuOnly: Bool,
    fallbackLang: Bool
) -> [BoxItem]? {
    var items: [BoxItem] = []
    var requestError: Error? = nil

    let request = VNRecognizeTextRequest { req, err in
        if let err = err {
            requestError = err
            return
        }
        guard let observations = req.results as? [VNRecognizedTextObservation] else { return }
        for obs in observations {
            if let top = obs.topCandidates(1).first {
                let b = obs.boundingBox
                let item = BoxItem(
                    text: top.string,
                    x: Double(b.origin.x),
                    y: Double(b.origin.y),
                    w: Double(b.size.width),
                    h: Double(b.size.height),
                    confidence: Double(top.confidence)
                )
                items.append(item)
            }
        }
    }

    request.recognitionLevel = level
    if cpuOnly {
        request.usesCPUOnly = true
    }

    var supported: [String] = []
    if #available(macOS 12.0, *) {
        supported = (try? request.supportedRecognitionLanguages()) ?? []
    }

    if fallbackLang {
        // 最精简安全语言集
        let candidates = ["zh-Hans", "en-US"]
        let filtered = supported.isEmpty ? candidates : candidates.filter { supported.contains($0) }
        request.recognitionLanguages = filtered.isEmpty ? ["en-US"] : filtered
    } else if let hint = langHint, !hint.isEmpty {
        let mapped = mapLang(hint)
        let candidates = [mapped, "en-US", "zh-Hans"]
        let filtered = supported.isEmpty ? candidates : candidates.filter { supported.contains($0) }
        request.recognitionLanguages = filtered.isEmpty ? ["en-US"] : filtered
    } else {
        // 默认核心中英组合，响应极快且避免多语种模型导致 ANE 崩溃 / CRImageReaderError 1
        let candidates = ["zh-Hans", "en-US"]
        let filtered = supported.isEmpty ? candidates : candidates.filter { supported.contains($0) }
        request.recognitionLanguages = filtered.isEmpty ? ["en-US"] : filtered
    }
    request.usesLanguageCorrection = true

    let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])
    do {
        try handler.perform([request])
        if requestError != nil {
            return nil
        }
        return items
    } catch {
        return nil
    }
}

func executeTextRecognition(cgImage: CGImage, langHint: String?) -> [BoxItem]? {
    // 策略 1: 准确模式 (利用 ANE/GPU 硬件加速，中英预编译快速通道)
    if let items = tryRecognize(cgImage: cgImage, level: .accurate, langHint: langHint, cpuOnly: false, fallbackLang: false) {
        return items
    }

    // 策略 2: 降级到 CPU 执行 (规避 ANE/E5RT 硬件调度错误: CRImageReaderError 1)
    if let items = tryRecognize(cgImage: cgImage, level: .accurate, langHint: langHint, cpuOnly: true, fallbackLang: false) {
        return items
    }

    // 策略 3: 精简中英语言 + CPU 执行
    if let items = tryRecognize(cgImage: cgImage, level: .accurate, langHint: langHint, cpuOnly: true, fallbackLang: true) {
        return items
    }

    // 策略 4: 快速模式 + CPU 执行 (轻量特征识别兜底)
    if let items = tryRecognize(cgImage: cgImage, level: .fast, langHint: langHint, cpuOnly: true, fallbackLang: true) {
        return items
    }

    return nil
}

func main() {
    let args = CommandLine.arguments
    guard args.count > 1 else {
        fputs("Usage: apple-ocr <image-path> [--boxes] [--lang <hint>]\n", stderr)
        exit(1)
    }

    let imagePath = args[1]
    let isBoxes = args.contains("--boxes")
    let isQr = args.contains("--qr")
    var langHint: String? = nil
    if let langIdx = args.firstIndex(of: "--lang"), langIdx + 1 < args.count {
        langHint = args[langIdx + 1]
    }

    guard let cgImage = loadImage(at: imagePath) else {
        fputs("Failed to load image: \(imagePath)\n", stderr)
        exit(2)
    }

    // 若图片尺寸极小（如误点框选截图导致 1x1 或 2x2 像素），直接返回空结果，避免底层 Vision 断言崩溃
    if cgImage.width <= 2 || cgImage.height <= 2 {
        if isBoxes {
            print("[]")
        } else {
            print("")
        }
        exit(0)
    }

    if isQr {
        let request = VNDetectBarcodesRequest()
        let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])
        do {
            try handler.perform([request])
        } catch {
            fputs("Perform error: \(error.localizedDescription)\n", stderr)
            exit(3)
        }
        guard let results = request.results else { exit(0) }
        var barcodes: [String] = []
        for r in results {
            if let p = r.payloadStringValue, !p.isEmpty {
                barcodes.append(p)
            }
        }
        print(barcodes.joined(separator: "\n"))
        return
    }

    guard var items = executeTextRecognition(cgImage: cgImage, langHint: langHint) else {
        fputs("Perform error: Vision text recognition failed across all strategies\n", stderr)
        exit(3)
    }

    // Sort lines from top to bottom (y descending in Vision coordinate system),
    // then left to right (x ascending).
    items.sort { a, b in
        let midYA = a.y + a.h / 2.0
        let midYB = b.y + b.h / 2.0
        let lineTolerance = min(a.h, b.h) * 0.5
        if abs(midYA - midYB) > lineTolerance {
            return midYA > midYB
        }
        return a.x < b.x
    }

    if isBoxes {
        let encoder = JSONEncoder()
        if let data = try? encoder.encode(items), let jsonStr = String(data: data, encoding: .utf8) {
            print(jsonStr)
        }
    } else {
        let textLines = items.map { $0.text }
        print(textLines.joined(separator: "\n"))
    }
}

main()
