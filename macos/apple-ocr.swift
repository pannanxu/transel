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

func main() {
    let args = CommandLine.arguments
    guard args.count > 1 else {
        fputs("Usage: apple-ocr <image-path> [--boxes] [--lang <hint>]\n", stderr)
        exit(1)
    }

    let imagePath = args[1]
    let isBoxes = args.contains("--boxes")
    var langHint: String? = nil
    if let langIdx = args.firstIndex(of: "--lang"), langIdx + 1 < args.count {
        langHint = args[langIdx + 1]
    }

    guard let image = NSImage(contentsOfFile: imagePath),
          let tiffData = image.tiffRepresentation,
          let bitmapImage = NSBitmapImageRep(data: tiffData),
          let cgImage = bitmapImage.cgImage else {
        fputs("Failed to load image: \(imagePath)\n", stderr)
        exit(2)
    }

    var items: [BoxItem] = []

    let request = VNRecognizeTextRequest { request, error in
        if let error = error {
            fputs("Vision error: \(error.localizedDescription)\n", stderr)
            return
        }
        guard let observations = request.results as? [VNRecognizedTextObservation] else { return }
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

    request.recognitionLevel = .accurate
    if let hint = langHint, !hint.isEmpty {
        let mapped = mapLang(hint)
        request.recognitionLanguages = [mapped, "zh-Hans", "en-US"]
    } else {
        request.recognitionLanguages = [
            "zh-Hans", "zh-Hant", "en-US", "ja-JP", "ko-KR",
            "fr-FR", "de-DE", "es-ES", "pt-BR", "it-IT", "ru-RU"
        ]
    }
    request.usesLanguageCorrection = true

    let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])
    do {
        try handler.perform([request])
    } catch {
        fputs("Perform error: \(error.localizedDescription)\n", stderr)
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

main()
