import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';

/// Large frosted glass badge that displays the node's current state.
///
/// Color-coded background tint:
///   green = active, blue = configured, orange = idle, red = uninitialized.
/// Includes an animated glow ring when the node is active.
class RoleBadge extends StatefulWidget {
  final String state;

  const RoleBadge({super.key, required this.state});

  @override
  State<RoleBadge> createState() => _RoleBadgeState();
}

class _RoleBadgeState extends State<RoleBadge>
    with SingleTickerProviderStateMixin {
  late final AnimationController _glowController;
  late final Animation<double> _glowAnimation;

  @override
  void initState() {
    super.initState();
    _glowController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 2000),
    );
    _glowAnimation = Tween<double>(begin: 0.25, end: 0.6).animate(
      CurvedAnimation(parent: _glowController, curve: Curves.easeInOut),
    );
    if (widget.state == 'active') {
      _glowController.repeat(reverse: true);
    }
  }

  @override
  void didUpdateWidget(covariant RoleBadge oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.state == 'active' && !_glowController.isAnimating) {
      _glowController.repeat(reverse: true);
    } else if (widget.state != 'active' && _glowController.isAnimating) {
      _glowController.stop();
      _glowController.value = 0.0;
    }
  }

  @override
  void dispose() {
    _glowController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final (icon, color, label) = _resolveState();

    return AnimatedBuilder(
      animation: _glowAnimation,
      builder: (context, child) {
        final glowOpacity =
            widget.state == 'active' ? _glowAnimation.value : 0.0;

        return Container(
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            boxShadow: [
              if (widget.state == 'active')
                BoxShadow(
                  color: color.withValues(alpha: glowOpacity),
                  blurRadius: 32,
                  spreadRadius: 8,
                ),
            ],
          ),
          child: child,
        );
      },
      child: FrostedGlass(
        borderRadius: BorderRadius.circular(UIConstants.radiusCircle),
        tintColor: color,
        opacity: 0.12,
        padding: const EdgeInsets.all(28),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Container(
              width: 64,
              height: 64,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: color.withValues(alpha: 0.15),
                border: Border.all(
                  color: color.withValues(alpha: 0.4),
                  width: 2,
                ),
              ),
              child: Icon(icon, color: color, size: 30),
            ),
            const SizedBox(height: UIConstants.spacingMD),
            Text(
              label.toUpperCase(),
              style: theme.textTheme.titleMedium?.copyWith(
                color: color,
                fontWeight: FontWeight.w700,
                letterSpacing: 2,
              ),
            ),
          ],
        ),
      ),
    );
  }

  (IconData, Color, String) _resolveState() {
    return switch (widget.state) {
      'active' => (IconlyBold.play, Colors.green, 'Active'),
      'configured' => (IconlyBold.setting, Colors.blue, 'Configured'),
      'idle' => (IconlyBold.time_circle, Colors.orange, 'Idle'),
      'uninitialized' => (IconlyBold.danger, Colors.red, 'Uninitialized'),
      _ => (IconlyBold.info_circle, Colors.grey, widget.state),
    };
  }
}
