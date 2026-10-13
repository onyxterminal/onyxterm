import AppKit

let canvasWidth: CGFloat = 540
let canvasHeight: CGFloat = 360
let glyphMaxSide: CGFloat = 220
let topInset: CGFloat = 70

func fail(_ message: String, code: Int32) -> Never {
    FileHandle.standardError.write(Data((message + "\n").utf8))
    exit(code)
}

let arguments = CommandLine.arguments
guard arguments.count == 3 else {
    fail("usage: compose-dmg-background.swift <glyph.png> <output.png>", code: 64)
}

guard let glyph = NSImage(contentsOfFile: arguments[1]),
      let glyphImage = glyph.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
    fail("cannot read glyph \(arguments[1])", code: 66)
}

let srgb = CGColorSpace(name: CGColorSpace.sRGB)!
let pixelsWide = Int(canvasWidth)
let pixelsHigh = Int(canvasHeight)
guard let context = CGContext(
    data: nil,
    width: pixelsWide,
    height: pixelsHigh,
    bitsPerComponent: 8,
    bytesPerRow: 0,
    space: srgb,
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
) else {
    fail("cannot create drawing context", code: 70)
}

let background = CGColor(red: 0.08, green: 0.09, blue: 0.11, alpha: 1)
context.setFillColor(background)
context.fill(CGRect(x: 0, y: 0, width: canvasWidth, height: canvasHeight))

let glyphAspect = CGFloat(glyphImage.width) / CGFloat(glyphImage.height)
let glyphHeight = glyphAspect >= 1 ? glyphMaxSide / glyphAspect : glyphMaxSide
let glyphWidth = glyphHeight * glyphAspect
let glyphRect = CGRect(
    x: (canvasWidth - glyphWidth) / 2,
    y: canvasHeight - glyphHeight - topInset,
    width: glyphWidth,
    height: glyphHeight
)
context.interpolationQuality = .high
context.draw(glyphImage, in: glyphRect)

guard let composed = context.makeImage(),
      let png = NSBitmapImageRep(cgImage: composed).representation(using: .png, properties: [:]) else {
    fail("cannot encode background", code: 70)
}

do {
    try png.write(to: URL(fileURLWithPath: arguments[2]))
} catch {
    fail("cannot write \(arguments[2]): \(error)", code: 73)
}
