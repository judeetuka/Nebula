package com.nebula.nebula_node.platform

import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification

/**
 * NotificationListenerService that monitors all device notifications for the Nebula platform bridge.
 *
 * The service instance is held in a companion object so that [NebulaPlatformBridge]
 * can call [getActiveNotifications] and [cancelNotification] on it.
 *
 * Unlike a custom collection approach, this relies on the system's [getActiveNotifications]
 * method which returns the live set of active notifications. This avoids synchronization
 * issues and stale references.
 */
class NebulaNotificationListener : NotificationListenerService() {

    companion object {
        /**
         * The currently active listener instance, or null if the service is not running.
         */
        @Volatile
        var instance: NebulaNotificationListener? = null
            private set
    }

    override fun onNotificationPosted(sbn: StatusBarNotification?) {
        // No-op: we use getActiveNotifications() for live queries
        // rather than maintaining our own mutable list.
    }

    override fun onNotificationRemoved(sbn: StatusBarNotification?) {
        // No-op: same rationale as onNotificationPosted.
    }

    override fun onListenerConnected() {
        super.onListenerConnected()
        instance = this
    }

    override fun onListenerDisconnected() {
        instance = null
        super.onListenerDisconnected()
    }
}
