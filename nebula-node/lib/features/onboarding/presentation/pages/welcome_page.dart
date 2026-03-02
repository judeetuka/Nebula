import 'package:flutter/material.dart';
import 'package:nebula_ui/nebula_ui.dart';

import '../../../../config/router.dart';

class WelcomePage extends StatelessWidget {
  const WelcomePage({super.key});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Scaffold(
      body: SafeArea(
        child: Padding(
          padding: UIConstants.paddingXL,
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              const Spacer(flex: 2),
              Icon(
                Icons.cloud_outlined,
                size: 96,
                color: theme.colorScheme.primary,
              ),
              const SizedBox(height: UIConstants.spacingXL),
              Text(
                'NEBULA',
                style: theme.textTheme.headlineLarge?.copyWith(
                  fontWeight: FontWeight.bold,
                  color: theme.colorScheme.primary,
                  letterSpacing: 4,
                ),
              ),
              const SizedBox(height: UIConstants.spacingSM),
              Text(
                'Distributed Compute Node',
                style: theme.textTheme.titleMedium?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
              const Spacer(flex: 2),
              Text(
                'Scan a QR code to join a cluster',
                style: theme.textTheme.bodyLarge?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: UIConstants.spacingXL),
              SizedBox(
                width: double.infinity,
                height: UIConstants.buttonLG,
                child: FilledButton.icon(
                  onPressed: () {
                    Navigator.pushNamed(context, AppRoutes.scan);
                  },
                  icon: const Icon(Icons.qr_code_scanner),
                  label: const Text('Scan QR Code'),
                ),
              ),
              const Spacer(),
            ],
          ),
        ),
      ),
    );
  }
}
