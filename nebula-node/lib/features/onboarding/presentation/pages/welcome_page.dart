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
              const Spacer(),

              // Onboarding step tracker
              StepTracker(
                steps: const [
                  TrackerStep(
                    label: 'Grant Permissions',
                    description: 'Allow camera access for QR scanning',
                    icon: Icons.shield_outlined,
                    status: StepStatus.active,
                  ),
                  TrackerStep(
                    label: 'Scan QR Code',
                    description: 'Scan a cluster configuration QR code',
                    icon: Icons.qr_code_scanner,
                    status: StepStatus.pending,
                  ),
                  TrackerStep(
                    label: 'Connected',
                    description: 'Node joins the compute cluster',
                    icon: Icons.check_circle_outline,
                    status: StepStatus.pending,
                  ),
                ],
              ),

              const Spacer(),

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
              const SizedBox(height: UIConstants.spacingLG),
            ],
          ),
        ),
      ),
    );
  }
}
