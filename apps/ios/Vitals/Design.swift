// Colours and small building blocks, matching Android's Palette and the
// desktop's metric colours. Text colours meet 4.5:1 in light and dark.

import SwiftUI
import VitalsCore

enum Palette {
    static let cpu = Color(hex: 0x0088F2)
    static let memory = Color(hex: 0x00B667)
    static let disk = Color(hex: 0xCB9400)
    static let network = Color(hex: 0xAC67EF)
    static let gpu = Color(hex: 0xF05653)
    static let thermal = Color(hex: 0xF6722B)
    static let ok = Color(hex: 0x1EAB53)
    static let warn = Color(hex: 0xE89D00)
    static let danger = Color(hex: 0xEE343B)
    static let accent = Color(light: 0x22864A, dark: 0x6CB882)

    static func of(_ level: Level) -> Color {
        switch level {
        case .ok: ok
        case .warn: warn
        case .danger: danger
        case .unknown: .secondary
        }
    }
}

extension Color {
    init(hex: UInt32) {
        self.init(
            red: Double((hex >> 16) & 0xFF) / 255, green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255)
    }

    init(light: UInt32, dark: UInt32) {
        self.init(UIColor { t in
            let h = t.userInterfaceStyle == .dark ? dark : light
            return UIColor(
                red: CGFloat((h >> 16) & 0xFF) / 255, green: CGFloat((h >> 8) & 0xFF) / 255,
                blue: CGFloat(h & 0xFF) / 255, alpha: 1)
        })
    }
}

/// A ring gauge. nil value = unmeasured: empty track and an em dash.
struct Ring: View {
    let label: String
    let value: Double?
    let text: String
    let colour: Color
    var scale: Double = 100

    var body: some View {
        VStack(spacing: 6) {
            ZStack {
                Circle().stroke(colour.opacity(0.15), lineWidth: 7)
                Circle()
                    .trim(from: 0, to: min(1, max(0, (value ?? 0) / scale)))
                    .stroke(colour, style: StrokeStyle(lineWidth: 7, lineCap: .round))
                    .rotationEffect(.degrees(-90))
                    .opacity(value == nil ? 0 : 1)
                Text(text).font(.callout.monospacedDigit().weight(.semibold))
                    .minimumScaleFactor(0.6).lineLimit(1).padding(6)
            }
            .frame(width: 66, height: 66)
            Text(label).font(.caption).foregroundStyle(.secondary).lineLimit(1)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label)
        .accessibilityValue(text)
    }
}

/// A tiny line chart of the last minute.
struct Sparkline: View {
    let values: [Double]
    let colour: Color
    var max: Double = 100

    var body: some View {
        GeometryReader { g in
            Path { p in
                guard values.count > 1 else { return }
                for (i, v) in values.enumerated() {
                    let x = g.size.width * Double(i) / Double(values.count - 1)
                    let y = g.size.height * (1 - Swift.min(1, Swift.max(0, v / max)))
                    i == 0 ? p.move(to: CGPoint(x: x, y: y)) : p.addLine(to: CGPoint(x: x, y: y))
                }
            }
            .stroke(colour, style: StrokeStyle(lineWidth: 1.5, lineJoin: .round))
        }
        .accessibilityHidden(true)
    }
}

struct InfoRow: View {
    let label: String
    let value: String

    init(_ label: String, _ value: String) {
        self.label = label
        self.value = value
    }

    var body: some View {
        LabeledContent(label) {
            Text(value).monospacedDigit().foregroundStyle(.primary)
        }
    }
}

/// A horizontal bar; nil = empty track.
struct Bar: View {
    let fraction: Double?
    let colour: Color

    var body: some View {
        GeometryReader { g in
            ZStack(alignment: .leading) {
                Capsule().fill(colour.opacity(0.15))
                Capsule().fill(colour)
                    .frame(width: g.size.width * min(1, max(0, fraction ?? 0)))
                    .opacity(fraction == nil ? 0 : 1)
            }
        }
        .frame(height: 6)
        .accessibilityHidden(true)
    }
}
