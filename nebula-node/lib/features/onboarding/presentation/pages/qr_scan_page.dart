import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:mobile_scanner/mobile_scanner.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../../../config/router.dart';
import '../../domain/entities/qr_payload.dart';
import '../../../engine/presentation/providers/engine_provider.dart';
import '../providers/onboarding_provider.dart';
import '../widgets/permission_checker.dart';

/// Full-screen camera QR scanner page.
///
/// Displays the camera viewfinder with a scanning frame overlay. On QR
/// detection, parses the JSON payload, configures the cluster, and navigates
/// to the status page.
class QrScanPage extends ConsumerStatefulWidget {
  const QrScanPage({super.key});

  @override
  ConsumerState<QrScanPage> createState() => _QrScanPageState();
}

class _QrScanPageState extends ConsumerState<QrScanPage> {
  bool _processing = false;
  bool _torchOn = false;

  @override
  void dispose() {
    ref.read(qrScannerSourceProvider).dispose();
    super.dispose();
  }

  void _onBarcodeDetected(BarcodeCapture capture) {
    if (_processing) return;

    final barcode = capture.barcodes.firstOrNull;
    if (barcode == null || barcode.rawValue == null) return;

    _processPayload(barcode.rawValue!);
  }

  Future<void> _processPayload(String raw) async {
    if (_processing) return;
    setState(() => _processing = true);

    final Map<String, dynamic> json;
    try {
      json = jsonDecode(raw) as Map<String, dynamic>;
    } on FormatException {
      if (mounted) {
        NotificationToast.error(context, 'Invalid QR code');
      }
      setState(() => _processing = false);
      return;
    }

    final QrPayload payload;
    try {
      payload = QrPayload.fromJson(json);
    } on Object {
      if (mounted) {
        NotificationToast.error(
          context,
          'Missing required fields: version, cluster_id, server_url, auth_token',
        );
      }
      setState(() => _processing = false);
      return;
    }

    final repository = ref.read(engineRepositoryProvider);
    await repository.configureCluster(
      payload.clusterId,
      payload.serverUrl,
      payload.authToken,
    );

    ref.invalidate(nodeStatusProvider);
    ref.invalidate(isConfiguredProvider);

    if (mounted) {
      NotificationToast.success(context, 'Joined cluster successfully!');
      Navigator.pushNamedAndRemoveUntil(
        context,
        AppRoutes.status,
        (_) => false,
      );
    }
  }

  Future<void> _toggleTorch() async {
    final scannerSource = ref.read(qrScannerSourceProvider);
    await scannerSource.toggleTorch();
    setState(() => _torchOn = !_torchOn);
  }

  void _showManualEntry() {
    AppAlertDialog.showWithInput(
      context: context,
      title: 'Enter QR Payload',
      message: 'Paste the cluster configuration JSON below.',
      hintText:
          '{"version":1,"cluster_id":"...","server_url":"...","auth_token":"..."}',
      actionText: 'Join',
      multiLine: true,
      maxLines: 6,
      validator: (value) {
        if (value.isEmpty) return 'Payload cannot be empty';
        try {
          jsonDecode(value);
        } on FormatException {
          return 'Invalid JSON format';
        }
        return null;
      },
      onActionPressed: (value) => _processPayload(value),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final scannerSource = ref.watch(qrScannerSourceProvider);

    return PermissionChecker(
      child: Scaffold(
        extendBodyBehindAppBar: true,
        appBar: BlurredAppBar(
          title: 'Scan QR Code',
          centerTitle: true,
          backgroundOpacity: 0.6,
          actions: [
            IconButton(
              icon: const Icon(Icons.close),
              onPressed: () => Navigator.pop(context),
            ),
          ],
        ),
        body: Stack(
          children: [
            // Camera viewfinder
            MobileScanner(
              controller: scannerSource.controller,
              onDetect: _onBarcodeDetected,
            ),

            // Scanning overlay
            _ScanOverlay(processing: _processing),

            // Bottom controls
            Positioned(
              left: 0,
              right: 0,
              bottom: 0,
              child: Container(
                padding: EdgeInsets.only(
                  left: UIConstants.spacingXL,
                  right: UIConstants.spacingXL,
                  top: UIConstants.spacingLG,
                  bottom: MediaQuery.of(context).padding.bottom +
                      UIConstants.spacingXL,
                ),
                decoration: BoxDecoration(
                  gradient: LinearGradient(
                    begin: Alignment.topCenter,
                    end: Alignment.bottomCenter,
                    colors: [
                      Colors.transparent,
                      Colors.black.withValues(alpha: 0.8),
                    ],
                  ),
                ),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    if (_processing)
                      const Padding(
                        padding:
                            EdgeInsets.only(bottom: UIConstants.spacingLG),
                        child: ProgressBar(progress: 1.0),
                      ),
                    Row(
                      mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                      children: [
                        // Torch toggle
                        IconButton.filled(
                          onPressed: _toggleTorch,
                          style: IconButton.styleFrom(
                            backgroundColor: _torchOn
                                ? theme.colorScheme.primary
                                : theme.colorScheme.surface
                                    .withValues(alpha: 0.3),
                          ),
                          icon: Icon(
                            _torchOn ? Icons.flash_on : Icons.flash_off,
                            color: _torchOn
                                ? theme.colorScheme.onPrimary
                                : Colors.white,
                          ),
                        ),

                        // Manual entry
                        TextButton.icon(
                          onPressed: _processing ? null : _showManualEntry,
                          icon: const Icon(
                            Icons.keyboard,
                            color: Colors.white70,
                          ),
                          label: Text(
                            'Enter manually',
                            style: theme.textTheme.bodyMedium?.copyWith(
                              color: Colors.white70,
                            ),
                          ),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Semi-transparent overlay with a scanning frame cutout.
class _ScanOverlay extends StatelessWidget {
  final bool processing;

  const _ScanOverlay({required this.processing});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return LayoutBuilder(
      builder: (context, constraints) {
        final scanAreaSize = constraints.maxWidth * 0.7;
        final scanAreaTop =
            (constraints.maxHeight - scanAreaSize) / 2 - 40;

        return Stack(
          children: [
            // Dark overlay with cutout
            ColorFiltered(
              colorFilter: ColorFilter.mode(
                Colors.black.withValues(alpha: 0.5),
                BlendMode.srcOut,
              ),
              child: Stack(
                children: [
                  Container(
                    decoration: const BoxDecoration(
                      color: Colors.black,
                      backgroundBlendMode: BlendMode.dstOut,
                    ),
                  ),
                  Positioned(
                    top: scanAreaTop,
                    left: (constraints.maxWidth - scanAreaSize) / 2,
                    child: Container(
                      width: scanAreaSize,
                      height: scanAreaSize,
                      decoration: BoxDecoration(
                        color: Colors.red, // Any color works with srcOut
                        borderRadius:
                            BorderRadius.circular(UIConstants.radiusLarge),
                      ),
                    ),
                  ),
                ],
              ),
            ),

            // Scanning frame border
            Positioned(
              top: scanAreaTop,
              left: (constraints.maxWidth - scanAreaSize) / 2,
              child: Container(
                width: scanAreaSize,
                height: scanAreaSize,
                decoration: BoxDecoration(
                  borderRadius:
                      BorderRadius.circular(UIConstants.radiusLarge),
                  border: Border.all(
                    color: processing
                        ? theme.colorScheme.primary
                        : Colors.white.withValues(alpha: 0.6),
                    width: 2,
                  ),
                ),
                child: Column(
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    Icon(
                      Icons.qr_code_scanner,
                      size: 48,
                      color: Colors.white.withValues(alpha: 0.4),
                    ),
                    const SizedBox(height: UIConstants.spacingSM),
                    Text(
                      processing
                          ? 'Processing...'
                          : 'Align QR code within frame',
                      style: Theme.of(context).textTheme.bodySmall?.copyWith(
                            color: Colors.white.withValues(alpha: 0.6),
                          ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        );
      },
    );
  }
}
