/// Data source for QR code scanning.
///
/// Currently a stub. The real camera-based scanner using mobile_scanner
/// will be integrated in a future phase.
class QrScannerSource {
  /// Simulates scanning a QR code and returning its raw string content.
  ///
  /// In production, this will open the device camera and decode a QR code.
  Future<String> scanQrCode() async {
    await Future<void>.delayed(const Duration(milliseconds: 500));
    // Stub: return a sample QR payload JSON.
    return '{"version":1,"cluster_id":"cluster-demo-001","server_url":"https://nebula.example.com","auth_token":"stub-token-abc123"}';
  }
}
