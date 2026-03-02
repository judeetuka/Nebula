import 'package:flutter/material.dart';
import 'package:nebula_ui/nebula_ui.dart';

/// Widget that checks and requests camera permissions before allowing
/// QR code scanning.
///
/// Currently a stub that always shows the child. Real permission handling
/// will be added when mobile_scanner is integrated.
class PermissionChecker extends StatefulWidget {
  final Widget child;
  final Widget? deniedChild;

  const PermissionChecker({
    super.key,
    required this.child,
    this.deniedChild,
  });

  @override
  State<PermissionChecker> createState() => _PermissionCheckerState();
}

class _PermissionCheckerState extends State<PermissionChecker> {
  bool _granted = true;

  Future<void> _checkPermission() async {
    // Stub: always grant permission.
    // When mobile_scanner is integrated, this will check
    // Permission.camera via permission_handler.
    setState(() => _granted = true);
  }

  @override
  void initState() {
    super.initState();
    _checkPermission();
  }

  @override
  Widget build(BuildContext context) {
    if (_granted) {
      return widget.child;
    }

    return widget.deniedChild ?? _buildDeniedView(context);
  }

  Widget _buildDeniedView(BuildContext context) {
    final theme = Theme.of(context);

    return Center(
      child: Padding(
        padding: UIConstants.paddingXL,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.camera_alt_outlined,
              size: 48,
              color: theme.colorScheme.error,
            ),
            const SizedBox(height: UIConstants.spacingLG),
            Text(
              'Camera permission is required to scan QR codes.',
              style: theme.textTheme.bodyLarge,
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: UIConstants.spacingXL),
            FilledButton(
              onPressed: _checkPermission,
              child: const Text('Grant Permission'),
            ),
          ],
        ),
      ),
    );
  }
}
