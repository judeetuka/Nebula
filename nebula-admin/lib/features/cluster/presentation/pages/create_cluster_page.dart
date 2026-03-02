import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:nebula_ui/nebula_ui.dart';

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
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(error)),
        );
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Scaffold(
      appBar: AppBar(
        title: const Text('Create Cluster'),
      ),
      body: Center(
        child: SingleChildScrollView(
          padding: UIConstants.paddingXL,
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 500),
            child: Form(
              key: _formKey,
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Icon(
                    Icons.add_circle_outline,
                    size: 56,
                    color: theme.colorScheme.primary,
                  ),
                  const SizedBox(height: UIConstants.spacingLG),
                  Text(
                    'New Compute Cluster',
                    style: theme.textTheme.headlineSmall?.copyWith(
                      fontWeight: FontWeight.bold,
                    ),
                    textAlign: TextAlign.center,
                  ),
                  const SizedBox(height: UIConstants.spacingSM),
                  Text(
                    'Give your cluster a name to get started',
                    style: theme.textTheme.bodyMedium?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                    textAlign: TextAlign.center,
                  ),
                  const SizedBox(height: UIConstants.spacingXXL),
                  TextFormField(
                    controller: _nameController,
                    decoration: const InputDecoration(
                      labelText: 'Cluster Name',
                      hintText: 'e.g. Production Alpha',
                      prefixIcon: Icon(Icons.cloud_outlined),
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
                  const SizedBox(height: UIConstants.spacingXL),
                  SizedBox(
                    height: UIConstants.buttonLG,
                    child: FilledButton(
                      onPressed: _isCreating ? null : _handleCreate,
                      child: _isCreating
                          ? const SizedBox(
                              height: 20,
                              width: 20,
                              child: CircularProgressIndicator(strokeWidth: 2),
                            )
                          : const Text('Create Cluster'),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
