#!/usr/bin/env swift

import AppKit
import Foundation

guard CommandLine.arguments.count == 4 else {
    fputs("usage: generate-windows-icons.swift APP_IMAGE TRAY_IMAGE OUTPUT_DIR\n", stderr)
    exit(2)
}

let appURL = URL(fileURLWithPath: CommandLine.arguments[1])
let trayURL = URL(fileURLWithPath: CommandLine.arguments[2])
let outputURL = URL(fileURLWithPath: CommandLine.arguments[3], isDirectory: true)

guard let appImage = NSImage(contentsOf: appURL), let trayImage = NSImage(contentsOf: trayURL) else {
    fputs("failed to load rendered icon PNGs\n", stderr)
    exit(1)
}

func render(_ image: NSImage, size: Int) -> NSBitmapImageRep {
    let bitmap = NSBitmapImageRep(
        bitmapDataPlanes: nil,
        pixelsWide: size,
        pixelsHigh: size,
        bitsPerSample: 8,
        samplesPerPixel: 4,
        hasAlpha: true,
        isPlanar: false,
        colorSpaceName: .deviceRGB,
        bitmapFormat: [],
        bytesPerRow: size * 4,
        bitsPerPixel: 32
    )!
    NSGraphicsContext.saveGraphicsState()
    let context = NSGraphicsContext(bitmapImageRep: bitmap)!
    context.imageInterpolation = .high
    NSGraphicsContext.current = context
    context.cgContext.clear(CGRect(x: 0, y: 0, width: size, height: size))
    image.draw(
        in: NSRect(x: 0, y: 0, width: size, height: size),
        from: .zero,
        operation: .sourceOver,
        fraction: 1
    )
    context.flushGraphics()
    NSGraphicsContext.restoreGraphicsState()
    return bitmap
}

func png(_ image: NSImage, size: Int) -> Data {
    render(image, size: size).representation(using: .png, properties: [:])!
}

func appendLE<T: FixedWidthInteger>(_ value: T, to data: inout Data) {
    var littleEndian = value.littleEndian
    withUnsafeBytes(of: &littleEndian) { data.append(contentsOf: $0) }
}

let sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
let images = sizes.map { png(appImage, size: $0) }
var ico = Data()
appendLE(UInt16(0), to: &ico)
appendLE(UInt16(1), to: &ico)
appendLE(UInt16(sizes.count), to: &ico)

var offset = 6 + (16 * sizes.count)
for (size, image) in zip(sizes, images) {
    ico.append(UInt8(size == 256 ? 0 : size))
    ico.append(UInt8(size == 256 ? 0 : size))
    ico.append(0)
    ico.append(0)
    appendLE(UInt16(1), to: &ico)
    appendLE(UInt16(32), to: &ico)
    appendLE(UInt32(image.count), to: &ico)
    appendLE(UInt32(offset), to: &ico)
    offset += image.count
}
for image in images {
    ico.append(image)
}
try ico.write(to: outputURL.appendingPathComponent("MonitorSwitch.ico"))

let tray = render(trayImage, size: 32)
let byteCount = tray.bytesPerRow * tray.pixelsHigh
var trayData = Data(bytes: tray.bitmapData!, count: byteCount)
trayData.withUnsafeMutableBytes { bytes in
    let pixels = bytes.bindMemory(to: UInt8.self)
    for index in stride(from: 0, to: pixels.count, by: 4) {
        let alpha = Int(pixels[index + 3])
        guard alpha > 0 && alpha < 255 else { continue }
        for channel in 0..<3 {
            pixels[index + channel] = UInt8(min(255, Int(pixels[index + channel]) * 255 / alpha))
        }
    }
}
try trayData.write(to: outputURL.appendingPathComponent("TrayIcon32.rgba"))
try tray.representation(using: .png, properties: [:])!
    .write(to: outputURL.appendingPathComponent("TrayIcon32.png"))
