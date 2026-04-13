import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:manny_ui/manny_ui.dart';

import '../providers/cluster_provider.dart';

class CreateClusterPage extends ConsumerStatefulWidget {
  const CreateClusterPage({super.key});

  @override
  ConsumerState<CreateClusterPage> createState() => _CreateClusterPageState();
}

class _CreateClusterPageState extends ConsumerState<CreateClusterPage> {
  final _nameController = TextEditingController();
  final _formKey = GlobalKey<FormState>();
  bool _isCreating = false;

  @override
  void dispose() {
    _nameController.dispose();
    super.dispose();
  }

  Future<void> _handleCreate() async {
    if (!_formKey.currentState!.validate()) return;

    setState(() => _isCreating = true);

    final success = await ref
        .read(clustersProvider.notifier)
        .createCluster(name: _nameController.text.trim());

    if (!mounted) return;

    setState(() => _isCreating = false);

    if (success) {
      Navigator.of(context).pop();
    } else {
      final error = ref.read(clustersProvider).error;
      if (error != null) {
        NotificationToast.error(context, error);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return FrostedScaffold(
      title: 'New Cluster',
      body: Center(
        child: SingleChildScrollView(
          padding: const EdgeInsets.symmetric(
            horizontal: UIConstants.spacingXL,
            vertical: UIConstants.spacingXXL,
          ),
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 460),
            child: FrostedGlass(
              borderRadius: BorderRadius.circular(20),
              padding: const EdgeInsets.all(UIConstants.spacingXL),
              child: Form(
                key: _formKey,
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    // Icon header
                    Center(
                      child: Container(
                        width: 64,
                        height: 64,
                        decoration: BoxDecoration(
                          color: theme.colorScheme.primary.withValues(alpha: 0.12),
                          borderRadius: BorderRadius.circular(20),
                        ),
                        child: Icon(
                          IconlyBold.plus,
                          size: 30,
                          color: theme.colorScheme.primary,
                        ),
                      ),
                    ),
                    const SizedBox(height: UIConstants.spacingXL),

                    // Title
                    Text(
                      'Create Compute Cluster',
                      style: theme.textTheme.titleLarge?.copyWith(
                        fontWeight: FontWeight.bold,
                        letterSpacing: -0.5,
                      ),
                      textAlign: TextAlign.center,
                    ),
                    const SizedBox(height: UIConstants.spacingSM),

                    // Subtitle
                    Text(
                      'Give your cluster a name to get started',
                      style: theme.textTheme.bodyMedium?.copyWith(
                        color: theme.colorScheme.onSurface.withValues(alpha: 0.5),
                      ),
                      textAlign: TextAlign.center,
                    ),
                    const SizedBox(height: UIConstants.spacingXXL),

                    // Name field inside a frosted container
                    FrostedGlass(
                      borderRadius: BorderRadius.circular(UIConstants.radiusMedium),
                      padding: const EdgeInsets.symmetric(
                        horizontal: UIConstants.spacingLG,
                        vertical: UIConstants.spacingXS,
                      ),
                      shadow: false,
                      opacity: 0.08,
                      child: TextFormField(
                        controller: _nameController,
                        decoration: InputDecoration(
                          labelText: 'Cluster Name',
                          hintText: 'e.g. Production Alpha',
                          prefixIcon: Icon(
                            IconlyBroken.discovery,
                            color: theme.colorScheme.primary.withValues(alpha: 0.7),
                          ),
                          border: InputBorder.none,
                          enabledBorder: InputBorder.none,
                          focusedBorder: InputBorder.none,
                          errorBorder: InputBorder.none,
                          focusedErrorBorder: InputBorder.none,
                          contentPadding: const EdgeInsets.symmetric(
                            vertical: UIConstants.spacingMD,
                          ),
                        ),
                        textInputAction: TextInputAction.done,
                        autofocus: true,
                        onFieldSubmitted: (_) => _handleCreate(),
                        validator: (value) {
                          if (value == null || value.trim().isEmpty) {
                            return 'Cluster name is required';
                          }
                          if (value.trim().length < 3) {
                            return 'Name must be at least 3 characters';
                          }
                          return null;
                        },
                      ),
                    ),
                    const SizedBox(height: UIConstants.spacingXL),

                    // Create button
                    SizedBox(
                      height: UIConstants.buttonLG,
                      child: FilledButton(
                        onPressed: _isCreating ? null : _handleCreate,
                        style: FilledButton.styleFrom(
                          shape: RoundedRectangleBorder(
                            borderRadius: BorderRadius.circular(
                              UIConstants.radiusMedium,
                            ),
                          ),
                        ),
                        child: _isCreating
                            ? SizedBox(
                                height: 20,
                                width: 20,
                                child: CupertinoActivityIndicator(
                                  color: theme.colorScheme.onPrimary,
                                ),
                              )
                            : Row(
                                mainAxisAlignment: MainAxisAlignment.center,
                                children: [
                                  Icon(IconlyBold.plus, size: 18,
                                    color: theme.colorScheme.onPrimary),
                                  const SizedBox(width: UIConstants.spacingSM),
                                  Text(
                                    'Create Cluster',
                                    style: TextStyle(
                                      fontWeight: FontWeight.w700,
                                      color: theme.colorScheme.onPrimary,
                                    ),
                                  ),
                                ],
                              ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
