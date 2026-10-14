// cutout <in.jpg> <out.png>: keep only the photo's subject (the pen and its cap) on a transparent background,
// cropped to it with a small margin. macOS 14+ only (Vision foreground instance mask). Built and run by
// scripts/make-cutouts.sh; nothing here touches the original photo.
import Foundation
import Vision
import CoreImage
import ImageIO
import UniformTypeIdentifiers

let args = CommandLine.arguments
guard args.count == 3 else { FileHandle.standardError.write("usage: cutout in out.png\n".data(using: .utf8)!); exit(2) }
let inURL = URL(fileURLWithPath: args[1]), outURL = URL(fileURLWithPath: args[2])
guard let src = CIImage(contentsOf: inURL, options: [.applyOrientationProperty: true]) else { print("cannot read"); exit(1) }
let handler = VNImageRequestHandler(ciImage: src)
let req = VNGenerateForegroundInstanceMaskRequest()
try handler.perform([req])
guard let obs = req.results?.first, !obs.allInstances.isEmpty else { print("no subject found"); exit(3) }
let maskBuf = try obs.generateScaledMaskForImage(forInstances: obs.allInstances, from: handler)
let mask = CIImage(cvPixelBuffer: maskBuf)
let clear = CIImage(color: .clear).cropped(to: src.extent)
let out = src.applyingFilter("CIBlendWithMask", parameters: [kCIInputBackgroundImageKey: clear, kCIInputMaskImageKey: mask])
let ctx = CIContext()
// crop to the subject with a small margin so the pen fills the frame
var box = out.extent
if let cg = ctx.createCGImage(mask, from: mask.extent) {
  let w = cg.width, h = cg.height
  var data = [UInt8](repeating: 0, count: w*h)
  let cs = CGColorSpaceCreateDeviceGray()
  let c = CGContext(data: &data, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w, space: cs, bitmapInfo: 0)!
  c.draw(cg, in: CGRect(x: 0, y: 0, width: w, height: h))
  var minX = w, minY = h, maxX = 0, maxY = 0
  for y in 0..<h { for x in 0..<w where data[y*w+x] > 24 { minX = min(minX,x); maxX = max(maxX,x); minY = min(minY,y); maxY = max(maxY,y) } }
  if maxX > minX && maxY > minY {
    let pad = CGFloat(max(w,h)) * 0.02
    // CGContext rows run top-down in `data`; CIImage y runs bottom-up
    box = CGRect(x: CGFloat(minX) - pad, y: CGFloat(h - 1 - maxY) - pad, width: CGFloat(maxX - minX) + 2*pad, height: CGFloat(maxY - minY) + 2*pad).intersection(out.extent)
  }
}
guard let cgOut = ctx.createCGImage(out, from: box, format: .RGBA8, colorSpace: CGColorSpace(name: CGColorSpace.sRGB)) else { print("render failed"); exit(1) }
guard let dest = CGImageDestinationCreateWithURL(outURL as CFURL, UTType.png.identifier as CFString, 1, nil) else { exit(1) }
CGImageDestinationAddImage(dest, cgOut, nil)
guard CGImageDestinationFinalize(dest) else { print("encode failed"); exit(4) }
print("ok \(cgOut.width)x\(cgOut.height) instances=\(obs.allInstances.count)")
