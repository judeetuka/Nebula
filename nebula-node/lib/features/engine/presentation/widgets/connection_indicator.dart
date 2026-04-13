import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';

/// A compact frosted glass pill that shows connection status.
///
/// Displays an animated pulsing dot (green when connected, grey when
/// disconnected) alongside a status label.
class ConnectionIndicator extends StatefulWidget {
  final bool isActive;

  const ConnectionIndicator({super.key, required this.isActive});

  @override
  State<ConnectionIndicator> createState() => _ConnectionIndicatorState();
}

class _ConnectionIndicatorState extends State<ConnectionIndicator>
    with SingleTickerProviderStateMixin {
  late final AnimationController _pulseController;
  late final Animation<double> _pulseAnimation;

  @override
  void initState() {
    super.initState();
    _pulseController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1500),
    );
    _pulseAnimation = Tween<double>(begin: 0.6, end: 1.0).animate(
      CurvedAnimation(parent: _pulseController, curve: Curves.easeInOut),
    );
    if (widget.isActive) {
      _pulseController.repeat(reverse: true);
    }
  }

  @override
  void didUpdateWidget(covariant ConnectionIndicator oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.isActive && !_pulseController.isAnimating) {
      _pulseController.repeat(reverse: true);
    } else if (!widget.isActive && _pulseController.isAnimating) {
      _pulseController.stop();
      _pulseController.value = 0.0;
    }
  }

  @override
  void dispose() {
    _pulseController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final color = widget.isActive ? Colors.green : Colors.grey;
    final label = widget.isActive ? 'Connected' : 'Disconnected';
    final icon = widget.isActive ? IconlyBold.shield_done : IconlyBold.shield_fail;

    return Center(
      child: FrostedGlass(
        borderRadius: BorderRadius.circular(UIConstants.radiusXLarge),
        tintColor: color,
        opacity: 0.08,
        padding: const EdgeInsets.symmetric(
          horizontal: UIConstants.spacingLG,
          vertical: UIConstants.spacingSM,
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            AnimatedBuilder(
              animation: _pulseAnimation,
              builder: (context, child) {
                final dotOpacity =
                    widget.isActive ? _pulseAnimation.value : 0.5;
                return Container(
                  width: 10,
                  height: 10,
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    color: color.withValues(alpha: dotOpacity),
                    boxShadow: widget.isActive
                        ? [
                            BoxShadow(
                              color: color.withValues(alpha: 0.5),
                              blurRadius: 8,
                              spreadRadius: 1,
                            ),
                          ]
                        : null,
                  ),
                );
              },
            ),
            const SizedBox(width: UIConstants.spacingSM),
            Icon(icon, size: UIConstants.iconSM, color: color),
            const SizedBox(width: UIConstants.spacingXS),
            Text(
              label,
              style: theme.textTheme.labelMedium?.copyWith(
                color: color,
                fontWeight: FontWeight.w600,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
