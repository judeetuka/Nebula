import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';
import 'package:permission_handler/permission_handler.dart';

/// Widget that checks and requests runtime permissions before showing
/// its child content.
///
/// Wraps any child widget requiring camera (or other) permissions. When all
/// required permissions are granted the child is displayed; otherwise a
/// request screen with [ActionTile] rows per permission is shown.
class PermissionChecker extends StatefulWidget {
  final Widget child;
  final List<Permission> requiredPermissions;

  const PermissionChecker({
    super.key,
    required this.child,
    this.requiredPermissions = const [Permission.camera],
  });

  @override
  State<PermissionChecker> createState() => _PermissionCheckerState();
}

class _PermissionCheckerState extends State<PermissionChecker>
    with WidgetsBindingObserver {
  bool _allGranted = false;
  bool _checking = true;
  Map<Permission, PermissionStatus> _statuses = {};

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _checkPermissions();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  /// Re-check permissions when the app returns from Settings.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed && !_allGranted) {
      _checkPermissions();
    }
  }

  Future<void> _checkPermissions() async {
    setState(() => _checking = true);

    final statuses = <Permission, PermissionStatus>{};
    for (final permission in widget.requiredPermissions) {
      statuses[permission] = await permission.status;
    }

    final allGranted = statuses.values.every((s) => s.isGranted);

    setState(() {
      _statuses = statuses;
      _allGranted = allGranted;
      _checking = false;
    });
  }

  Future<void> _requestPermissions() async {
    final denied = _statuses.entries
        .where((e) => !e.value.isGranted)
        .map((e) => e.key)
        .toList();

    // Check if any are permanently denied before requesting.
    final permanentlyDenied = _statuses.entries
        .where((e) => e.value.isPermanentlyDenied)
        .map((e) => e.key)
        .toList();

    if (permanentlyDenied.isNotEmpty && mounted) {
      AppAlertDialog.showInfo(
        context: context,
        title: 'Permissions Required',
        message:
            'Some permissions have been permanently denied. '
            'Please open Settings and grant ${_permissionLabel(permanentlyDenied.first)} '
            'permission manually.',
      );
      await openAppSettings();
      return;
    }

    final results = await denied.request();

    final newAllGranted = results.values.every((s) => s.isGranted);
    if (newAllGranted && mounted) {
      NotificationToast.success(context, 'Permissions granted');
    } else if (mounted) {
      final stillDenied = results.entries
          .where((e) => !e.value.isGranted)
          .map((e) => _permissionLabel(e.key))
          .join(', ');
      NotificationToast.warning(
        context,
        'Still missing: $stillDenied',
      );
    }

    await _checkPermissions();
  }

  String _permissionLabel(Permission permission) {
    if (permission == Permission.camera) return 'Camera';
    if (permission == Permission.microphone) return 'Microphone';
    if (permission == Permission.location) return 'Location';
    if (permission == Permission.storage) return 'Storage';
    if (permission == Permission.notification) return 'Notifications';
    return permission.toString().split('.').last;
  }

  IconData _permissionIcon(Permission permission) {
    if (permission == Permission.camera) return Icons.camera_alt_outlined;
    if (permission == Permission.microphone) return Icons.mic_outlined;
    if (permission == Permission.location) return Icons.location_on_outlined;
    if (permission == Permission.storage) return Icons.folder_outlined;
    if (permission == Permission.notification) {
      return Icons.notifications_outlined;
    }
    return Icons.security_outlined;
  }

  @override
  Widget build(BuildContext context) {
    if (_checking) {
      return const Scaffold(
        body: Center(child: CircularProgressIndicator()),
      );
    }

    if (_allGranted) {
      return widget.child;
    }

    return _buildDeniedView(context);
  }

  Widget _buildDeniedView(BuildContext context) {
    final theme = Theme.of(context);

    return Scaffold(
      appBar: BlurredAppBar(
        title: 'Permissions',
        centerTitle: true,
      ),
      body: Padding(
        padding: UIConstants.paddingXL,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const SizedBox(height: UIConstants.spacingXL),
            Icon(
              Icons.shield_outlined,
              size: 64,
              color: theme.colorScheme.primary,
            ),
            const SizedBox(height: UIConstants.spacingXL),
            Text(
              'Permissions Needed',
              style: theme.textTheme.headlineSmall?.copyWith(
                fontWeight: FontWeight.bold,
              ),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: UIConstants.spacingSM),
            Text(
              'The following permissions are required to scan QR codes and join a cluster.',
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: UIConstants.spacingXXL),

            // Permission list
            ...widget.requiredPermissions.map((permission) {
              final status = _statuses[permission];
              final granted = status?.isGranted ?? false;
              final label = _permissionLabel(permission);

              return Padding(
                padding:
                    const EdgeInsets.only(bottom: UIConstants.spacingXS),
                child: ActionTile(
                  icon: granted
                      ? Icons.check_circle
                      : _permissionIcon(permission),
                  title: granted ? '$label (granted)' : label,
                  onTap: () {},
                ),
              );
            }),

            const Spacer(),

            SizedBox(
              height: UIConstants.buttonLG,
              child: FilledButton.icon(
                onPressed: _requestPermissions,
                icon: const Icon(Icons.shield_outlined),
                label: const Text('Grant Permissions'),
              ),
            ),
            const SizedBox(height: UIConstants.spacingLG),
          ],
        ),
      ),
    );
  }
}
