import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../../../config/router.dart';

class WelcomePage extends StatelessWidget {
  const WelcomePage({super.key});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return FrostedScaffold(
      title: '',
      automaticallyImplyLeading: false,
      body: SafeArea(
        child: Padding(
          padding: UIConstants.paddingXL,
          child: Column(
            children: [
              const Spacer(flex: 2),

              // Logo in a frosted glass circle
              FrostedGlass(
                borderRadius: BorderRadius.circular(UIConstants.radiusCircle),
                tintColor: theme.colorScheme.primary,
                opacity: 0.08,
                padding: const EdgeInsets.all(28),
                child: Icon(
                  IconlyBold.discovery,
                  size: 56,
                  color: theme.colorScheme.primary,
                ),
              ),
              const SizedBox(height: UIConstants.spacingXXL),

              // Title
              Text(
                'NEBULA',
                style: theme.textTheme.headlineLarge?.copyWith(
                  fontWeight: FontWeight.w900,
                  color: theme.colorScheme.primary,
                  letterSpacing: 6,
                ),
              ),
              const SizedBox(height: UIConstants.spacingSM),
              Text(
                'Distributed Compute Node',
                style: theme.textTheme.titleMedium?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                  fontWeight: FontWeight.w400,
                  letterSpacing: 1,
                ),
              ),
              const Spacer(),

              // Onboarding step tracker
              FrostedGlass(
                borderRadius: BorderRadius.circular(UIConstants.radiusLarge),
                padding: const EdgeInsets.all(20),
                child: StepTracker(
                  steps: const [
                    TrackerStep(
                      label: 'Grant Permissions',
                      description: 'Allow camera access for QR scanning',
                      icon: IconlyBroken.shield_done,
                      status: StepStatus.active,
                    ),
                    TrackerStep(
                      label: 'Scan QR Code',
                      description: 'Scan a cluster configuration QR code',
                      icon: IconlyBroken.scan,
                      status: StepStatus.pending,
                    ),
                    TrackerStep(
                      label: 'Connected',
                      description: 'Node joins the compute cluster',
                      icon: IconlyBroken.tick_square,
                      status: StepStatus.pending,
                    ),
                  ],
                ),
              ),

              const Spacer(),

              // Scan button
              SizedBox(
                width: double.infinity,
                height: UIConstants.buttonLG,
                child: FrostedGlass(
                  borderRadius: BorderRadius.circular(UIConstants.radiusMedium),
                  tintColor: theme.colorScheme.primary,
                  opacity: 0.15,
                  padding: EdgeInsets.zero,
                  child: InkWell(
                    borderRadius:
                        BorderRadius.circular(UIConstants.radiusMedium),
                    onTap: () {
                      Navigator.pushNamed(context, AppRoutes.scan);
                    },
                    child: Center(
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Icon(
                            IconlyBold.scan,
                            color: theme.colorScheme.primary,
                            size: UIConstants.iconLG,
                          ),
                          const SizedBox(width: UIConstants.spacingMD),
                          Text(
                            'Scan QR Code',
                            style: theme.textTheme.titleSmall?.copyWith(
                              color: theme.colorScheme.primary,
                              fontWeight: FontWeight.w700,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
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
