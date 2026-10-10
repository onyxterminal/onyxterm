import AppKit

let canvasSize: CGFloat = 1024
let tileInset: CGFloat = 100
let tileCornerRadius: CGFloat = 185
let glyphScale: CGFloat = 0.56
let shadowBlur: CGFloat = 28
let shadowOffsetY: CGFloat = -10
let shadowOpacity: CGFloat = 0.30

func fail(_ message: String, code: Int32) -> Never {
    FileHandle.standardError.write(Data((message + "\n").utf8))
    exit(code)
}

let arguments = CommandLine.arguments
guard arguments.count == 3 else {
    fail("usage: compose-icon.swift <glyph.png> <output.png>", code: 64)
}

guard let glyph = NSImage(contentsOfFile: arguments[1]),
      let glyphImage = glyph.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
    fail("cannot read glyph \(arguments[1])", code: 66)
}

let srgb = CGColorSpace(name: CGColorSpace.sRGB)!
let pixels = Int(canvasSize)
guard let context = CGContext(
    data: nil,
    width: pixels,
    height: pixels,
    bitsPerComponent: 8,
    bytesPerRow: 0,
    space: srgb,
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
) else {
    fail("cannot create drawing context", code: 70)
}

let tileRect = CGRect(x: 0, y: 0, width: canvasSize, height: canvasSize).insetBy(dx: tileInset, dy: tileInset)
let tilePath = CGPath(
    roundedRect: tileRect,
    cornerWidth: tileCornerRadius,
    cornerHeight: tileCornerRadius,
    transform: nil
)

context.saveGState()
context.setShadow(
    offset: CGSize(width: 0, height: shadowOffsetY),
    blur: shadowBlur,
    color: CGColor(gray: 0, alpha: shadowOpacity)
)
context.addPath(tilePath)
context.setFillColor(CGColor(gray: 1, alpha: 1))
context.fillPath()
context.restoreGState()

let tileGradient = CGGradient(
    colorsSpace: srgb,
    colors: [CGColor(gray: 1.0, alpha: 1), CGColor(gray: 0.93, alpha: 1)] as CFArray,
    locations: [0, 1]
)!
context.saveGState()
context.addPath(tilePath)
context.clip()
context.drawLinearGradient(
    tileGradient,
    start: CGPoint(x: tileRect.midX, y: tileRect.maxY),
    end: CGPoint(x: tileRect.midX, y: tileRect.minY),
    options: []
)
context.restoreGState()

let glyphAspect = CGFloat(glyphImage.width) / CGFloat(glyphImage.height)
let glyphHeight = tileRect.height * glyphScale
let glyphWidth = glyphHeight * glyphAspect
let glyphRect = CGRect(
    x: tileRect.midX - glyphWidth / 2,
    y: tileRect.midY - glyphHeight / 2,
    width: glyphWidth,
    height: glyphHeight
)
context.interpolationQuality = .high
context.draw(glyphImage, in: glyphRect)

guard let composed = context.makeImage(),
      let png = NSBitmapImageRep(cgImage: composed).representation(using: .png, properties: [:]) else {
    fail("cannot encode icon", code: 70)
}

do {
    try png.write(to: URL(fileURLWithPath: arguments[2]))
} catch {
    fail("cannot write \(arguments[2]): \(error)", code: 73)
}
