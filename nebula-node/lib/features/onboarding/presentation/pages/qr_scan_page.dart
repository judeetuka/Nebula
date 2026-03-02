import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:nebula_ui/nebula_ui.dart';

import '../../../../config/router.dart';
import '../../domain/entities/qr_payload.dart';
import '../../../engine/presentation/providers/engine_provider.dart';

/// Placeholder QR scan page.
///
/// Uses a manual JSON text field until mobile_scanner is integrated.
class QrScanPage extends ConsumerStatefulWidget {
  const QrScanPage({super.key});

  @override
  ConsumerState<QrScanPage> createState() => _QrScanPageState();
}

class _QrScanPageState extends ConsumerState<QrScanPage> {
  final _controller = TextEditingController();
  String? _error;
  bool _loading = false;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Future<void> _handleSubmit() async {
    setState(() {
      _error = null;
      _loading = true;
    });

    final text = _controller.text.trim();
    if (text.isEmpty) {
      setState(() {
        _error = 'Please enter the QR payload JSON.';
        _loading = false;
      });
      return;
    }

    final Map<String, dynamic> json;
    try {
      json = jsonDecode(text) as Map<String, dynamic>;
    } on FormatException {
      setState(() {
        _error = 'Invalid JSON format.';
        _loading = false;
      });
      return;
    }

    final QrPayload payload;
    try {
      payload = QrPayload.fromJson(json);
    } on Object {
      setState(() {
        _error = 'Missing required fields: version, cluster_id, server_url, auth_token.';
        _loading = false;
      });
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
      Navigator.pushNamedAndRemoveUntil(
        context,
        AppRoutes.status,
        (_) => false,
      );
    }

    setState(() => _loading = false);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Scaffold(
      appBar: AppBar(
        title: const Text('Join Cluster'),
        centerTitle: true,
      ),
      body: SingleChildScrollView(
        padding: UIConstants.paddingXL,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Icon(
              Icons.qr_code_2,
              size: 64,
              color: theme.colorScheme.primary,
            ),
            const SizedBox(height: UIConstants.spacingXL),
            Text(
              'Camera scanner coming soon.\nPaste the QR payload JSON below to join a cluster.',
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: UIConstants.spacingXL),
            TextField(
              controller: _controller,
              maxLines: 8,
              decoration: InputDecoration(
                labelText: 'QR Payload JSON',
                hintText:
                    '{"version":1,"cluster_id":"...","server_url":"...","auth_token":"..."}',
                errorText: _error,
                border: const OutlineInputBorder(),
              ),
            ),
            const SizedBox(height: UIConstants.spacingXL),
            SizedBox(
              height: UIConstants.buttonLG,
              child: FilledButton(
                onPressed: _loading ? null : _handleSubmit,
                child: _loading
                    ? const SizedBox(
                        width: 20,
                        height: 20,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Text('Join Cluster'),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
