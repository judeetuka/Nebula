package com.nebula.nebula_node.platform

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.app.Service
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import android.util.Log
import androidx.core.app.NotificationCompat

/**
 * Dedicated foreground service for keeping the NEBULA engine alive.
 *
 * Holds a partial wake lock to prevent CPU sleep, ensures the
 * [NebulaContentObserverService] is running, and maintains a persistent
 * notification indicating the node's connection state.
 */
class NebulaForegroundService : Service() {

    companion object {
        private const val TAG = "NebulaForegroundSvc"
        private const val NOTIFICATION_ID = 3003
        private const val CHANNEL_ID = "nebula_foreground"
        private const val WAKE_LOCK_TAG = "nebula:foreground_service"

        @Volatile
        var instance: NebulaForegroundService? = null
            private set
    }

    private var wakeLock: PowerManager.WakeLock? = null

    override fun onCreate() {
        super.onCreate()
        instance = this
        createNotificationChannel()
        Log.i(TAG, "NebulaForegroundService created")
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startForeground(NOTIFICATION_ID, buildNotification())
        acquireWakeLock()
        startContentObserverService()

        Log.i(TAG, "NebulaForegroundService started")
        return START_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onDestroy() {
        releaseWakeLock()
        instance = null
        Log.i(TAG, "NebulaForegroundService destroyed")
        super.onDestroy()
    }

    private fun acquireWakeLock() {
        if (wakeLock != null) return

        val powerManager = getSystemService(Context.POWER_SERVICE) as PowerManager
        wakeLock = powerManager.newWakeLock(
            PowerManager.PARTIAL_WAKE_LOCK,
            WAKE_LOCK_TAG
        ).apply {
            acquire()
        }
        Log.d(TAG, "Wake lock acquired")
    }

    private fun releaseWakeLock() {
        wakeLock?.let {
            if (it.isHeld) {
                it.release()
                Log.d(TAG, "Wake lock released")
            }
        }
        wakeLock = null
    }

    /**
     * Ensure the ContentObserverService is running alongside this service.
     */
    private fun startContentObserverService() {
        if (NebulaContentObserverService.instance != null) return

        val observerIntent = Intent(this, NebulaContentObserverService::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(observerIntent)
        } else {
            startService(observerIntent)
        }
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "NEBULA Node",
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "NEBULA node engine is running"
            }
            val manager = getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(channel)
        }
    }

    private fun buildNotification(): Notification {
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle("NEBULA Node")
            .setContentText("Connected")
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setOngoing(true)
            .build()
    }
}
