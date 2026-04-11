import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';
import 'package:qr_flutter/qr_flutter.dart';

class QrDisplay extends StatelessWidget {
  final String clusterId;
  final String serverUrl;
  final String authToken;

  const QrDisplay({
    super.key,
    required this.clusterId,
    required this.serverUrl,
    this.authToken = '',
  });

  String get _payload => jsonEncode({
        'v': 1,
        'c': clusterId,
        's': serverUrl,
        't': authToken,
      });

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: UIConstants.paddingLG,
      decoration: BoxDecoration(
        color: Colors.white,
        borderRadius: BorderRadius.circular(UIConstants.radiusLarge),
      ),
      child: QrImageView(
        data: _payload,
        version: QrVersions.auto,
        size: 250,
        backgroundColor: Colors.white,
        eyeStyle: const QrEyeStyle(
          eyeShape: QrEyeShape.square,
          color: Color(0xFF6C3CE0),
        ),
        dataModuleStyle: const QrDataModuleStyle(
          dataModuleShape: QrDataModuleShape.square,
          color: Color(0xFF6C3CE0),
        ),
      ),
    );
  }
}
