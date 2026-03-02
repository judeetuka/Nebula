import 'package:mobile_scanner/mobile_scanner.dart';

/// Data source managing the camera controller for QR code scanning.
///
/// The actual QR detection happens in the UI via the [MobileScanner] widget.
/// This source owns the [MobileScannerController] lifecycle.
class QrScannerSource {
  MobileScannerController? _controller;

  /// Returns the active scanner controller, creating one if needed.
  MobileScannerController get controller {
    _controller ??= MobileScannerController(
      detectionSpeed: DetectionSpeed.normal,
      facing: CameraFacing.back,
    );
    return _controller!;
  }

  /// Toggles the device torch on/off.
  Future<void> toggleTorch() async {
    await controller.toggleTorch();
  }

  /// Disposes the camera controller and releases resources.
  void dispose() {
    _controller?.dispose();
    _controller = null;
  }
}
