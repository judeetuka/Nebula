// coverage:ignore-file
// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'events.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

T _$identity<T>(T value) => value;

final _privateConstructorUsedError = UnsupportedError(
  'It seems like you constructed your class using `MyClass._()`. This constructor is only meant to be used by freezed and you are not supposed to need it nor use it.\nPlease check the documentation here for more information: https://github.com/rrousselGit/freezed#adding-getters-and-methods-to-our-models',
);

/// @nodoc
mixin _$EngineEvent {
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String newState, String? role) stateChanged,
    required TResult Function(int memberCount, String? masterId)
    membershipChanged,
    required TResult Function(String nodeId, int battery, double cpu)
    heartbeatReceived,
    required TResult Function(String taskId, String status) taskUpdate,
    required TResult Function(String pluginId, String action, bool success)
    pluginResult,
    required TResult Function(bool connected) mqttStatus,
    required TResult Function(int connectedPeers) peerMeshStatus,
    required TResult Function(List<String> line) successionUpdated,
    required TResult Function(String message, String source) error,
    required TResult Function(String level, String message) log,
  }) => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String newState, String? role)? stateChanged,
    TResult? Function(int memberCount, String? masterId)? membershipChanged,
    TResult? Function(String nodeId, int battery, double cpu)?
    heartbeatReceived,
    TResult? Function(String taskId, String status)? taskUpdate,
    TResult? Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult? Function(bool connected)? mqttStatus,
    TResult? Function(int connectedPeers)? peerMeshStatus,
    TResult? Function(List<String> line)? successionUpdated,
    TResult? Function(String message, String source)? error,
    TResult? Function(String level, String message)? log,
  }) => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String newState, String? role)? stateChanged,
    TResult Function(int memberCount, String? masterId)? membershipChanged,
    TResult Function(String nodeId, int battery, double cpu)? heartbeatReceived,
    TResult Function(String taskId, String status)? taskUpdate,
    TResult Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult Function(bool connected)? mqttStatus,
    TResult Function(int connectedPeers)? peerMeshStatus,
    TResult Function(List<String> line)? successionUpdated,
    TResult Function(String message, String source)? error,
    TResult Function(String level, String message)? log,
    required TResult orElse(),
  }) => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(EngineEvent_StateChanged value) stateChanged,
    required TResult Function(EngineEvent_MembershipChanged value)
    membershipChanged,
    required TResult Function(EngineEvent_HeartbeatReceived value)
    heartbeatReceived,
    required TResult Function(EngineEvent_TaskUpdate value) taskUpdate,
    required TResult Function(EngineEvent_PluginResult value) pluginResult,
    required TResult Function(EngineEvent_MqttStatus value) mqttStatus,
    required TResult Function(EngineEvent_PeerMeshStatus value) peerMeshStatus,
    required TResult Function(EngineEvent_SuccessionUpdated value)
    successionUpdated,
    required TResult Function(EngineEvent_Error value) error,
    required TResult Function(EngineEvent_Log value) log,
  }) => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(EngineEvent_StateChanged value)? stateChanged,
    TResult? Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult? Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult? Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult? Function(EngineEvent_PluginResult value)? pluginResult,
    TResult? Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult? Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult? Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult? Function(EngineEvent_Error value)? error,
    TResult? Function(EngineEvent_Log value)? log,
  }) => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(EngineEvent_StateChanged value)? stateChanged,
    TResult Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult Function(EngineEvent_PluginResult value)? pluginResult,
    TResult Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult Function(EngineEvent_Error value)? error,
    TResult Function(EngineEvent_Log value)? log,
    required TResult orElse(),
  }) => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class $EngineEventCopyWith<$Res> {
  factory $EngineEventCopyWith(
    EngineEvent value,
    $Res Function(EngineEvent) then,
  ) = _$EngineEventCopyWithImpl<$Res, EngineEvent>;
}

/// @nodoc
class _$EngineEventCopyWithImpl<$Res, $Val extends EngineEvent>
    implements $EngineEventCopyWith<$Res> {
  _$EngineEventCopyWithImpl(this._value, this._then);

  // ignore: unused_field
  final $Val _value;
  // ignore: unused_field
  final $Res Function($Val) _then;

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
}

/// @nodoc
abstract class _$$EngineEvent_StateChangedImplCopyWith<$Res> {
  factory _$$EngineEvent_StateChangedImplCopyWith(
    _$EngineEvent_StateChangedImpl value,
    $Res Function(_$EngineEvent_StateChangedImpl) then,
  ) = __$$EngineEvent_StateChangedImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String newState, String? role});
}

/// @nodoc
class __$$EngineEvent_StateChangedImplCopyWithImpl<$Res>
    extends _$EngineEventCopyWithImpl<$Res, _$EngineEvent_StateChangedImpl>
    implements _$$EngineEvent_StateChangedImplCopyWith<$Res> {
  __$$EngineEvent_StateChangedImplCopyWithImpl(
    _$EngineEvent_StateChangedImpl _value,
    $Res Function(_$EngineEvent_StateChangedImpl) _then,
  ) : super(_value, _then);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({Object? newState = null, Object? role = freezed}) {
    return _then(
      _$EngineEvent_StateChangedImpl(
        newState: null == newState
            ? _value.newState
            : newState // ignore: cast_nullable_to_non_nullable
                  as String,
        role: freezed == role
            ? _value.role
            : role // ignore: cast_nullable_to_non_nullable
                  as String?,
      ),
    );
  }
}

/// @nodoc

class _$EngineEvent_StateChangedImpl extends EngineEvent_StateChanged {
  const _$EngineEvent_StateChangedImpl({required this.newState, this.role})
    : super._();

  @override
  final String newState;
  @override
  final String? role;

  @override
  String toString() {
    return 'EngineEvent.stateChanged(newState: $newState, role: $role)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$EngineEvent_StateChangedImpl &&
            (identical(other.newState, newState) ||
                other.newState == newState) &&
            (identical(other.role, role) || other.role == role));
  }

  @override
  int get hashCode => Object.hash(runtimeType, newState, role);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$EngineEvent_StateChangedImplCopyWith<_$EngineEvent_StateChangedImpl>
  get copyWith =>
      __$$EngineEvent_StateChangedImplCopyWithImpl<
        _$EngineEvent_StateChangedImpl
      >(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String newState, String? role) stateChanged,
    required TResult Function(int memberCount, String? masterId)
    membershipChanged,
    required TResult Function(String nodeId, int battery, double cpu)
    heartbeatReceived,
    required TResult Function(String taskId, String status) taskUpdate,
    required TResult Function(String pluginId, String action, bool success)
    pluginResult,
    required TResult Function(bool connected) mqttStatus,
    required TResult Function(int connectedPeers) peerMeshStatus,
    required TResult Function(List<String> line) successionUpdated,
    required TResult Function(String message, String source) error,
    required TResult Function(String level, String message) log,
  }) {
    return stateChanged(newState, role);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String newState, String? role)? stateChanged,
    TResult? Function(int memberCount, String? masterId)? membershipChanged,
    TResult? Function(String nodeId, int battery, double cpu)?
    heartbeatReceived,
    TResult? Function(String taskId, String status)? taskUpdate,
    TResult? Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult? Function(bool connected)? mqttStatus,
    TResult? Function(int connectedPeers)? peerMeshStatus,
    TResult? Function(List<String> line)? successionUpdated,
    TResult? Function(String message, String source)? error,
    TResult? Function(String level, String message)? log,
  }) {
    return stateChanged?.call(newState, role);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String newState, String? role)? stateChanged,
    TResult Function(int memberCount, String? masterId)? membershipChanged,
    TResult Function(String nodeId, int battery, double cpu)? heartbeatReceived,
    TResult Function(String taskId, String status)? taskUpdate,
    TResult Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult Function(bool connected)? mqttStatus,
    TResult Function(int connectedPeers)? peerMeshStatus,
    TResult Function(List<String> line)? successionUpdated,
    TResult Function(String message, String source)? error,
    TResult Function(String level, String message)? log,
    required TResult orElse(),
  }) {
    if (stateChanged != null) {
      return stateChanged(newState, role);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(EngineEvent_StateChanged value) stateChanged,
    required TResult Function(EngineEvent_MembershipChanged value)
    membershipChanged,
    required TResult Function(EngineEvent_HeartbeatReceived value)
    heartbeatReceived,
    required TResult Function(EngineEvent_TaskUpdate value) taskUpdate,
    required TResult Function(EngineEvent_PluginResult value) pluginResult,
    required TResult Function(EngineEvent_MqttStatus value) mqttStatus,
    required TResult Function(EngineEvent_PeerMeshStatus value) peerMeshStatus,
    required TResult Function(EngineEvent_SuccessionUpdated value)
    successionUpdated,
    required TResult Function(EngineEvent_Error value) error,
    required TResult Function(EngineEvent_Log value) log,
  }) {
    return stateChanged(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(EngineEvent_StateChanged value)? stateChanged,
    TResult? Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult? Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult? Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult? Function(EngineEvent_PluginResult value)? pluginResult,
    TResult? Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult? Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult? Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult? Function(EngineEvent_Error value)? error,
    TResult? Function(EngineEvent_Log value)? log,
  }) {
    return stateChanged?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(EngineEvent_StateChanged value)? stateChanged,
    TResult Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult Function(EngineEvent_PluginResult value)? pluginResult,
    TResult Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult Function(EngineEvent_Error value)? error,
    TResult Function(EngineEvent_Log value)? log,
    required TResult orElse(),
  }) {
    if (stateChanged != null) {
      return stateChanged(this);
    }
    return orElse();
  }
}

abstract class EngineEvent_StateChanged extends EngineEvent {
  const factory EngineEvent_StateChanged({
    required final String newState,
    final String? role,
  }) = _$EngineEvent_StateChangedImpl;
  const EngineEvent_StateChanged._() : super._();

  String get newState;
  String? get role;

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$EngineEvent_StateChangedImplCopyWith<_$EngineEvent_StateChangedImpl>
  get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$EngineEvent_MembershipChangedImplCopyWith<$Res> {
  factory _$$EngineEvent_MembershipChangedImplCopyWith(
    _$EngineEvent_MembershipChangedImpl value,
    $Res Function(_$EngineEvent_MembershipChangedImpl) then,
  ) = __$$EngineEvent_MembershipChangedImplCopyWithImpl<$Res>;
  @useResult
  $Res call({int memberCount, String? masterId});
}

/// @nodoc
class __$$EngineEvent_MembershipChangedImplCopyWithImpl<$Res>
    extends _$EngineEventCopyWithImpl<$Res, _$EngineEvent_MembershipChangedImpl>
    implements _$$EngineEvent_MembershipChangedImplCopyWith<$Res> {
  __$$EngineEvent_MembershipChangedImplCopyWithImpl(
    _$EngineEvent_MembershipChangedImpl _value,
    $Res Function(_$EngineEvent_MembershipChangedImpl) _then,
  ) : super(_value, _then);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({Object? memberCount = null, Object? masterId = freezed}) {
    return _then(
      _$EngineEvent_MembershipChangedImpl(
        memberCount: null == memberCount
            ? _value.memberCount
            : memberCount // ignore: cast_nullable_to_non_nullable
                  as int,
        masterId: freezed == masterId
            ? _value.masterId
            : masterId // ignore: cast_nullable_to_non_nullable
                  as String?,
      ),
    );
  }
}

/// @nodoc

class _$EngineEvent_MembershipChangedImpl
    extends EngineEvent_MembershipChanged {
  const _$EngineEvent_MembershipChangedImpl({
    required this.memberCount,
    this.masterId,
  }) : super._();

  @override
  final int memberCount;
  @override
  final String? masterId;

  @override
  String toString() {
    return 'EngineEvent.membershipChanged(memberCount: $memberCount, masterId: $masterId)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$EngineEvent_MembershipChangedImpl &&
            (identical(other.memberCount, memberCount) ||
                other.memberCount == memberCount) &&
            (identical(other.masterId, masterId) ||
                other.masterId == masterId));
  }

  @override
  int get hashCode => Object.hash(runtimeType, memberCount, masterId);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$EngineEvent_MembershipChangedImplCopyWith<
    _$EngineEvent_MembershipChangedImpl
  >
  get copyWith =>
      __$$EngineEvent_MembershipChangedImplCopyWithImpl<
        _$EngineEvent_MembershipChangedImpl
      >(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String newState, String? role) stateChanged,
    required TResult Function(int memberCount, String? masterId)
    membershipChanged,
    required TResult Function(String nodeId, int battery, double cpu)
    heartbeatReceived,
    required TResult Function(String taskId, String status) taskUpdate,
    required TResult Function(String pluginId, String action, bool success)
    pluginResult,
    required TResult Function(bool connected) mqttStatus,
    required TResult Function(int connectedPeers) peerMeshStatus,
    required TResult Function(List<String> line) successionUpdated,
    required TResult Function(String message, String source) error,
    required TResult Function(String level, String message) log,
  }) {
    return membershipChanged(memberCount, masterId);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String newState, String? role)? stateChanged,
    TResult? Function(int memberCount, String? masterId)? membershipChanged,
    TResult? Function(String nodeId, int battery, double cpu)?
    heartbeatReceived,
    TResult? Function(String taskId, String status)? taskUpdate,
    TResult? Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult? Function(bool connected)? mqttStatus,
    TResult? Function(int connectedPeers)? peerMeshStatus,
    TResult? Function(List<String> line)? successionUpdated,
    TResult? Function(String message, String source)? error,
    TResult? Function(String level, String message)? log,
  }) {
    return membershipChanged?.call(memberCount, masterId);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String newState, String? role)? stateChanged,
    TResult Function(int memberCount, String? masterId)? membershipChanged,
    TResult Function(String nodeId, int battery, double cpu)? heartbeatReceived,
    TResult Function(String taskId, String status)? taskUpdate,
    TResult Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult Function(bool connected)? mqttStatus,
    TResult Function(int connectedPeers)? peerMeshStatus,
    TResult Function(List<String> line)? successionUpdated,
    TResult Function(String message, String source)? error,
    TResult Function(String level, String message)? log,
    required TResult orElse(),
  }) {
    if (membershipChanged != null) {
      return membershipChanged(memberCount, masterId);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(EngineEvent_StateChanged value) stateChanged,
    required TResult Function(EngineEvent_MembershipChanged value)
    membershipChanged,
    required TResult Function(EngineEvent_HeartbeatReceived value)
    heartbeatReceived,
    required TResult Function(EngineEvent_TaskUpdate value) taskUpdate,
    required TResult Function(EngineEvent_PluginResult value) pluginResult,
    required TResult Function(EngineEvent_MqttStatus value) mqttStatus,
    required TResult Function(EngineEvent_PeerMeshStatus value) peerMeshStatus,
    required TResult Function(EngineEvent_SuccessionUpdated value)
    successionUpdated,
    required TResult Function(EngineEvent_Error value) error,
    required TResult Function(EngineEvent_Log value) log,
  }) {
    return membershipChanged(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(EngineEvent_StateChanged value)? stateChanged,
    TResult? Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult? Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult? Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult? Function(EngineEvent_PluginResult value)? pluginResult,
    TResult? Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult? Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult? Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult? Function(EngineEvent_Error value)? error,
    TResult? Function(EngineEvent_Log value)? log,
  }) {
    return membershipChanged?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(EngineEvent_StateChanged value)? stateChanged,
    TResult Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult Function(EngineEvent_PluginResult value)? pluginResult,
    TResult Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult Function(EngineEvent_Error value)? error,
    TResult Function(EngineEvent_Log value)? log,
    required TResult orElse(),
  }) {
    if (membershipChanged != null) {
      return membershipChanged(this);
    }
    return orElse();
  }
}

abstract class EngineEvent_MembershipChanged extends EngineEvent {
  const factory EngineEvent_MembershipChanged({
    required final int memberCount,
    final String? masterId,
  }) = _$EngineEvent_MembershipChangedImpl;
  const EngineEvent_MembershipChanged._() : super._();

  int get memberCount;
  String? get masterId;

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$EngineEvent_MembershipChangedImplCopyWith<
    _$EngineEvent_MembershipChangedImpl
  >
  get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$EngineEvent_HeartbeatReceivedImplCopyWith<$Res> {
  factory _$$EngineEvent_HeartbeatReceivedImplCopyWith(
    _$EngineEvent_HeartbeatReceivedImpl value,
    $Res Function(_$EngineEvent_HeartbeatReceivedImpl) then,
  ) = __$$EngineEvent_HeartbeatReceivedImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String nodeId, int battery, double cpu});
}

/// @nodoc
class __$$EngineEvent_HeartbeatReceivedImplCopyWithImpl<$Res>
    extends _$EngineEventCopyWithImpl<$Res, _$EngineEvent_HeartbeatReceivedImpl>
    implements _$$EngineEvent_HeartbeatReceivedImplCopyWith<$Res> {
  __$$EngineEvent_HeartbeatReceivedImplCopyWithImpl(
    _$EngineEvent_HeartbeatReceivedImpl _value,
    $Res Function(_$EngineEvent_HeartbeatReceivedImpl) _then,
  ) : super(_value, _then);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? nodeId = null,
    Object? battery = null,
    Object? cpu = null,
  }) {
    return _then(
      _$EngineEvent_HeartbeatReceivedImpl(
        nodeId: null == nodeId
            ? _value.nodeId
            : nodeId // ignore: cast_nullable_to_non_nullable
                  as String,
        battery: null == battery
            ? _value.battery
            : battery // ignore: cast_nullable_to_non_nullable
                  as int,
        cpu: null == cpu
            ? _value.cpu
            : cpu // ignore: cast_nullable_to_non_nullable
                  as double,
      ),
    );
  }
}

/// @nodoc

class _$EngineEvent_HeartbeatReceivedImpl
    extends EngineEvent_HeartbeatReceived {
  const _$EngineEvent_HeartbeatReceivedImpl({
    required this.nodeId,
    required this.battery,
    required this.cpu,
  }) : super._();

  @override
  final String nodeId;
  @override
  final int battery;
  @override
  final double cpu;

  @override
  String toString() {
    return 'EngineEvent.heartbeatReceived(nodeId: $nodeId, battery: $battery, cpu: $cpu)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$EngineEvent_HeartbeatReceivedImpl &&
            (identical(other.nodeId, nodeId) || other.nodeId == nodeId) &&
            (identical(other.battery, battery) || other.battery == battery) &&
            (identical(other.cpu, cpu) || other.cpu == cpu));
  }

  @override
  int get hashCode => Object.hash(runtimeType, nodeId, battery, cpu);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$EngineEvent_HeartbeatReceivedImplCopyWith<
    _$EngineEvent_HeartbeatReceivedImpl
  >
  get copyWith =>
      __$$EngineEvent_HeartbeatReceivedImplCopyWithImpl<
        _$EngineEvent_HeartbeatReceivedImpl
      >(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String newState, String? role) stateChanged,
    required TResult Function(int memberCount, String? masterId)
    membershipChanged,
    required TResult Function(String nodeId, int battery, double cpu)
    heartbeatReceived,
    required TResult Function(String taskId, String status) taskUpdate,
    required TResult Function(String pluginId, String action, bool success)
    pluginResult,
    required TResult Function(bool connected) mqttStatus,
    required TResult Function(int connectedPeers) peerMeshStatus,
    required TResult Function(List<String> line) successionUpdated,
    required TResult Function(String message, String source) error,
    required TResult Function(String level, String message) log,
  }) {
    return heartbeatReceived(nodeId, battery, cpu);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String newState, String? role)? stateChanged,
    TResult? Function(int memberCount, String? masterId)? membershipChanged,
    TResult? Function(String nodeId, int battery, double cpu)?
    heartbeatReceived,
    TResult? Function(String taskId, String status)? taskUpdate,
    TResult? Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult? Function(bool connected)? mqttStatus,
    TResult? Function(int connectedPeers)? peerMeshStatus,
    TResult? Function(List<String> line)? successionUpdated,
    TResult? Function(String message, String source)? error,
    TResult? Function(String level, String message)? log,
  }) {
    return heartbeatReceived?.call(nodeId, battery, cpu);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String newState, String? role)? stateChanged,
    TResult Function(int memberCount, String? masterId)? membershipChanged,
    TResult Function(String nodeId, int battery, double cpu)? heartbeatReceived,
    TResult Function(String taskId, String status)? taskUpdate,
    TResult Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult Function(bool connected)? mqttStatus,
    TResult Function(int connectedPeers)? peerMeshStatus,
    TResult Function(List<String> line)? successionUpdated,
    TResult Function(String message, String source)? error,
    TResult Function(String level, String message)? log,
    required TResult orElse(),
  }) {
    if (heartbeatReceived != null) {
      return heartbeatReceived(nodeId, battery, cpu);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(EngineEvent_StateChanged value) stateChanged,
    required TResult Function(EngineEvent_MembershipChanged value)
    membershipChanged,
    required TResult Function(EngineEvent_HeartbeatReceived value)
    heartbeatReceived,
    required TResult Function(EngineEvent_TaskUpdate value) taskUpdate,
    required TResult Function(EngineEvent_PluginResult value) pluginResult,
    required TResult Function(EngineEvent_MqttStatus value) mqttStatus,
    required TResult Function(EngineEvent_PeerMeshStatus value) peerMeshStatus,
    required TResult Function(EngineEvent_SuccessionUpdated value)
    successionUpdated,
    required TResult Function(EngineEvent_Error value) error,
    required TResult Function(EngineEvent_Log value) log,
  }) {
    return heartbeatReceived(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(EngineEvent_StateChanged value)? stateChanged,
    TResult? Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult? Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult? Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult? Function(EngineEvent_PluginResult value)? pluginResult,
    TResult? Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult? Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult? Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult? Function(EngineEvent_Error value)? error,
    TResult? Function(EngineEvent_Log value)? log,
  }) {
    return heartbeatReceived?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(EngineEvent_StateChanged value)? stateChanged,
    TResult Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult Function(EngineEvent_PluginResult value)? pluginResult,
    TResult Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult Function(EngineEvent_Error value)? error,
    TResult Function(EngineEvent_Log value)? log,
    required TResult orElse(),
  }) {
    if (heartbeatReceived != null) {
      return heartbeatReceived(this);
    }
    return orElse();
  }
}

abstract class EngineEvent_HeartbeatReceived extends EngineEvent {
  const factory EngineEvent_HeartbeatReceived({
    required final String nodeId,
    required final int battery,
    required final double cpu,
  }) = _$EngineEvent_HeartbeatReceivedImpl;
  const EngineEvent_HeartbeatReceived._() : super._();

  String get nodeId;
  int get battery;
  double get cpu;

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$EngineEvent_HeartbeatReceivedImplCopyWith<
    _$EngineEvent_HeartbeatReceivedImpl
  >
  get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$EngineEvent_TaskUpdateImplCopyWith<$Res> {
  factory _$$EngineEvent_TaskUpdateImplCopyWith(
    _$EngineEvent_TaskUpdateImpl value,
    $Res Function(_$EngineEvent_TaskUpdateImpl) then,
  ) = __$$EngineEvent_TaskUpdateImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String taskId, String status});
}

/// @nodoc
class __$$EngineEvent_TaskUpdateImplCopyWithImpl<$Res>
    extends _$EngineEventCopyWithImpl<$Res, _$EngineEvent_TaskUpdateImpl>
    implements _$$EngineEvent_TaskUpdateImplCopyWith<$Res> {
  __$$EngineEvent_TaskUpdateImplCopyWithImpl(
    _$EngineEvent_TaskUpdateImpl _value,
    $Res Function(_$EngineEvent_TaskUpdateImpl) _then,
  ) : super(_value, _then);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({Object? taskId = null, Object? status = null}) {
    return _then(
      _$EngineEvent_TaskUpdateImpl(
        taskId: null == taskId
            ? _value.taskId
            : taskId // ignore: cast_nullable_to_non_nullable
                  as String,
        status: null == status
            ? _value.status
            : status // ignore: cast_nullable_to_non_nullable
                  as String,
      ),
    );
  }
}

/// @nodoc

class _$EngineEvent_TaskUpdateImpl extends EngineEvent_TaskUpdate {
  const _$EngineEvent_TaskUpdateImpl({
    required this.taskId,
    required this.status,
  }) : super._();

  @override
  final String taskId;
  @override
  final String status;

  @override
  String toString() {
    return 'EngineEvent.taskUpdate(taskId: $taskId, status: $status)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$EngineEvent_TaskUpdateImpl &&
            (identical(other.taskId, taskId) || other.taskId == taskId) &&
            (identical(other.status, status) || other.status == status));
  }

  @override
  int get hashCode => Object.hash(runtimeType, taskId, status);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$EngineEvent_TaskUpdateImplCopyWith<_$EngineEvent_TaskUpdateImpl>
  get copyWith =>
      __$$EngineEvent_TaskUpdateImplCopyWithImpl<_$EngineEvent_TaskUpdateImpl>(
        this,
        _$identity,
      );

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String newState, String? role) stateChanged,
    required TResult Function(int memberCount, String? masterId)
    membershipChanged,
    required TResult Function(String nodeId, int battery, double cpu)
    heartbeatReceived,
    required TResult Function(String taskId, String status) taskUpdate,
    required TResult Function(String pluginId, String action, bool success)
    pluginResult,
    required TResult Function(bool connected) mqttStatus,
    required TResult Function(int connectedPeers) peerMeshStatus,
    required TResult Function(List<String> line) successionUpdated,
    required TResult Function(String message, String source) error,
    required TResult Function(String level, String message) log,
  }) {
    return taskUpdate(taskId, status);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String newState, String? role)? stateChanged,
    TResult? Function(int memberCount, String? masterId)? membershipChanged,
    TResult? Function(String nodeId, int battery, double cpu)?
    heartbeatReceived,
    TResult? Function(String taskId, String status)? taskUpdate,
    TResult? Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult? Function(bool connected)? mqttStatus,
    TResult? Function(int connectedPeers)? peerMeshStatus,
    TResult? Function(List<String> line)? successionUpdated,
    TResult? Function(String message, String source)? error,
    TResult? Function(String level, String message)? log,
  }) {
    return taskUpdate?.call(taskId, status);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String newState, String? role)? stateChanged,
    TResult Function(int memberCount, String? masterId)? membershipChanged,
    TResult Function(String nodeId, int battery, double cpu)? heartbeatReceived,
    TResult Function(String taskId, String status)? taskUpdate,
    TResult Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult Function(bool connected)? mqttStatus,
    TResult Function(int connectedPeers)? peerMeshStatus,
    TResult Function(List<String> line)? successionUpdated,
    TResult Function(String message, String source)? error,
    TResult Function(String level, String message)? log,
    required TResult orElse(),
  }) {
    if (taskUpdate != null) {
      return taskUpdate(taskId, status);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(EngineEvent_StateChanged value) stateChanged,
    required TResult Function(EngineEvent_MembershipChanged value)
    membershipChanged,
    required TResult Function(EngineEvent_HeartbeatReceived value)
    heartbeatReceived,
    required TResult Function(EngineEvent_TaskUpdate value) taskUpdate,
    required TResult Function(EngineEvent_PluginResult value) pluginResult,
    required TResult Function(EngineEvent_MqttStatus value) mqttStatus,
    required TResult Function(EngineEvent_PeerMeshStatus value) peerMeshStatus,
    required TResult Function(EngineEvent_SuccessionUpdated value)
    successionUpdated,
    required TResult Function(EngineEvent_Error value) error,
    required TResult Function(EngineEvent_Log value) log,
  }) {
    return taskUpdate(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(EngineEvent_StateChanged value)? stateChanged,
    TResult? Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult? Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult? Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult? Function(EngineEvent_PluginResult value)? pluginResult,
    TResult? Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult? Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult? Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult? Function(EngineEvent_Error value)? error,
    TResult? Function(EngineEvent_Log value)? log,
  }) {
    return taskUpdate?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(EngineEvent_StateChanged value)? stateChanged,
    TResult Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult Function(EngineEvent_PluginResult value)? pluginResult,
    TResult Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult Function(EngineEvent_Error value)? error,
    TResult Function(EngineEvent_Log value)? log,
    required TResult orElse(),
  }) {
    if (taskUpdate != null) {
      return taskUpdate(this);
    }
    return orElse();
  }
}

abstract class EngineEvent_TaskUpdate extends EngineEvent {
  const factory EngineEvent_TaskUpdate({
    required final String taskId,
    required final String status,
  }) = _$EngineEvent_TaskUpdateImpl;
  const EngineEvent_TaskUpdate._() : super._();

  String get taskId;
  String get status;

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$EngineEvent_TaskUpdateImplCopyWith<_$EngineEvent_TaskUpdateImpl>
  get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$EngineEvent_PluginResultImplCopyWith<$Res> {
  factory _$$EngineEvent_PluginResultImplCopyWith(
    _$EngineEvent_PluginResultImpl value,
    $Res Function(_$EngineEvent_PluginResultImpl) then,
  ) = __$$EngineEvent_PluginResultImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String pluginId, String action, bool success});
}

/// @nodoc
class __$$EngineEvent_PluginResultImplCopyWithImpl<$Res>
    extends _$EngineEventCopyWithImpl<$Res, _$EngineEvent_PluginResultImpl>
    implements _$$EngineEvent_PluginResultImplCopyWith<$Res> {
  __$$EngineEvent_PluginResultImplCopyWithImpl(
    _$EngineEvent_PluginResultImpl _value,
    $Res Function(_$EngineEvent_PluginResultImpl) _then,
  ) : super(_value, _then);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? pluginId = null,
    Object? action = null,
    Object? success = null,
  }) {
    return _then(
      _$EngineEvent_PluginResultImpl(
        pluginId: null == pluginId
            ? _value.pluginId
            : pluginId // ignore: cast_nullable_to_non_nullable
                  as String,
        action: null == action
            ? _value.action
            : action // ignore: cast_nullable_to_non_nullable
                  as String,
        success: null == success
            ? _value.success
            : success // ignore: cast_nullable_to_non_nullable
                  as bool,
      ),
    );
  }
}

/// @nodoc

class _$EngineEvent_PluginResultImpl extends EngineEvent_PluginResult {
  const _$EngineEvent_PluginResultImpl({
    required this.pluginId,
    required this.action,
    required this.success,
  }) : super._();

  @override
  final String pluginId;
  @override
  final String action;
  @override
  final bool success;

  @override
  String toString() {
    return 'EngineEvent.pluginResult(pluginId: $pluginId, action: $action, success: $success)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$EngineEvent_PluginResultImpl &&
            (identical(other.pluginId, pluginId) ||
                other.pluginId == pluginId) &&
            (identical(other.action, action) || other.action == action) &&
            (identical(other.success, success) || other.success == success));
  }

  @override
  int get hashCode => Object.hash(runtimeType, pluginId, action, success);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$EngineEvent_PluginResultImplCopyWith<_$EngineEvent_PluginResultImpl>
  get copyWith =>
      __$$EngineEvent_PluginResultImplCopyWithImpl<
        _$EngineEvent_PluginResultImpl
      >(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String newState, String? role) stateChanged,
    required TResult Function(int memberCount, String? masterId)
    membershipChanged,
    required TResult Function(String nodeId, int battery, double cpu)
    heartbeatReceived,
    required TResult Function(String taskId, String status) taskUpdate,
    required TResult Function(String pluginId, String action, bool success)
    pluginResult,
    required TResult Function(bool connected) mqttStatus,
    required TResult Function(int connectedPeers) peerMeshStatus,
    required TResult Function(List<String> line) successionUpdated,
    required TResult Function(String message, String source) error,
    required TResult Function(String level, String message) log,
  }) {
    return pluginResult(pluginId, action, success);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String newState, String? role)? stateChanged,
    TResult? Function(int memberCount, String? masterId)? membershipChanged,
    TResult? Function(String nodeId, int battery, double cpu)?
    heartbeatReceived,
    TResult? Function(String taskId, String status)? taskUpdate,
    TResult? Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult? Function(bool connected)? mqttStatus,
    TResult? Function(int connectedPeers)? peerMeshStatus,
    TResult? Function(List<String> line)? successionUpdated,
    TResult? Function(String message, String source)? error,
    TResult? Function(String level, String message)? log,
  }) {
    return pluginResult?.call(pluginId, action, success);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String newState, String? role)? stateChanged,
    TResult Function(int memberCount, String? masterId)? membershipChanged,
    TResult Function(String nodeId, int battery, double cpu)? heartbeatReceived,
    TResult Function(String taskId, String status)? taskUpdate,
    TResult Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult Function(bool connected)? mqttStatus,
    TResult Function(int connectedPeers)? peerMeshStatus,
    TResult Function(List<String> line)? successionUpdated,
    TResult Function(String message, String source)? error,
    TResult Function(String level, String message)? log,
    required TResult orElse(),
  }) {
    if (pluginResult != null) {
      return pluginResult(pluginId, action, success);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(EngineEvent_StateChanged value) stateChanged,
    required TResult Function(EngineEvent_MembershipChanged value)
    membershipChanged,
    required TResult Function(EngineEvent_HeartbeatReceived value)
    heartbeatReceived,
    required TResult Function(EngineEvent_TaskUpdate value) taskUpdate,
    required TResult Function(EngineEvent_PluginResult value) pluginResult,
    required TResult Function(EngineEvent_MqttStatus value) mqttStatus,
    required TResult Function(EngineEvent_PeerMeshStatus value) peerMeshStatus,
    required TResult Function(EngineEvent_SuccessionUpdated value)
    successionUpdated,
    required TResult Function(EngineEvent_Error value) error,
    required TResult Function(EngineEvent_Log value) log,
  }) {
    return pluginResult(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(EngineEvent_StateChanged value)? stateChanged,
    TResult? Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult? Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult? Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult? Function(EngineEvent_PluginResult value)? pluginResult,
    TResult? Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult? Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult? Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult? Function(EngineEvent_Error value)? error,
    TResult? Function(EngineEvent_Log value)? log,
  }) {
    return pluginResult?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(EngineEvent_StateChanged value)? stateChanged,
    TResult Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult Function(EngineEvent_PluginResult value)? pluginResult,
    TResult Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult Function(EngineEvent_Error value)? error,
    TResult Function(EngineEvent_Log value)? log,
    required TResult orElse(),
  }) {
    if (pluginResult != null) {
      return pluginResult(this);
    }
    return orElse();
  }
}

abstract class EngineEvent_PluginResult extends EngineEvent {
  const factory EngineEvent_PluginResult({
    required final String pluginId,
    required final String action,
    required final bool success,
  }) = _$EngineEvent_PluginResultImpl;
  const EngineEvent_PluginResult._() : super._();

  String get pluginId;
  String get action;
  bool get success;

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$EngineEvent_PluginResultImplCopyWith<_$EngineEvent_PluginResultImpl>
  get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$EngineEvent_MqttStatusImplCopyWith<$Res> {
  factory _$$EngineEvent_MqttStatusImplCopyWith(
    _$EngineEvent_MqttStatusImpl value,
    $Res Function(_$EngineEvent_MqttStatusImpl) then,
  ) = __$$EngineEvent_MqttStatusImplCopyWithImpl<$Res>;
  @useResult
  $Res call({bool connected});
}

/// @nodoc
class __$$EngineEvent_MqttStatusImplCopyWithImpl<$Res>
    extends _$EngineEventCopyWithImpl<$Res, _$EngineEvent_MqttStatusImpl>
    implements _$$EngineEvent_MqttStatusImplCopyWith<$Res> {
  __$$EngineEvent_MqttStatusImplCopyWithImpl(
    _$EngineEvent_MqttStatusImpl _value,
    $Res Function(_$EngineEvent_MqttStatusImpl) _then,
  ) : super(_value, _then);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({Object? connected = null}) {
    return _then(
      _$EngineEvent_MqttStatusImpl(
        connected: null == connected
            ? _value.connected
            : connected // ignore: cast_nullable_to_non_nullable
                  as bool,
      ),
    );
  }
}

/// @nodoc

class _$EngineEvent_MqttStatusImpl extends EngineEvent_MqttStatus {
  const _$EngineEvent_MqttStatusImpl({required this.connected}) : super._();

  @override
  final bool connected;

  @override
  String toString() {
    return 'EngineEvent.mqttStatus(connected: $connected)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$EngineEvent_MqttStatusImpl &&
            (identical(other.connected, connected) ||
                other.connected == connected));
  }

  @override
  int get hashCode => Object.hash(runtimeType, connected);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$EngineEvent_MqttStatusImplCopyWith<_$EngineEvent_MqttStatusImpl>
  get copyWith =>
      __$$EngineEvent_MqttStatusImplCopyWithImpl<_$EngineEvent_MqttStatusImpl>(
        this,
        _$identity,
      );

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String newState, String? role) stateChanged,
    required TResult Function(int memberCount, String? masterId)
    membershipChanged,
    required TResult Function(String nodeId, int battery, double cpu)
    heartbeatReceived,
    required TResult Function(String taskId, String status) taskUpdate,
    required TResult Function(String pluginId, String action, bool success)
    pluginResult,
    required TResult Function(bool connected) mqttStatus,
    required TResult Function(int connectedPeers) peerMeshStatus,
    required TResult Function(List<String> line) successionUpdated,
    required TResult Function(String message, String source) error,
    required TResult Function(String level, String message) log,
  }) {
    return mqttStatus(connected);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String newState, String? role)? stateChanged,
    TResult? Function(int memberCount, String? masterId)? membershipChanged,
    TResult? Function(String nodeId, int battery, double cpu)?
    heartbeatReceived,
    TResult? Function(String taskId, String status)? taskUpdate,
    TResult? Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult? Function(bool connected)? mqttStatus,
    TResult? Function(int connectedPeers)? peerMeshStatus,
    TResult? Function(List<String> line)? successionUpdated,
    TResult? Function(String message, String source)? error,
    TResult? Function(String level, String message)? log,
  }) {
    return mqttStatus?.call(connected);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String newState, String? role)? stateChanged,
    TResult Function(int memberCount, String? masterId)? membershipChanged,
    TResult Function(String nodeId, int battery, double cpu)? heartbeatReceived,
    TResult Function(String taskId, String status)? taskUpdate,
    TResult Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult Function(bool connected)? mqttStatus,
    TResult Function(int connectedPeers)? peerMeshStatus,
    TResult Function(List<String> line)? successionUpdated,
    TResult Function(String message, String source)? error,
    TResult Function(String level, String message)? log,
    required TResult orElse(),
  }) {
    if (mqttStatus != null) {
      return mqttStatus(connected);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(EngineEvent_StateChanged value) stateChanged,
    required TResult Function(EngineEvent_MembershipChanged value)
    membershipChanged,
    required TResult Function(EngineEvent_HeartbeatReceived value)
    heartbeatReceived,
    required TResult Function(EngineEvent_TaskUpdate value) taskUpdate,
    required TResult Function(EngineEvent_PluginResult value) pluginResult,
    required TResult Function(EngineEvent_MqttStatus value) mqttStatus,
    required TResult Function(EngineEvent_PeerMeshStatus value) peerMeshStatus,
    required TResult Function(EngineEvent_SuccessionUpdated value)
    successionUpdated,
    required TResult Function(EngineEvent_Error value) error,
    required TResult Function(EngineEvent_Log value) log,
  }) {
    return mqttStatus(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(EngineEvent_StateChanged value)? stateChanged,
    TResult? Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult? Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult? Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult? Function(EngineEvent_PluginResult value)? pluginResult,
    TResult? Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult? Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult? Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult? Function(EngineEvent_Error value)? error,
    TResult? Function(EngineEvent_Log value)? log,
  }) {
    return mqttStatus?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(EngineEvent_StateChanged value)? stateChanged,
    TResult Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult Function(EngineEvent_PluginResult value)? pluginResult,
    TResult Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult Function(EngineEvent_Error value)? error,
    TResult Function(EngineEvent_Log value)? log,
    required TResult orElse(),
  }) {
    if (mqttStatus != null) {
      return mqttStatus(this);
    }
    return orElse();
  }
}

abstract class EngineEvent_MqttStatus extends EngineEvent {
  const factory EngineEvent_MqttStatus({required final bool connected}) =
      _$EngineEvent_MqttStatusImpl;
  const EngineEvent_MqttStatus._() : super._();

  bool get connected;

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$EngineEvent_MqttStatusImplCopyWith<_$EngineEvent_MqttStatusImpl>
  get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$EngineEvent_PeerMeshStatusImplCopyWith<$Res> {
  factory _$$EngineEvent_PeerMeshStatusImplCopyWith(
    _$EngineEvent_PeerMeshStatusImpl value,
    $Res Function(_$EngineEvent_PeerMeshStatusImpl) then,
  ) = __$$EngineEvent_PeerMeshStatusImplCopyWithImpl<$Res>;
  @useResult
  $Res call({int connectedPeers});
}

/// @nodoc
class __$$EngineEvent_PeerMeshStatusImplCopyWithImpl<$Res>
    extends _$EngineEventCopyWithImpl<$Res, _$EngineEvent_PeerMeshStatusImpl>
    implements _$$EngineEvent_PeerMeshStatusImplCopyWith<$Res> {
  __$$EngineEvent_PeerMeshStatusImplCopyWithImpl(
    _$EngineEvent_PeerMeshStatusImpl _value,
    $Res Function(_$EngineEvent_PeerMeshStatusImpl) _then,
  ) : super(_value, _then);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({Object? connectedPeers = null}) {
    return _then(
      _$EngineEvent_PeerMeshStatusImpl(
        connectedPeers: null == connectedPeers
            ? _value.connectedPeers
            : connectedPeers // ignore: cast_nullable_to_non_nullable
                  as int,
      ),
    );
  }
}

/// @nodoc

class _$EngineEvent_PeerMeshStatusImpl extends EngineEvent_PeerMeshStatus {
  const _$EngineEvent_PeerMeshStatusImpl({required this.connectedPeers})
    : super._();

  @override
  final int connectedPeers;

  @override
  String toString() {
    return 'EngineEvent.peerMeshStatus(connectedPeers: $connectedPeers)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$EngineEvent_PeerMeshStatusImpl &&
            (identical(other.connectedPeers, connectedPeers) ||
                other.connectedPeers == connectedPeers));
  }

  @override
  int get hashCode => Object.hash(runtimeType, connectedPeers);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$EngineEvent_PeerMeshStatusImplCopyWith<_$EngineEvent_PeerMeshStatusImpl>
  get copyWith =>
      __$$EngineEvent_PeerMeshStatusImplCopyWithImpl<
        _$EngineEvent_PeerMeshStatusImpl
      >(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String newState, String? role) stateChanged,
    required TResult Function(int memberCount, String? masterId)
    membershipChanged,
    required TResult Function(String nodeId, int battery, double cpu)
    heartbeatReceived,
    required TResult Function(String taskId, String status) taskUpdate,
    required TResult Function(String pluginId, String action, bool success)
    pluginResult,
    required TResult Function(bool connected) mqttStatus,
    required TResult Function(int connectedPeers) peerMeshStatus,
    required TResult Function(List<String> line) successionUpdated,
    required TResult Function(String message, String source) error,
    required TResult Function(String level, String message) log,
  }) {
    return peerMeshStatus(connectedPeers);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String newState, String? role)? stateChanged,
    TResult? Function(int memberCount, String? masterId)? membershipChanged,
    TResult? Function(String nodeId, int battery, double cpu)?
    heartbeatReceived,
    TResult? Function(String taskId, String status)? taskUpdate,
    TResult? Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult? Function(bool connected)? mqttStatus,
    TResult? Function(int connectedPeers)? peerMeshStatus,
    TResult? Function(List<String> line)? successionUpdated,
    TResult? Function(String message, String source)? error,
    TResult? Function(String level, String message)? log,
  }) {
    return peerMeshStatus?.call(connectedPeers);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String newState, String? role)? stateChanged,
    TResult Function(int memberCount, String? masterId)? membershipChanged,
    TResult Function(String nodeId, int battery, double cpu)? heartbeatReceived,
    TResult Function(String taskId, String status)? taskUpdate,
    TResult Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult Function(bool connected)? mqttStatus,
    TResult Function(int connectedPeers)? peerMeshStatus,
    TResult Function(List<String> line)? successionUpdated,
    TResult Function(String message, String source)? error,
    TResult Function(String level, String message)? log,
    required TResult orElse(),
  }) {
    if (peerMeshStatus != null) {
      return peerMeshStatus(connectedPeers);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(EngineEvent_StateChanged value) stateChanged,
    required TResult Function(EngineEvent_MembershipChanged value)
    membershipChanged,
    required TResult Function(EngineEvent_HeartbeatReceived value)
    heartbeatReceived,
    required TResult Function(EngineEvent_TaskUpdate value) taskUpdate,
    required TResult Function(EngineEvent_PluginResult value) pluginResult,
    required TResult Function(EngineEvent_MqttStatus value) mqttStatus,
    required TResult Function(EngineEvent_PeerMeshStatus value) peerMeshStatus,
    required TResult Function(EngineEvent_SuccessionUpdated value)
    successionUpdated,
    required TResult Function(EngineEvent_Error value) error,
    required TResult Function(EngineEvent_Log value) log,
  }) {
    return peerMeshStatus(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(EngineEvent_StateChanged value)? stateChanged,
    TResult? Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult? Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult? Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult? Function(EngineEvent_PluginResult value)? pluginResult,
    TResult? Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult? Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult? Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult? Function(EngineEvent_Error value)? error,
    TResult? Function(EngineEvent_Log value)? log,
  }) {
    return peerMeshStatus?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(EngineEvent_StateChanged value)? stateChanged,
    TResult Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult Function(EngineEvent_PluginResult value)? pluginResult,
    TResult Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult Function(EngineEvent_Error value)? error,
    TResult Function(EngineEvent_Log value)? log,
    required TResult orElse(),
  }) {
    if (peerMeshStatus != null) {
      return peerMeshStatus(this);
    }
    return orElse();
  }
}

abstract class EngineEvent_PeerMeshStatus extends EngineEvent {
  const factory EngineEvent_PeerMeshStatus({
    required final int connectedPeers,
  }) = _$EngineEvent_PeerMeshStatusImpl;
  const EngineEvent_PeerMeshStatus._() : super._();

  int get connectedPeers;

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$EngineEvent_PeerMeshStatusImplCopyWith<_$EngineEvent_PeerMeshStatusImpl>
  get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$EngineEvent_SuccessionUpdatedImplCopyWith<$Res> {
  factory _$$EngineEvent_SuccessionUpdatedImplCopyWith(
    _$EngineEvent_SuccessionUpdatedImpl value,
    $Res Function(_$EngineEvent_SuccessionUpdatedImpl) then,
  ) = __$$EngineEvent_SuccessionUpdatedImplCopyWithImpl<$Res>;
  @useResult
  $Res call({List<String> line});
}

/// @nodoc
class __$$EngineEvent_SuccessionUpdatedImplCopyWithImpl<$Res>
    extends _$EngineEventCopyWithImpl<$Res, _$EngineEvent_SuccessionUpdatedImpl>
    implements _$$EngineEvent_SuccessionUpdatedImplCopyWith<$Res> {
  __$$EngineEvent_SuccessionUpdatedImplCopyWithImpl(
    _$EngineEvent_SuccessionUpdatedImpl _value,
    $Res Function(_$EngineEvent_SuccessionUpdatedImpl) _then,
  ) : super(_value, _then);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({Object? line = null}) {
    return _then(
      _$EngineEvent_SuccessionUpdatedImpl(
        line: null == line
            ? _value._line
            : line // ignore: cast_nullable_to_non_nullable
                  as List<String>,
      ),
    );
  }
}

/// @nodoc

class _$EngineEvent_SuccessionUpdatedImpl
    extends EngineEvent_SuccessionUpdated {
  const _$EngineEvent_SuccessionUpdatedImpl({required final List<String> line})
    : _line = line,
      super._();

  final List<String> _line;
  @override
  List<String> get line {
    if (_line is EqualUnmodifiableListView) return _line;
    // ignore: implicit_dynamic_type
    return EqualUnmodifiableListView(_line);
  }

  @override
  String toString() {
    return 'EngineEvent.successionUpdated(line: $line)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$EngineEvent_SuccessionUpdatedImpl &&
            const DeepCollectionEquality().equals(other._line, _line));
  }

  @override
  int get hashCode =>
      Object.hash(runtimeType, const DeepCollectionEquality().hash(_line));

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$EngineEvent_SuccessionUpdatedImplCopyWith<
    _$EngineEvent_SuccessionUpdatedImpl
  >
  get copyWith =>
      __$$EngineEvent_SuccessionUpdatedImplCopyWithImpl<
        _$EngineEvent_SuccessionUpdatedImpl
      >(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String newState, String? role) stateChanged,
    required TResult Function(int memberCount, String? masterId)
    membershipChanged,
    required TResult Function(String nodeId, int battery, double cpu)
    heartbeatReceived,
    required TResult Function(String taskId, String status) taskUpdate,
    required TResult Function(String pluginId, String action, bool success)
    pluginResult,
    required TResult Function(bool connected) mqttStatus,
    required TResult Function(int connectedPeers) peerMeshStatus,
    required TResult Function(List<String> line) successionUpdated,
    required TResult Function(String message, String source) error,
    required TResult Function(String level, String message) log,
  }) {
    return successionUpdated(line);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String newState, String? role)? stateChanged,
    TResult? Function(int memberCount, String? masterId)? membershipChanged,
    TResult? Function(String nodeId, int battery, double cpu)?
    heartbeatReceived,
    TResult? Function(String taskId, String status)? taskUpdate,
    TResult? Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult? Function(bool connected)? mqttStatus,
    TResult? Function(int connectedPeers)? peerMeshStatus,
    TResult? Function(List<String> line)? successionUpdated,
    TResult? Function(String message, String source)? error,
    TResult? Function(String level, String message)? log,
  }) {
    return successionUpdated?.call(line);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String newState, String? role)? stateChanged,
    TResult Function(int memberCount, String? masterId)? membershipChanged,
    TResult Function(String nodeId, int battery, double cpu)? heartbeatReceived,
    TResult Function(String taskId, String status)? taskUpdate,
    TResult Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult Function(bool connected)? mqttStatus,
    TResult Function(int connectedPeers)? peerMeshStatus,
    TResult Function(List<String> line)? successionUpdated,
    TResult Function(String message, String source)? error,
    TResult Function(String level, String message)? log,
    required TResult orElse(),
  }) {
    if (successionUpdated != null) {
      return successionUpdated(line);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(EngineEvent_StateChanged value) stateChanged,
    required TResult Function(EngineEvent_MembershipChanged value)
    membershipChanged,
    required TResult Function(EngineEvent_HeartbeatReceived value)
    heartbeatReceived,
    required TResult Function(EngineEvent_TaskUpdate value) taskUpdate,
    required TResult Function(EngineEvent_PluginResult value) pluginResult,
    required TResult Function(EngineEvent_MqttStatus value) mqttStatus,
    required TResult Function(EngineEvent_PeerMeshStatus value) peerMeshStatus,
    required TResult Function(EngineEvent_SuccessionUpdated value)
    successionUpdated,
    required TResult Function(EngineEvent_Error value) error,
    required TResult Function(EngineEvent_Log value) log,
  }) {
    return successionUpdated(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(EngineEvent_StateChanged value)? stateChanged,
    TResult? Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult? Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult? Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult? Function(EngineEvent_PluginResult value)? pluginResult,
    TResult? Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult? Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult? Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult? Function(EngineEvent_Error value)? error,
    TResult? Function(EngineEvent_Log value)? log,
  }) {
    return successionUpdated?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(EngineEvent_StateChanged value)? stateChanged,
    TResult Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult Function(EngineEvent_PluginResult value)? pluginResult,
    TResult Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult Function(EngineEvent_Error value)? error,
    TResult Function(EngineEvent_Log value)? log,
    required TResult orElse(),
  }) {
    if (successionUpdated != null) {
      return successionUpdated(this);
    }
    return orElse();
  }
}

abstract class EngineEvent_SuccessionUpdated extends EngineEvent {
  const factory EngineEvent_SuccessionUpdated({
    required final List<String> line,
  }) = _$EngineEvent_SuccessionUpdatedImpl;
  const EngineEvent_SuccessionUpdated._() : super._();

  List<String> get line;

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$EngineEvent_SuccessionUpdatedImplCopyWith<
    _$EngineEvent_SuccessionUpdatedImpl
  >
  get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$EngineEvent_ErrorImplCopyWith<$Res> {
  factory _$$EngineEvent_ErrorImplCopyWith(
    _$EngineEvent_ErrorImpl value,
    $Res Function(_$EngineEvent_ErrorImpl) then,
  ) = __$$EngineEvent_ErrorImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String message, String source});
}

/// @nodoc
class __$$EngineEvent_ErrorImplCopyWithImpl<$Res>
    extends _$EngineEventCopyWithImpl<$Res, _$EngineEvent_ErrorImpl>
    implements _$$EngineEvent_ErrorImplCopyWith<$Res> {
  __$$EngineEvent_ErrorImplCopyWithImpl(
    _$EngineEvent_ErrorImpl _value,
    $Res Function(_$EngineEvent_ErrorImpl) _then,
  ) : super(_value, _then);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({Object? message = null, Object? source = null}) {
    return _then(
      _$EngineEvent_ErrorImpl(
        message: null == message
            ? _value.message
            : message // ignore: cast_nullable_to_non_nullable
                  as String,
        source: null == source
            ? _value.source
            : source // ignore: cast_nullable_to_non_nullable
                  as String,
      ),
    );
  }
}

/// @nodoc

class _$EngineEvent_ErrorImpl extends EngineEvent_Error {
  const _$EngineEvent_ErrorImpl({required this.message, required this.source})
    : super._();

  @override
  final String message;
  @override
  final String source;

  @override
  String toString() {
    return 'EngineEvent.error(message: $message, source: $source)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$EngineEvent_ErrorImpl &&
            (identical(other.message, message) || other.message == message) &&
            (identical(other.source, source) || other.source == source));
  }

  @override
  int get hashCode => Object.hash(runtimeType, message, source);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$EngineEvent_ErrorImplCopyWith<_$EngineEvent_ErrorImpl> get copyWith =>
      __$$EngineEvent_ErrorImplCopyWithImpl<_$EngineEvent_ErrorImpl>(
        this,
        _$identity,
      );

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String newState, String? role) stateChanged,
    required TResult Function(int memberCount, String? masterId)
    membershipChanged,
    required TResult Function(String nodeId, int battery, double cpu)
    heartbeatReceived,
    required TResult Function(String taskId, String status) taskUpdate,
    required TResult Function(String pluginId, String action, bool success)
    pluginResult,
    required TResult Function(bool connected) mqttStatus,
    required TResult Function(int connectedPeers) peerMeshStatus,
    required TResult Function(List<String> line) successionUpdated,
    required TResult Function(String message, String source) error,
    required TResult Function(String level, String message) log,
  }) {
    return error(message, source);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String newState, String? role)? stateChanged,
    TResult? Function(int memberCount, String? masterId)? membershipChanged,
    TResult? Function(String nodeId, int battery, double cpu)?
    heartbeatReceived,
    TResult? Function(String taskId, String status)? taskUpdate,
    TResult? Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult? Function(bool connected)? mqttStatus,
    TResult? Function(int connectedPeers)? peerMeshStatus,
    TResult? Function(List<String> line)? successionUpdated,
    TResult? Function(String message, String source)? error,
    TResult? Function(String level, String message)? log,
  }) {
    return error?.call(message, source);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String newState, String? role)? stateChanged,
    TResult Function(int memberCount, String? masterId)? membershipChanged,
    TResult Function(String nodeId, int battery, double cpu)? heartbeatReceived,
    TResult Function(String taskId, String status)? taskUpdate,
    TResult Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult Function(bool connected)? mqttStatus,
    TResult Function(int connectedPeers)? peerMeshStatus,
    TResult Function(List<String> line)? successionUpdated,
    TResult Function(String message, String source)? error,
    TResult Function(String level, String message)? log,
    required TResult orElse(),
  }) {
    if (error != null) {
      return error(message, source);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(EngineEvent_StateChanged value) stateChanged,
    required TResult Function(EngineEvent_MembershipChanged value)
    membershipChanged,
    required TResult Function(EngineEvent_HeartbeatReceived value)
    heartbeatReceived,
    required TResult Function(EngineEvent_TaskUpdate value) taskUpdate,
    required TResult Function(EngineEvent_PluginResult value) pluginResult,
    required TResult Function(EngineEvent_MqttStatus value) mqttStatus,
    required TResult Function(EngineEvent_PeerMeshStatus value) peerMeshStatus,
    required TResult Function(EngineEvent_SuccessionUpdated value)
    successionUpdated,
    required TResult Function(EngineEvent_Error value) error,
    required TResult Function(EngineEvent_Log value) log,
  }) {
    return error(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(EngineEvent_StateChanged value)? stateChanged,
    TResult? Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult? Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult? Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult? Function(EngineEvent_PluginResult value)? pluginResult,
    TResult? Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult? Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult? Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult? Function(EngineEvent_Error value)? error,
    TResult? Function(EngineEvent_Log value)? log,
  }) {
    return error?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(EngineEvent_StateChanged value)? stateChanged,
    TResult Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult Function(EngineEvent_PluginResult value)? pluginResult,
    TResult Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult Function(EngineEvent_Error value)? error,
    TResult Function(EngineEvent_Log value)? log,
    required TResult orElse(),
  }) {
    if (error != null) {
      return error(this);
    }
    return orElse();
  }
}

abstract class EngineEvent_Error extends EngineEvent {
  const factory EngineEvent_Error({
    required final String message,
    required final String source,
  }) = _$EngineEvent_ErrorImpl;
  const EngineEvent_Error._() : super._();

  String get message;
  String get source;

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$EngineEvent_ErrorImplCopyWith<_$EngineEvent_ErrorImpl> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$EngineEvent_LogImplCopyWith<$Res> {
  factory _$$EngineEvent_LogImplCopyWith(
    _$EngineEvent_LogImpl value,
    $Res Function(_$EngineEvent_LogImpl) then,
  ) = __$$EngineEvent_LogImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String level, String message});
}

/// @nodoc
class __$$EngineEvent_LogImplCopyWithImpl<$Res>
    extends _$EngineEventCopyWithImpl<$Res, _$EngineEvent_LogImpl>
    implements _$$EngineEvent_LogImplCopyWith<$Res> {
  __$$EngineEvent_LogImplCopyWithImpl(
    _$EngineEvent_LogImpl _value,
    $Res Function(_$EngineEvent_LogImpl) _then,
  ) : super(_value, _then);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({Object? level = null, Object? message = null}) {
    return _then(
      _$EngineEvent_LogImpl(
        level: null == level
            ? _value.level
            : level // ignore: cast_nullable_to_non_nullable
                  as String,
        message: null == message
            ? _value.message
            : message // ignore: cast_nullable_to_non_nullable
                  as String,
      ),
    );
  }
}

/// @nodoc

class _$EngineEvent_LogImpl extends EngineEvent_Log {
  const _$EngineEvent_LogImpl({required this.level, required this.message})
    : super._();

  @override
  final String level;
  @override
  final String message;

  @override
  String toString() {
    return 'EngineEvent.log(level: $level, message: $message)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$EngineEvent_LogImpl &&
            (identical(other.level, level) || other.level == level) &&
            (identical(other.message, message) || other.message == message));
  }

  @override
  int get hashCode => Object.hash(runtimeType, level, message);

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$EngineEvent_LogImplCopyWith<_$EngineEvent_LogImpl> get copyWith =>
      __$$EngineEvent_LogImplCopyWithImpl<_$EngineEvent_LogImpl>(
        this,
        _$identity,
      );

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String newState, String? role) stateChanged,
    required TResult Function(int memberCount, String? masterId)
    membershipChanged,
    required TResult Function(String nodeId, int battery, double cpu)
    heartbeatReceived,
    required TResult Function(String taskId, String status) taskUpdate,
    required TResult Function(String pluginId, String action, bool success)
    pluginResult,
    required TResult Function(bool connected) mqttStatus,
    required TResult Function(int connectedPeers) peerMeshStatus,
    required TResult Function(List<String> line) successionUpdated,
    required TResult Function(String message, String source) error,
    required TResult Function(String level, String message) log,
  }) {
    return log(level, message);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String newState, String? role)? stateChanged,
    TResult? Function(int memberCount, String? masterId)? membershipChanged,
    TResult? Function(String nodeId, int battery, double cpu)?
    heartbeatReceived,
    TResult? Function(String taskId, String status)? taskUpdate,
    TResult? Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult? Function(bool connected)? mqttStatus,
    TResult? Function(int connectedPeers)? peerMeshStatus,
    TResult? Function(List<String> line)? successionUpdated,
    TResult? Function(String message, String source)? error,
    TResult? Function(String level, String message)? log,
  }) {
    return log?.call(level, message);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String newState, String? role)? stateChanged,
    TResult Function(int memberCount, String? masterId)? membershipChanged,
    TResult Function(String nodeId, int battery, double cpu)? heartbeatReceived,
    TResult Function(String taskId, String status)? taskUpdate,
    TResult Function(String pluginId, String action, bool success)?
    pluginResult,
    TResult Function(bool connected)? mqttStatus,
    TResult Function(int connectedPeers)? peerMeshStatus,
    TResult Function(List<String> line)? successionUpdated,
    TResult Function(String message, String source)? error,
    TResult Function(String level, String message)? log,
    required TResult orElse(),
  }) {
    if (log != null) {
      return log(level, message);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(EngineEvent_StateChanged value) stateChanged,
    required TResult Function(EngineEvent_MembershipChanged value)
    membershipChanged,
    required TResult Function(EngineEvent_HeartbeatReceived value)
    heartbeatReceived,
    required TResult Function(EngineEvent_TaskUpdate value) taskUpdate,
    required TResult Function(EngineEvent_PluginResult value) pluginResult,
    required TResult Function(EngineEvent_MqttStatus value) mqttStatus,
    required TResult Function(EngineEvent_PeerMeshStatus value) peerMeshStatus,
    required TResult Function(EngineEvent_SuccessionUpdated value)
    successionUpdated,
    required TResult Function(EngineEvent_Error value) error,
    required TResult Function(EngineEvent_Log value) log,
  }) {
    return log(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(EngineEvent_StateChanged value)? stateChanged,
    TResult? Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult? Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult? Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult? Function(EngineEvent_PluginResult value)? pluginResult,
    TResult? Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult? Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult? Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult? Function(EngineEvent_Error value)? error,
    TResult? Function(EngineEvent_Log value)? log,
  }) {
    return log?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(EngineEvent_StateChanged value)? stateChanged,
    TResult Function(EngineEvent_MembershipChanged value)? membershipChanged,
    TResult Function(EngineEvent_HeartbeatReceived value)? heartbeatReceived,
    TResult Function(EngineEvent_TaskUpdate value)? taskUpdate,
    TResult Function(EngineEvent_PluginResult value)? pluginResult,
    TResult Function(EngineEvent_MqttStatus value)? mqttStatus,
    TResult Function(EngineEvent_PeerMeshStatus value)? peerMeshStatus,
    TResult Function(EngineEvent_SuccessionUpdated value)? successionUpdated,
    TResult Function(EngineEvent_Error value)? error,
    TResult Function(EngineEvent_Log value)? log,
    required TResult orElse(),
  }) {
    if (log != null) {
      return log(this);
    }
    return orElse();
  }
}

abstract class EngineEvent_Log extends EngineEvent {
  const factory EngineEvent_Log({
    required final String level,
    required final String message,
  }) = _$EngineEvent_LogImpl;
  const EngineEvent_Log._() : super._();

  String get level;
  String get message;

  /// Create a copy of EngineEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$EngineEvent_LogImplCopyWith<_$EngineEvent_LogImpl> get copyWith =>
      throw _privateConstructorUsedError;
}
